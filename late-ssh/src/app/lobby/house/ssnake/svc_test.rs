use super::*;
use crate::app::lobby::house::ssnake::levels::open_test_arena;
use crate::app::lobby::house::ssnake::settings::{
    SSNAKE_BONUS_FOOD_MULTIPLIER, SSNAKE_DEATH_SHRINK_PER_TICK, SSNAKE_EDGE_BONUS_CHIPS,
    SSNAKE_FOOD_CHIPS, SSNAKE_MIN_RESPAWN_LENGTH, SSNAKE_SKIP_VOTE_TTL,
};

/// A running arena on a deterministic walled 30x20 board, with both snakes
/// parked at known safe cells and no food down.
fn arena_state() -> (SharedState, Uuid, Uuid, u64) {
    let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    state.sit(a);
    state.sit(b);
    let generation = state.wake_arena().expect("arena should wake").generation;
    state.level = Some(Arc::new(open_test_arena(30, 20)));
    state.players[0].body = VecDeque::from([Pos { x: 5, y: 5 }]);
    state.players[0].pending_growth = 0;
    state.players[1].body = VecDeque::from([Pos { x: 20, y: 10 }]);
    state.players[1].pending_growth = 0;
    state.point = None;
    // Swapping the level out by hand leaves the edge count from the arena
    // that was rolled at construction; clear it so a test that does not opt
    // into the wall bonus is not quietly paying one.
    state.food_wall_edges = 0;
    (state, a, b, generation)
}

#[test]
fn a_single_snake_keeps_the_arena_running() {
    let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
    let user = Uuid::now_v7();
    assert_eq!(state.phase, SsnakePhase::Idle);
    assert!(state.level.is_some(), "the board exists before anyone sits");

    assert_eq!(state.sit(user), Some(0));
    assert!(state.wake_arena().is_some(), "one snake is enough");
    assert_eq!(state.phase, SsnakePhase::Running);
    assert_eq!(state.players[0].body.len(), 1);
    assert_eq!(state.players[0].motion, Motion::Idle);
    assert!(state.point.is_some());
}

#[test]
fn arena_sleeps_when_the_last_snake_stands_up() {
    let (mut state, a, b, generation) = arena_state();
    state.leave(a, true);
    assert_eq!(state.phase, SsnakePhase::Running, "one snake still playing");
    state.leave(b, true);
    assert_eq!(state.phase, SsnakePhase::Idle);
    assert!(
        state.tick(generation).next_millis.is_none(),
        "the stale loop must retire itself"
    );
}

#[test]
fn joining_mid_flight_lands_clear_of_the_other_snakes() {
    let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
    state.level = Some(Arc::new(open_test_arena(30, 20)));
    state.point = None;
    state.sit(Uuid::now_v7());
    state.sit(Uuid::now_v7());
    // Pin the incumbents into one corner; the newcomer must not join them.
    state.players[0].body = VecDeque::from([Pos { x: 2, y: 2 }]);
    state.players[1].body = VecDeque::from([Pos { x: 3, y: 3 }]);

    let settings = SsnakeTableSettings {
        seats: 3,
        ..Default::default()
    };
    state.settings = settings;
    state.seat_limit = 3;
    assert_eq!(state.sit(Uuid::now_v7()), Some(2));

    let spawn = state.players[2].body[0];
    let clearance = [state.players[0].body[0], state.players[1].body[0]]
        .iter()
        .map(|head| torus_distance(spawn, *head, 30, 20))
        .min()
        .unwrap();
    assert!(
        clearance >= 6,
        "newcomer spawned {clearance} cells from the pack at {spawn:?}"
    );
}

#[test]
fn only_moving_snakes_count_toward_the_multiplier() {
    let (mut state, a, _, _) = arena_state();
    assert_eq!(state.moving_snakes(), 0, "both snakes are parked");
    state.steer(a, Direction::Right);
    assert_eq!(state.moving_snakes(), 1);
    state.players[1].motion = Motion::Moving(Direction::Left);
    assert_eq!(state.moving_snakes(), 2);
    // A dying snake is off the board as far as payouts are concerned.
    state.players[1].motion = Motion::Dying;
    assert_eq!(state.moving_snakes(), 1);
}

#[test]
fn food_pays_the_eater_scaled_by_moving_snakes() {
    let (mut state, a, _, generation) = arena_state();
    state.points_left = 5;
    state.bonus_food = false;
    state.point = Some(Pos { x: 6, y: 5 });
    state.players[1].motion = Motion::Moving(Direction::Left);
    state.steer(a, Direction::Right);

    state.tick(generation);
    assert_eq!(
        state.players[0].chips,
        SSNAKE_FOOD_CHIPS * 2,
        "two snakes moving"
    );
    assert_eq!(state.players[0].last_chip, Some(SsnakeChipKind::Food));
    assert_eq!(state.players[1].chips, 0, "only the eater is paid");
    assert_eq!(state.points_left, 4);
    // Nothing reaches the ledger mid-flight; the seat banks on the way out.
    assert_eq!(
        state.leave(a, false),
        Some(Settlement {
            user_id: a,
            chips: SSNAKE_FOOD_CHIPS * 2,
            chip_move: ChipMove::SsnakeArenaEarned,
        })
    );
}

#[test]
fn food_against_a_wall_pays_the_edge_bonus() {
    let (mut state, a, _, generation) = arena_state();
    state.points_left = 5;
    state.bonus_food = false;
    // The test arena is walled all round, so (1,1) is a corner cell with a
    // wall above and a wall to its left.
    state.players[0].body = VecDeque::from([Pos { x: 2, y: 1 }]);
    state.point = Some(Pos { x: 1, y: 1 });
    assert_eq!(
        state.wall_edges(Pos { x: 1, y: 1 }),
        2,
        "corner touches two"
    );
    state.food_wall_edges = state.wall_edges(Pos { x: 1, y: 1 });
    state.steer(a, Direction::Left);

    state.tick(generation);
    assert_eq!(
        state.players[0].chips,
        SSNAKE_FOOD_CHIPS + 2 * SSNAKE_EDGE_BONUS_CHIPS,
        "corner food pays the base plus two edges, one snake moving"
    );
}

#[test]
fn open_floor_food_pays_the_bare_base() {
    let (state, _, _, _) = arena_state();
    assert_eq!(state.wall_edges(Pos { x: 10, y: 10 }), 0);
    assert_eq!(
        ssnake_food_chips(0, false, 1),
        SSNAKE_FOOD_CHIPS,
        "no walls, no pink, one snake"
    );
    // Edges land on the base, so pink and the snake count multiply them too.
    assert_eq!(
        ssnake_food_chips(2, true, 3),
        (SSNAKE_FOOD_CHIPS + 2 * SSNAKE_EDGE_BONUS_CHIPS) * SSNAKE_BONUS_FOOD_MULTIPLIER * 3
    );
}

#[test]
fn pink_food_pays_the_bonus_multiple() {
    let (mut state, a, _, generation) = arena_state();
    state.points_left = 5;
    state.bonus_food = true;
    state.point = Some(Pos { x: 6, y: 5 });
    state.steer(a, Direction::Right);

    state.tick(generation);
    assert_eq!(
        state.players[0].chips,
        SSNAKE_FOOD_CHIPS * SSNAKE_BONUS_FOOD_MULTIPLIER,
        "one snake moving, pink food"
    );
    assert_eq!(state.players[0].last_chip, Some(SsnakeChipKind::BonusFood));
}

#[test]
fn last_food_pays_the_clear_bonus_and_reshuffles_the_arena() {
    let (mut state, a, _, generation) = arena_state();
    let before = state.level.clone().unwrap();
    state.points_left = 1;
    state.bonus_food = false;
    state.point = Some(Pos { x: 6, y: 5 });
    state.steer(a, Direction::Right);

    let outcome = state.tick(generation);
    assert_eq!(
        state.players[0].chips,
        SSNAKE_FOOD_CHIPS + SSNAKE_CLEAR_CHIPS,
        "the closer takes the food and the clear bonus"
    );
    assert_eq!(
        state.players[0].last_chip,
        Some(SsnakeChipKind::ArenaClear),
        "both land on one tick, so the pop reads as the clear"
    );

    assert_eq!(state.phase, SsnakePhase::Running, "play never stops");
    let after = state.level.clone().unwrap();
    assert!(!Arc::ptr_eq(&before, &after), "a new arena is drawn");
    assert_eq!(state.points_left, after.points_needed);
    assert!(state.point.is_some(), "the new arena is stocked");
    for seat in 0..2 {
        assert_eq!(state.players[seat].body.len(), 1, "everyone is re-seeded");
        assert_eq!(state.players[seat].motion, Motion::Idle);
    }
    assert!(
        outcome.next_millis.is_some(),
        "the loop keeps turning through the shuffle"
    );
}

#[test]
fn skipping_needs_every_seated_player() {
    let (mut state, a, b, _) = arena_state();
    let before = state.level.clone().unwrap();

    assert!(!state.vote_skip(a), "one of two is not consensus");
    assert!(state.has_live_vote(0));
    assert!(Arc::ptr_eq(&state.level.clone().unwrap(), &before));

    // Voting twice is not a second vote.
    assert!(!state.vote_skip(a));
    assert_eq!(state.live_vote_count(), 1);

    assert!(state.vote_skip(b), "the whole table agreed");
    assert!(!Arc::ptr_eq(&state.level.clone().unwrap(), &before));
    assert!(
        state.is_frozen(),
        "a skip lands on the same hold as a clear"
    );
    assert_eq!(state.live_vote_count(), 0, "votes clear with the arena");
}

#[test]
fn a_skip_is_locked_out_for_the_cooldown() {
    let (mut state, a, b, _) = arena_state();
    assert!(!state.vote_skip(a));
    assert!(state.vote_skip(b));

    // The hold would swallow the vote on its own, so clear it and prove the
    // cooldown is what refuses the second skip.
    state.frozen_until = None;
    let after_first = state.level.clone().unwrap();
    assert!(!state.vote_skip(a));
    assert!(!state.vote_skip(b), "a lone table cannot reroll on demand");
    assert!(Arc::ptr_eq(&state.level.clone().unwrap(), &after_first));
    assert!(state.skip_cooldown_secs_left().is_some());

    // Once it lapses the table can try again.
    state.skip_available_at = None;
    assert!(!state.vote_skip(a));
    assert!(state.vote_skip(b));
}

#[test]
fn a_solo_player_still_waits_out_the_cooldown() {
    let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
    let user = Uuid::now_v7();
    state.sit(user);
    state.wake_arena();

    // Alone, one vote is the whole table — which is exactly why the
    // cooldown has to carry the weight here.
    assert!(state.vote_skip(user), "solo consensus is immediate");
    state.frozen_until = None;
    assert!(!state.vote_skip(user), "but not repeatable");
}

#[test]
fn changing_the_seats_drops_a_stale_tally() {
    let (mut state, a, _, _) = arena_state();
    assert!(!state.vote_skip(a));
    assert_eq!(state.live_vote_count(), 1);

    // A third player raises the bar, so the votes cast against a two-seat
    // table cannot carry over.
    state.seat_limit = 3;
    state.sit(Uuid::now_v7());
    assert_eq!(state.live_vote_count(), 0);
}

#[test]
fn votes_lapse_so_they_cannot_accumulate() {
    let (mut state, a, b, _) = arena_state();
    assert!(!state.vote_skip(a));
    // Backdate past the TTL: a vote cast and forgotten must not hand the
    // next voter an instant skip.
    state.skip_votes[0] = Some(Instant::now() - SSNAKE_SKIP_VOTE_TTL);
    assert_eq!(state.live_vote_count(), 0);

    let before = state.level.clone().unwrap();
    assert!(!state.vote_skip(b), "b is now alone in wanting it");
    assert!(Arc::ptr_eq(&state.level.clone().unwrap(), &before));
}

#[test]
fn spectators_cannot_vote() {
    let (mut state, _, _, _) = arena_state();
    assert!(!state.vote_skip(Uuid::now_v7()));
    assert_eq!(state.live_vote_count(), 0);
}

#[test]
fn the_new_arena_holds_still_before_anyone_can_move() {
    let (mut state, a, _, generation) = arena_state();
    state.points_left = 1;
    state.point = Some(Pos { x: 6, y: 5 });
    state.steer(a, Direction::Right);
    state.tick(generation);

    assert!(state.is_frozen(), "the shuffle arms the hold");
    let head = state.players[0].body[0];

    // A steer carried over from the arena that just ended is dropped, and
    // the board does not advance while the hold is on.
    state.steer(a, Direction::Left);
    assert_eq!(state.players[0].motion, Motion::Idle);
    let outcome = state.tick(generation);
    assert!(outcome.ticked, "the hold still repaints");
    assert_eq!(state.players[0].body[0], head, "nobody moved");

    // Once it lapses, play resumes normally.
    state.frozen_until = None;
    state.steer(a, Direction::Left);
    assert_eq!(state.players[0].motion, Motion::Moving(Direction::Left));
}

#[test]
fn a_seat_that_crashed_more_than_it_ate_banks_a_debit() {
    // The sign lives in the ChipMove, so a losing visit takes the same exit
    // as a winning one and the amount stays positive.
    let (mut state, a, _, _) = arena_state();
    state.players[0].chips = -30;
    assert_eq!(
        state.leave(a, false),
        Some(Settlement {
            user_id: a,
            chips: 30,
            chip_move: ChipMove::SsnakeArenaLost,
        })
    );
}

#[test]
fn a_seat_that_broke_even_writes_nothing() {
    // Nothing moved, so nothing should reach the ledger, and nothing should
    // fire the chip notify behind it.
    let (mut state, a, _, _) = arena_state();
    assert_eq!(state.players[0].chips, 0);
    assert_eq!(state.leave(a, false), None);
}

#[test]
fn crashing_costs_chips_and_respawns_instead_of_eliminating() {
    let (mut state, a, _, generation) = arena_state();
    state.players[0].body[0] = Pos { x: 1, y: 5 };
    state.steer(a, Direction::Left);

    state.tick(generation);
    assert_eq!(state.players[0].chips, -SSNAKE_CRASH_CHIPS);
    assert_eq!(state.players[0].last_chip, Some(SsnakeChipKind::Crash));
    assert_eq!(state.players[0].motion, Motion::Dying);

    // The shrink runs out and the snake simply comes back.
    state.players[0].body = VecDeque::from([Pos { x: 1, y: 5 }]);
    state.players[0].respawn_length = 9;
    state.step_death_shrink(0);
    assert_eq!(state.players[0].motion, Motion::Idle);
    assert_eq!(state.players[0].pending_growth, 9);
    assert_eq!(state.players[0].body.len(), 1);
}

#[test]
fn bailing_out_while_moving_costs_the_crash_penalty() {
    // Otherwise `l` is a free escape: a snake one tick from a wall stands
    // up, dodges the -10, and sits straight back down on a clean spawn.
    let (mut state, a, _, _) = arena_state();
    state.players[0].chips = 100;
    state.steer(a, Direction::Left);

    // The penalty lands before the seat is cashed out, so what reaches the
    // ledger is the whole visit including the exit charge.
    assert_eq!(
        state.leave(a, true),
        Some(Settlement {
            user_id: a,
            chips: 100 - SSNAKE_CRASH_CHIPS,
            chip_move: ChipMove::SsnakeArenaEarned,
        })
    );
    assert!(state.status_message.contains("bailed"));
}

#[test]
fn standing_up_parked_or_dying_is_free() {
    // A never-steered snake was never at risk, and a dying one has already
    // been charged for the crash it is playing out.
    for motion in [Motion::Idle, Motion::Dying] {
        let (mut state, a, _, _) = arena_state();
        state.players[0].motion = motion;
        state.players[0].chips = 100;
        assert_eq!(
            state.leave(a, true).map(|settlement| settlement.chips),
            Some(100),
            "{motion:?} should leave for free"
        );
    }
}

#[test]
fn the_idle_kick_banks_the_seat_without_charging_it() {
    // Nothing frees a seat when a session drops, so for anyone who closes
    // their terminal the kick is the only settle path: it has to pay out.
    // It must not charge the bail penalty though, since it is involuntary
    // and two minutes out, so it cannot be used to dodge a crash.
    let (mut state, a, _, _) = arena_state();
    state.steer(a, Direction::Left);
    state.players[0].chips = 250;
    state.last_activity[0] = Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);
    let generation = state.activity_generation[0];

    let outcome = state.kick_inactive_user(a, generation);
    assert!(outcome.reclaimed);
    assert_eq!(
        outcome.settlement,
        Some(Settlement {
            user_id: a,
            chips: 250,
            chip_move: ChipMove::SsnakeArenaEarned,
        }),
        "a dropped connection still banks what it earned, at full value"
    );
    assert!(state.seats[0].is_none(), "the seat reopens");
}

#[test]
fn a_kick_that_reclaims_nothing_changes_nothing() {
    // The kick timer is re-armed on every keypress, so the common case is a
    // stale timer firing against a seat that is still being played. It must
    // not publish or settle, or a steering player would wake every session
    // watching the table once per keystroke.
    let (mut state, a, _, _) = arena_state();
    state.players[0].chips = 250;
    let generation = state.activity_generation[0];

    let outcome = state.kick_inactive_user(a, generation);
    assert!(!outcome.reclaimed, "the seat was touched recently");
    assert!(outcome.settlement.is_none());
    assert_eq!(state.seats[0], Some(a));
}

#[test]
fn a_crash_sheds_a_fifth_of_the_snake() {
    let (mut state, a, _, generation) = arena_state();
    // A long snake running head-first into the left wall.
    state.players[0].body = (1..=20).map(|x| Pos { x, y: 5 }).collect();
    state.players[0].pending_growth = 0;
    state.steer(a, Direction::Left);
    state.tick(generation);

    assert_eq!(state.players[0].motion, Motion::Dying);
    assert_eq!(
        state.players[0].respawn_length, 16,
        "20 long, back at 80% — an unwieldy snake gets a way out"
    );

    // The floor keeps a short snake from respawning as a bare head.
    assert_eq!(ssnake_respawn_length(2), SSNAKE_MIN_RESPAWN_LENGTH);
    assert_eq!(ssnake_respawn_length(100), 80);
}

#[test]
fn the_death_shrink_eats_several_segments_a_tick() {
    let (mut state, _, _, _) = arena_state();
    let body: VecDeque<Pos> = (0..10).map(|x| Pos { x, y: 5 }).collect();
    state.players[0].body = body;
    state.players[0].motion = Motion::Dying;

    state.step_death_shrink(0);
    assert_eq!(
        state.players[0].body.len(),
        10 - SSNAKE_DEATH_SHRINK_PER_TICK,
        "a long snake should not take its own length in ticks to clear"
    );
    assert_eq!(state.players[0].motion, Motion::Dying, "still shrinking");

    // The last partial batch stops at the head rather than overrunning it.
    state.players[0].body = VecDeque::from([Pos { x: 1, y: 5 }, Pos { x: 2, y: 5 }]);
    state.players[0].respawn_length = 4;
    state.step_death_shrink(0);
    assert_eq!(state.players[0].motion, Motion::Idle, "respawned");
    assert_eq!(state.players[0].body.len(), 1);
}

#[test]
fn earnings_survive_an_arena_shuffle_but_not_standing_up() {
    let (mut state, a, _, _) = arena_state();
    state.players[0].chips = 250;
    state.load_level();
    assert_eq!(
        state.players[0].chips, 250,
        "the session tally carries over"
    );
    state.leave(a, true);
    assert_eq!(state.players[0].chips, 0, "standing up clears the tally");
}

#[test]
fn steer_rejects_reversal_against_last_move() {
    let (mut state, a, _, generation) = arena_state();
    state.steer(a, Direction::Right);
    state.tick(generation);
    assert_eq!(state.players[0].last_moved, Some(Direction::Right));
    state.steer(a, Direction::Left);
    assert_eq!(state.players[0].motion, Motion::Moving(Direction::Right));
    state.steer(a, Direction::Up);
    assert_eq!(state.players[0].motion, Motion::Moving(Direction::Up));
    // Double-turn reversal within one tick is also blocked (OldDir guard).
    state.steer(a, Direction::Left);
    assert_eq!(state.players[0].motion, Motion::Moving(Direction::Up));
}

#[test]
fn table_caps_at_its_seat_limit() {
    let (mut state, _, _, _) = arena_state();
    assert!(state.sit(Uuid::now_v7()).is_none());
}

#[test]
fn the_house_table_seats_five_distinct_snakes() {
    let settings = SsnakeTableSettings {
        seats: 5,
        ..Default::default()
    };
    let mut state = SharedState::new(Uuid::now_v7(), settings);
    for seat in 0..5 {
        assert_eq!(state.sit(Uuid::now_v7()), Some(seat));
    }
    assert!(state.sit(Uuid::now_v7()).is_none(), "the table caps at 5");

    let labels: Vec<&str> = (0..5)
        .map(|seat| SsnakeColor::for_seat(seat).label())
        .collect();
    assert_eq!(labels, ["Green", "Red", "Blue", "Purple", "Cyan"]);
    for seat in 0..5 {
        assert_eq!(
            state.players[seat].body.len(),
            1,
            "seat {seat} should be on the board"
        );
    }
}

#[test]
fn wrap_pos_wraps_all_edges() {
    assert_eq!(
        wrap_pos(Pos { x: 0, y: 0 }, Direction::Left, 10, 6),
        Pos { x: 9, y: 0 }
    );
    assert_eq!(
        wrap_pos(Pos { x: 9, y: 0 }, Direction::Right, 10, 6),
        Pos { x: 0, y: 0 }
    );
    assert_eq!(
        wrap_pos(Pos { x: 0, y: 0 }, Direction::Up, 10, 6),
        Pos { x: 0, y: 5 }
    );
    assert_eq!(
        wrap_pos(Pos { x: 0, y: 5 }, Direction::Down, 10, 6),
        Pos { x: 0, y: 0 }
    );
}

#[test]
fn torus_distance_goes_the_short_way_round() {
    // Opposite walls are neighbours through the warp edges.
    assert_eq!(
        torus_distance(Pos { x: 0, y: 0 }, Pos { x: 9, y: 0 }, 10, 6),
        1
    );
    assert_eq!(
        torus_distance(Pos { x: 0, y: 0 }, Pos { x: 0, y: 5 }, 10, 6),
        1
    );
    assert_eq!(
        torus_distance(Pos { x: 1, y: 1 }, Pos { x: 4, y: 3 }, 10, 6),
        5
    );
}

#[test]
fn moving_into_own_vacated_tail_cell_is_safe() {
    let (mut state, _, _, generation) = arena_state();
    // Hand-build a 2x2 loop body: head at (5,5), tail at (5,6).
    state.players[0].body = VecDeque::from([
        Pos { x: 5, y: 5 },
        Pos { x: 6, y: 5 },
        Pos { x: 6, y: 6 },
        Pos { x: 5, y: 6 },
    ]);
    state.players[0].motion = Motion::Moving(Direction::Down);
    state.players[0].last_moved = Some(Direction::Left);
    state.tick(generation);
    assert_eq!(state.players[0].chips, 0, "tail cell vacated, no crash");
    assert_eq!(state.players[0].motion, Motion::Moving(Direction::Down));
}
