use super::*;
use crate::app::lobby::house::ssnake::levels::open_test_arena;

fn state_with_two_players() -> (SharedState, Uuid, Uuid) {
    let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    state.sit(a);
    state.sit(b);
    (state, a, b)
}

fn started_state() -> (SharedState, Uuid, Uuid, u64) {
    let (mut state, a, b) = state_with_two_players();
    let tick_loop = state.start_round(a).expect("round should start");
    (state, a, b, tick_loop.generation)
}

/// Started state on a deterministic walled 30x20 arena, with both snakes
/// parked at known safe cells and no point on the board.
fn arena_state() -> (SharedState, Uuid, Uuid, u64) {
    let (mut state, a, b, generation) = started_state();
    state.level = Some(Arc::new(open_test_arena(30, 20)));
    state.players[0].body = VecDeque::from([Pos { x: 5, y: 5 }]);
    state.players[0].pending_growth = 0;
    state.players[1].body = VecDeque::from([Pos { x: 20, y: 10 }]);
    state.players[1].pending_growth = 0;
    state.point = None;
    (state, a, b, generation)
}

#[test]
fn start_requires_two_players() {
    let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
    let user = Uuid::now_v7();
    state.sit(user);
    assert!(state.start_round(user).is_none());
    assert_eq!(state.phase, SsnakePhase::Waiting);
}

#[test]
fn start_picks_a_level_and_seeds_players() {
    let (state, _, _, _) = started_state();
    let level = state.level.as_ref().expect("level chosen");
    assert_eq!(state.points_left, level.points_needed);
    let seeded: Vec<_> = state
        .players
        .iter()
        .filter(|player| player.in_round)
        .collect();
    assert_eq!(seeded.len(), 2, "both seated snakes join the round");
    for player in seeded {
        assert_eq!(player.lives, level.lives);
        assert_eq!(player.body.len(), 1);
        assert_eq!(player.motion, Motion::Idle);
    }
    assert!(state.point.is_some());
}

#[test]
fn snakes_hold_still_until_first_steer() {
    let (mut state, _, _, generation) = started_state();
    let heads = [state.players[0].body[0], state.players[1].body[0]];
    state.tick_generation(generation);
    assert_eq!(state.players[0].body[0], heads[0]);
    assert_eq!(state.players[1].body[0], heads[1]);
}

#[test]
fn steer_rejects_reversal_against_last_move() {
    let (mut state, a, _, generation) = arena_state();
    state.steer(a, Direction::Right);
    state.tick_generation(generation);
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
fn wall_hit_costs_a_life_and_starts_death_shrink() {
    let (mut state, a, _, generation) = arena_state();
    state.players[0].body[0] = Pos { x: 1, y: 5 };
    let lives_before = state.players[0].lives;
    state.steer(a, Direction::Left);
    state.tick_generation(generation);
    assert_eq!(state.players[0].lives, lives_before - 1);
    assert_eq!(state.players[0].motion, Motion::Dying);
}

#[test]
fn last_point_awards_bonus_and_ends_match_on_score() {
    let (mut state, a, _, generation) = arena_state();
    let level = state.level.clone().unwrap();
    state.round_tick_count = SSNAKE_PLAYED_MIN_TICKS;
    state.points_left = 1;
    state.life_point = false;
    state.point = Some(Pos { x: 6, y: 5 });
    state.steer(a, Direction::Right);
    let outcome = state.tick_generation(generation);
    assert_eq!(state.phase, SsnakePhase::Finished);
    assert!(state.players[0].score >= level.points_bonus);
    let game_end = outcome.game_end.expect("match should end");
    assert_eq!(
        game_end.win.map(|win| win.user_id),
        Some(a),
        "sole scorer should win"
    );
}

#[test]
fn elimination_hands_the_win_to_the_survivor() {
    let (mut state, _, b, _) = started_state();
    state.round_tick_count = SSNAKE_PLAYED_MIN_TICKS;
    state.players[0].eliminated = true;
    state.players[0].body.clear();
    let game_end = state.finish_if_eliminated().expect("match should end");
    assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
    assert_eq!(game_end.win.map(|win| win.user_id), Some(b));
}

#[test]
fn death_shrink_respawns_with_previous_size_while_lives_remain() {
    let (mut state, _, _, _) = started_state();
    state.players[0].motion = Motion::Dying;
    state.players[0].respawn_length = 9;
    state.players[0].lives = 1;
    state.players[0].body = VecDeque::from([Pos { x: 3, y: 3 }]);
    state.step_death_shrink(0);
    assert_eq!(state.players[0].motion, Motion::Idle);
    assert_eq!(state.players[0].pending_growth, 9);
    assert_eq!(state.players[0].body.len(), 1);
    assert!(!state.players[0].eliminated);
}

#[test]
fn death_shrink_eliminates_when_out_of_lives() {
    let (mut state, _, _, _) = started_state();
    state.players[0].motion = Motion::Dying;
    state.players[0].lives = 0;
    state.players[0].body = VecDeque::from([Pos { x: 3, y: 3 }]);
    state.step_death_shrink(0);
    assert!(state.players[0].eliminated);
}

#[test]
fn leaving_mid_match_forfeits_to_the_opponent() {
    let (mut state, a, b, _) = started_state();
    state.round_tick_count = SSNAKE_PLAYED_MIN_TICKS;
    let game_end = state.leave(a).expect("match should end");
    assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
    assert_eq!(game_end.win.map(|win| win.user_id), Some(b));
}

#[test]
fn short_round_win_pays_no_chips() {
    let (mut state, a, _, _) = started_state();
    let game_end = state.leave(a).expect("match should end");
    assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
    assert!(
        game_end.win.is_none(),
        "instant forfeit must not farm the win reward"
    );
    assert!(
        !state.status_message.contains("chips"),
        "status must not promise chips for an ineligible round"
    );
}

#[test]
fn seated_player_cycles_arena_choice_outside_matches() {
    let (mut state, a, _) = state_with_two_players();
    assert_eq!(state.selected_level, None);
    state.select_level(a, 1);
    assert_eq!(state.selected_level, Some(0));
    assert!(state.level.is_some(), "picking a level previews it");
    state.select_level(a, -1);
    assert_eq!(state.selected_level, None);
    assert!(state.level.is_none(), "random arena shows no preview");

    // The fixed pick drives the next match.
    state.select_level(a, 3);
    assert_eq!(state.selected_level, Some(2));
    state.start_round(a).expect("round should start");
    assert_eq!(state.level.as_ref().unwrap().name, LEVELS[2].name);

    // Mid-match the choice is locked.
    state.select_level(a, 1);
    assert_eq!(state.selected_level, Some(2));
}

#[test]
fn two_seat_table_rejects_a_third_snake() {
    let (mut state, _, _) = state_with_two_players();
    assert!(state.sit(Uuid::now_v7()).is_none());
}

#[test]
fn three_player_match_runs_until_one_survivor() {
    let settings = SsnakeTableSettings {
        seats: 3,
        ..Default::default()
    };
    let mut state = SharedState::new(Uuid::now_v7(), settings);
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    let c = Uuid::now_v7();
    assert_eq!(state.sit(a), Some(0));
    assert_eq!(state.sit(b), Some(1));
    assert_eq!(state.sit(c), Some(2));
    assert!(state.sit(Uuid::now_v7()).is_none(), "table caps at 3");

    state.start_round(a).expect("round should start");
    state.round_tick_count = SSNAKE_PLAYED_MIN_TICKS;
    assert!(state.players[0].in_round);
    assert!(state.players[2].in_round);
    assert!(!state.players[3].in_round);

    // First knockout leaves two active snakes: the match continues.
    state.players[0].eliminated = true;
    state.players[0].body.clear();
    assert!(state.finish_if_eliminated().is_none());

    // Second knockout leaves one: the survivor wins.
    state.players[1].eliminated = true;
    state.players[1].body.clear();
    let game_end = state.finish_if_eliminated().expect("match should end");
    assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 2 }));
    assert_eq!(game_end.win.map(|win| win.user_id), Some(c));
}

#[test]
fn level_complete_with_three_players_rewards_top_score() {
    let settings = SsnakeTableSettings {
        seats: 3,
        ..Default::default()
    };
    let mut state = SharedState::new(Uuid::now_v7(), settings);
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    let c = Uuid::now_v7();
    state.sit(a);
    state.sit(b);
    state.sit(c);
    state.start_round(a).expect("round should start");
    state.round_tick_count = SSNAKE_PLAYED_MIN_TICKS;
    state.players[0].score = 10;
    state.players[1].score = 30;
    state.players[2].score = 20;

    let game_end = state.finish_level_complete();

    assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
    assert_eq!(game_end.win.map(|win| win.user_id), Some(b));
}

#[test]
fn spectators_cannot_change_the_arena() {
    let (mut state, _, _) = state_with_two_players();
    state.select_level(Uuid::now_v7(), 1);
    assert_eq!(state.selected_level, None);
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
    let lives_before = state.players[0].lives;
    state.tick_generation(generation);
    assert_eq!(state.players[0].lives, lives_before, "tail cell vacated");
    assert_eq!(state.players[0].motion, Motion::Moving(Direction::Down));
}
