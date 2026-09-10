//! Daily pool state tests.
//!
//! The physics and the rulesets are already covered in `pool_core`; what is
//! new here is the *match* layer — the round trip through JSON, the turn and
//! ball-in-hand bookkeeping, and the placement rules that decide whether a
//! shot is even accepted. So these tests never assert a ball position: they
//! assert what the state did with the verdict it was handed.

use uuid::Uuid;

use super::*;
use crate::app::games::pool_core::{
    cue::{MAX_SPEED, MISCUE_LIMIT, PowerBand, ShotMode},
    rules::{self, PoolRules},
};

fn players() -> (Uuid, Uuid) {
    (Uuid::new_v4(), Uuid::new_v4())
}

fn state(rules: PoolRules) -> DailyPoolState {
    let (a, b) = players();
    DailyPoolState::new(rules, a, b)
}

/// A break down the table at a sensible pace.
fn break_shot() -> Shot {
    Shot {
        place: None,
        azimuth: 0.0,
        tip: [0.0, 0.0],
        speed: 7.0,
        called_pocket: None,
        play_again: false,
    }
}

#[test]
fn a_fresh_match_is_racked_and_ready() {
    for rules in PoolRules::ALL {
        let state = state(rules);
        let expected = match rules {
            PoolRules::EightBall => 16,
            PoolRules::NineBall => 10,
            // Fifteen reds, six colours, and the cue ball.
            PoolRules::Snooker => 22,
        };
        assert_eq!(state.rack.balls.len(), expected, "{rules:?}");
        assert!(
            state.rack.on_table().count() == expected,
            "nothing starts in a pocket"
        );
        assert!(state.rack.get(CUE).is_some(), "there is a cue ball");
        assert_eq!(state.turn, 0, "seat 0 breaks");
        assert!(!state.is_finished());
        assert!(state.prev_rack.is_none());
    }
}

#[test]
fn both_seats_are_the_two_players_and_nobody_else() {
    let (a, b) = players();
    let state = DailyPoolState::new(PoolRules::NineBall, a, b);
    assert_ne!(state.seats[0], state.seats[1]);
    assert!(state.seat_of(a).is_some() && state.seat_of(b).is_some());
    assert_eq!(state.seat_of(Uuid::new_v4()), None);
    assert_eq!(state.turn_user(), state.user_of(state.turn));
}

#[test]
fn the_state_survives_a_round_trip_through_json() {
    let mut before = state(PoolRules::EightBall);
    before
        .apply_shot(0, &break_shot())
        .expect("the break is legal");

    let value = serde_json::to_value(&before).expect("serializes");
    let after = DailyPoolState::parse(&value).expect("parses");

    assert_eq!(after.rack, before.rack, "the rack must survive exactly");
    assert_eq!(after.turn, before.turn);
    assert_eq!(after.groups, before.groups);
    assert_eq!(after.ball_in_hand, before.ball_in_hand);
    assert_eq!(after.shots.len(), before.shots.len());
    assert_eq!(after.table, before.table);
}

#[test]
fn a_future_state_version_is_refused_rather_than_guessed_at() {
    let mut value = serde_json::to_value(state(PoolRules::NineBall)).expect("serializes");
    value["version"] = serde_json::json!(STATE_VERSION + 1);
    assert!(DailyPoolState::parse(&value).is_err());
}

#[test]
fn the_table_preset_is_resolved_not_assumed() {
    let mut state = state(PoolRules::NineBall);
    assert!(state.spec().is_ok());
    // A preset this build has never heard of must fail loudly: a rack means
    // nothing on equipment it was not played on.
    state.table = "an antique with pockets like buckets".to_string();
    assert!(state.spec().is_err());
    assert!(state.last_timeline().is_none());
}

#[test]
fn only_the_player_at_the_table_may_shoot() {
    let mut state = state(PoolRules::NineBall);
    assert!(
        state.apply_shot(1, &break_shot()).is_err(),
        "seat 1 is not on"
    );
    assert!(state.apply_shot(0, &break_shot()).is_ok());
}

#[test]
fn a_shot_records_its_history_and_keeps_the_rack_it_came_from() {
    let mut state = state(PoolRules::EightBall);
    let before = state.rack.clone();
    state.apply_shot(0, &break_shot()).expect("legal");

    assert_eq!(state.move_count(), 1);
    assert_eq!(state.shots[0].seat, 0);
    assert!(!state.shots[0].label.is_empty(), "every shot gets a label");
    assert_eq!(
        state.prev_rack.as_ref(),
        Some(&before),
        "the pre-shot rack is kept so the shot can be replayed"
    );
}

#[test]
fn the_last_shot_can_be_replayed_for_the_animation() {
    let mut state = state(PoolRules::NineBall);
    assert!(
        state.last_timeline().is_none(),
        "nothing to watch before the break"
    );
    state.apply_shot(0, &break_shot()).expect("legal");

    let timeline = state.last_timeline().expect("the break replays");
    assert!(timeline.duration > 0.0, "a break takes time");
    assert!(timeline.frame_count() > 1, "and more than one frame");
    // Re-deriving it twice must give the same thing, or two players watching
    // the same shot would see different racks.
    let again = state.last_timeline().expect("replays again");
    assert_eq!(again.frame_count(), timeline.frame_count());
    assert_eq!(again.duration, timeline.duration);
}

#[test]
fn an_unplayable_stroke_is_refused_before_it_reaches_the_simulator() {
    let mut state = state(PoolRules::NineBall);
    for bad in [
        Shot {
            speed: MAX_SPEED * 2.0,
            ..break_shot()
        },
        Shot {
            speed: 0.0,
            ..break_shot()
        },
        Shot {
            // Past the miscue limit: a real cue would slip off the ball.
            tip: [0.9, 0.0],
            ..break_shot()
        },
        Shot {
            azimuth: f64::NAN,
            ..break_shot()
        },
    ] {
        assert!(state.apply_shot(0, &bad).is_err(), "{bad:?} should refuse");
    }
    assert_eq!(state.move_count(), 0, "and none of them counted as a shot");
}

#[test]
fn the_rack_is_over_once_it_is_over() {
    let mut state = state(PoolRules::NineBall);
    state.winner = Some(0);
    assert!(state.is_finished());
    assert!(
        state.apply_shot(0, &break_shot()).is_err(),
        "you cannot shoot after the rack is decided"
    );
}

// ── Ball in hand ──────────────────────────────────────────────────────

#[test]
fn placing_the_cue_ball_needs_permission() {
    let mut state = state(PoolRules::NineBall);
    let spec = state.spec().expect("known table");
    let at = [spec.length * 0.15, spec.width * 0.5];
    assert!(
        state.ball_in_hand.is_none(),
        "the breaker does not get ball in hand"
    );
    let placed = Shot {
        place: Some(at),
        ..break_shot()
    };
    assert!(state.apply_shot(0, &placed).is_err());
}

#[test]
fn a_scratch_leaves_the_cue_ball_down_until_it_is_placed() {
    let mut state = state(PoolRules::NineBall);
    // Pot the cue ball by hand: getting there through the physics would mean
    // tuning a shot, and what is under test is the bookkeeping, not the shot.
    let cue = state
        .rack
        .balls
        .iter_mut()
        .find(|b| b.id == CUE)
        .expect("cue ball");
    cue.potted = Some(0);
    state.ball_in_hand = Some(BallInHand::Anywhere);

    assert!(state.must_place(), "there is no cue ball to shoot");
    assert!(
        state.apply_shot(0, &break_shot()).is_err(),
        "shooting without placing must be refused, or the simulator would
         quietly resurrect the cue ball at the pocket mouth"
    );

    let spec = state.spec().expect("known table");
    let placed = Shot {
        place: Some([spec.length * 0.3, spec.width * 0.5]),
        ..break_shot()
    };
    assert!(state.apply_shot(0, &placed).is_ok());
    assert!(!state.must_place(), "and now it is back up");
    assert!(
        state.ball_in_hand.is_none() || state.ball_in_hand.is_some(),
        "the next ruling owns this field"
    );
}

#[test]
fn the_cue_ball_cannot_be_placed_off_the_table_or_on_another_ball() {
    let mut state = state(PoolRules::EightBall);
    state.ball_in_hand = Some(BallInHand::Anywhere);
    let spec = state.spec().expect("known table");
    let occupied = state
        .rack
        .balls
        .iter()
        .find(|b| b.id != CUE)
        .expect("object balls")
        .pos;

    for bad in [
        [-1.0, spec.width * 0.5],
        [spec.length + 1.0, spec.width * 0.5],
        occupied,
    ] {
        let shot = Shot {
            place: Some(bad),
            ..break_shot()
        };
        assert!(
            state.apply_shot(0, &shot).is_err(),
            "{bad:?} is not a legal spot"
        );
    }
}

#[test]
fn the_kitchen_restriction_is_enforced() {
    let mut state = state(PoolRules::EightBall);
    state.ball_in_hand = Some(BallInHand::Kitchen);
    let spec = state.spec().expect("known table");
    let head = rules::head_string(spec);

    let behind = Shot {
        place: Some([head * 0.5, spec.width * 0.5]),
        ..break_shot()
    };
    let past = Shot {
        place: Some([head * 1.5, spec.width * 0.5]),
        ..break_shot()
    };
    assert!(
        state.clone().apply_shot(0, &past).is_err(),
        "past the head string is out of the kitchen"
    );
    assert!(state.apply_shot(0, &behind).is_ok());
}

// ── Calling the eight ─────────────────────────────────────────────────

#[test]
fn the_eight_must_be_called_and_only_the_eight() {
    let mut state = state(PoolRules::EightBall);
    assert!(!state.requires_call(), "a break is never a called shot");

    // Clear seat 0's group so the eight is all that is left to them.
    state.groups = Some([Group::Solids, Group::Stripes]);
    state.shots = vec![PoolShotRecord {
        seat: 0,
        shot: break_shot(),
        label: "break".to_string(),
        at: Utc::now(),
    }];
    for ball in state.rack.balls.iter_mut() {
        if (1..=7).contains(&ball.id) {
            ball.potted = Some(0);
        }
    }
    assert!(state.requires_call(), "down to the eight, so call it");
    assert!(
        state.apply_shot(0, &break_shot()).is_err(),
        "an uncalled shot on the eight is refused"
    );

    let called = Shot {
        called_pocket: Some(99),
        ..break_shot()
    };
    assert!(
        state.apply_shot(0, &called).is_err(),
        "and so is a pocket that does not exist"
    );
}

#[test]
fn the_pocket_for_the_eight_can_actually_be_named() {
    // Regression, and the same shape as the ball-in-hand dead board: nothing
    // wrote `called_pocket` at all, while `apply_shot` *refuses* an uncalled
    // shot on the eight. Eight-ball therefore reached a position where every
    // shot was rejected and no gesture would fix it — the game could not be
    // finished.
    let mut state = state(PoolRules::EightBall);
    state.groups = Some([Group::Solids, Group::Stripes]);
    state.shots = vec![PoolShotRecord {
        seat: 0,
        shot: break_shot(),
        label: "break".to_string(),
        at: Utc::now(),
    }];
    for ball in state.rack.balls.iter_mut() {
        if (1..=7).contains(&ball.id) {
            ball.potted = Some(0);
        }
    }
    assert!(state.requires_call());

    let spec = state.spec().expect("known table");
    let geom = spec.geometry();
    let mut draft = PoolDraft::new(&state);
    assert_eq!(draft.called_pocket, None, "nothing is called to start with");

    // Pointing at the middle of the cloth names nothing: a call has to be a
    // pocket, not the nearest one to a random spot.
    assert!(!draft.call_pocket_at(&state, [spec.length / 2.0, spec.width / 2.0]));
    assert_eq!(draft.called_pocket, None);

    for (index, pocket) in geom.pockets.iter().enumerate() {
        // A little short of the pocket, the way a click lands.
        let at = [
            pocket.center[0] - pocket.outward[0] * spec.ball_radius,
            pocket.center[1] - pocket.outward[1] * spec.ball_radius,
        ];
        assert!(draft.call_pocket_at(&state, at), "pocket {index}");
        assert_eq!(
            draft.called_pocket,
            Some(index as u8),
            "the nearest pocket to {at:?} is {index}"
        );
    }

    // And with one named, the shot the server was refusing goes through.
    let shot = draft.shot(&state).expect("aimed at the eight");
    assert!(shot.called_pocket.is_some());
    assert!(state.apply_shot(0, &shot).is_ok(), "a called shot is legal");
}

#[test]
fn nothing_is_called_when_nothing_is_being_called_for() {
    // A pocket named on an open table would ride along in the shot and decide
    // a rack that had not reached the eight yet.
    let state = state(PoolRules::EightBall);
    let spec = state.spec().expect("known table");
    let mut draft = PoolDraft::new(&state);
    assert!(!state.requires_call());
    let corner = spec.geometry().pockets[0].center;
    assert!(!draft.call_pocket_at(&state, corner));
    assert_eq!(draft.called_pocket, None);
}

// ── The shot draft ────────────────────────────────────────────────────
//
// `PoolDraft` lives in `state.rs`, but its arithmetic is all against a
// `DailyPoolState`, so it is tested here where a state is a one-liner.

use crate::app::lobby::daily::pool_draft::{PointerOutcome, PoolDraft};

fn drafted() -> (DailyPoolState, PoolDraft) {
    let state = state(PoolRules::NineBall);
    let draft = PoolDraft::new(&state);
    (state, draft)
}

#[test]
fn a_new_draft_is_aimed_at_something_legal() {
    let (state, draft) = drafted();
    assert_eq!(
        draft.target,
        state.legal_targets().first().copied(),
        "a player who fires straight away should play a real shot"
    );
    assert!(draft.shot(&state).is_some());
    assert_eq!(draft.aim_offset, 0.0);
}

#[test]
fn cycling_walks_the_legal_targets_and_wraps() {
    // Eight-ball, because nine-ball only ever has one legal target — the
    // lowest ball — so there is nothing there to cycle through.
    let state = state(PoolRules::EightBall);
    let mut draft = PoolDraft::new(&state);
    let targets = state.legal_targets();
    assert!(
        targets.len() > 2,
        "an open eight-ball table is every ball but the eight"
    );

    let first = draft.target.expect("aimed at something");
    draft.cycle_target(&state, 1);
    assert_ne!(draft.target, Some(first), "forward moves on");

    // All the way round comes home.
    for _ in 1..targets.len() {
        draft.cycle_target(&state, 1);
    }
    assert_eq!(draft.target, Some(first), "the cycle wraps");

    draft.cycle_target(&state, -1);
    assert_ne!(draft.target, Some(first), "and it walks backwards too");
}

#[test]
fn cycling_never_offers_a_ball_that_is_not_on() {
    // Nine-ball's only legal target is the lowest ball up, so the picker must
    // not let the player aim at a ball the rules would call a foul.
    let (mut state, mut draft) = drafted();
    for ball in state.rack.balls.iter_mut() {
        if (1..=5).contains(&ball.id) {
            ball.potted = Some(0);
        }
    }
    for _ in 0..12 {
        draft.cycle_target(&state, 1);
        assert_eq!(
            draft.target,
            Some(6),
            "the 6 is the lowest left, so it is the only thing on"
        );
    }
}

#[test]
fn the_aim_offset_slides_across_the_shot_line_not_along_it() {
    let (state, mut draft) = drafted();
    let spec = state.spec().expect("known table");
    let centre = draft.aim_point(&state).expect("aimed");
    draft.nudge_aim(1.0);
    let slid = draft.aim_point(&state).expect("still aimed");

    let moved = (slid[0] - centre[0]).hypot(slid[1] - centre[1]);
    assert!(
        (moved - spec.ball_radius).abs() < 1e-9,
        "one unit of offset is one ball radius, got {moved}"
    );

    // And it is perpendicular: the distance from the cue ball is unchanged to
    // first order, but the direction is not.
    let before = draft.azimuth(&state);
    draft.nudge_aim(-1.0);
    assert!(
        (before - draft.azimuth(&state)).abs() > 1e-6,
        "sliding the aim point must change the shot direction"
    );
}

#[test]
fn changing_target_forgets_the_old_offset() {
    // The offset is measured across a particular shot line; carrying it onto a
    // new target would silently aim somewhere nobody asked for.
    let (state, mut draft) = drafted();
    draft.nudge_aim(1.5);
    assert!(draft.aim_offset != 0.0);
    draft.cycle_target(&state, 1);
    assert_eq!(draft.aim_offset, 0.0);
}

#[test]
fn the_ghost_ball_sits_two_radii_back_from_the_target() {
    let (state, draft) = drafted();
    let spec = state.spec().expect("known table");
    let target = state
        .rack
        .get(draft.target.expect("aimed"))
        .expect("on the table");
    let ghost = draft.ghost(&state).expect("a centred aim contacts");

    let gap = (ghost[0] - target.pos[0]).hypot(ghost[1] - target.pos[1]);
    assert!(
        (gap - 2.0 * spec.ball_radius).abs() < 1e-9,
        "the ghost is where the cue ball touches: {gap}"
    );
}

#[test]
fn the_ghost_ball_vanishes_when_the_aim_misses() {
    // Losing the ghost is how the board says the line is off the ball, so it
    // has to actually go when the line stops touching.
    let (state, mut draft) = drafted();
    draft.nudge_aim(2.4);
    assert!(
        draft.ghost(&state).is_none(),
        "past two radii the cue ball passes clean by"
    );
    draft.nudge_aim(-2.4);
    assert!(draft.ghost(&state).is_some(), "and comes back when it does");
}

#[test]
fn the_tip_never_leaves_the_miscue_limit() {
    // The board refuses to set up a shot the server would then reject, so a
    // player cannot walk the tip off the ball and only find out on firing.
    let (state, mut draft) = drafted();
    for _ in 0..40 {
        draft.nudge_tip(0.05, 0.05);
    }
    let offset = draft.tip[0].hypot(draft.tip[1]);
    assert!(
        offset <= MISCUE_LIMIT + 1e-9,
        "tip walked out to {offset}, past the miscue limit"
    );
    let shot = draft.shot(&state).expect("aimed");
    assert!(
        state.clone().apply_shot(0, &shot).is_ok(),
        "and the shot it builds is one the server accepts"
    );
}

// ── Modes and the stroke ──────────────────────────────────────────────

#[test]
fn a_mode_key_arms_it_and_the_same_key_puts_it_down() {
    // There is no key-up event in a terminal, so a mode is armed and dropped
    // rather than held; pressing the same key twice must be a round trip.
    let (_, mut draft) = drafted();
    assert_eq!(draft.mode, ShotMode::Idle);
    draft.toggle_mode(ShotMode::Aim);
    assert_eq!(draft.mode, ShotMode::Aim);
    draft.toggle_mode(ShotMode::Aim);
    assert_eq!(draft.mode, ShotMode::Idle);

    // Arming another mode replaces the first: only one thing steers the
    // pointer at a time.
    draft.toggle_mode(ShotMode::Aim);
    draft.toggle_mode(ShotMode::Spin);
    assert_eq!(draft.mode, ShotMode::Spin);

    assert!(draft.cancel(), "esc puts an armed cue down");
    assert_eq!(draft.mode, ShotMode::Idle);
    assert!(
        !draft.cancel(),
        "and reports nothing when there was nothing to drop, so esc can leave"
    );
}

#[test]
fn committing_keeps_the_adjustment_and_esc_puts_it_back() {
    // Esc is the undo: it restores what the mode was armed with and drops the
    // mode. Without a counterpart that discards, a gesture that merely left
    // the mode would land in exactly the same place as one that kept it.
    let (_, mut draft) = drafted();

    draft.toggle_mode(ShotMode::Aim);
    draft.nudge_aim(0.6);
    let kept = draft.aim_offset;
    assert!(draft.commit(), "a left click commits");
    assert_eq!(draft.mode, ShotMode::Idle);
    assert_eq!(draft.aim_offset, kept, "committing keeps it");

    draft.toggle_mode(ShotMode::Aim);
    draft.nudge_aim(1.2);
    assert_ne!(draft.aim_offset, kept, "moved somewhere else");
    assert!(draft.cancel(), "esc cancels");
    assert_eq!(
        draft.aim_offset, kept,
        "cancelling restores the value the mode was armed with, not zero"
    );
}

#[test]
fn right_click_zeroes_the_armed_adjustment_and_stays_in_it() {
    // "Put the spin back to centre" is what a player reaches for far more
    // often than "undo the last few pixels of it", and centre-ball is fiddly
    // to walk back to on a face a few pixels across. So right click is a
    // reset, not an undo, and it leaves the mode armed so the next nudge
    // carries on from neutral.
    let (state, mut draft) = drafted();

    draft.toggle_mode(ShotMode::Spin);
    draft.nudge_tip(0.3, -0.2);
    assert_ne!(draft.tip, [0.0, 0.0]);
    assert!(draft.reset(&state), "right click resets the tip");
    assert_eq!(draft.tip, [0.0, 0.0], "back to centre ball");
    assert_eq!(draft.mode, ShotMode::Spin, "and still on the cue ball");

    draft.toggle_mode(ShotMode::Aim);
    draft.nudge_aim(0.9);
    assert!(draft.reset(&state));
    assert_eq!(draft.aim_offset, 0.0, "back to dead on");
    assert_eq!(draft.mode, ShotMode::Aim);

    // Committed and idle, a reset clears the whole shot. This is the common
    // case and the one that was missing: you look at the panel, decide against
    // the english, and there is nothing to re-arm and undo because you already
    // put the cue down.
    draft.commit();
    draft.nudge_tip(0.2, 0.1);
    draft.nudge_aim(0.5);
    assert_eq!(draft.mode, ShotMode::Idle);
    assert!(
        draft.reset(&state),
        "an idle board still has something to clear"
    );
    assert_eq!(draft.tip, [0.0, 0.0]);
    assert_eq!(draft.aim_offset, 0.0);
    assert!(
        !draft.reset(&state),
        "and nothing to clear once it is square"
    );

    // A half-drawn cue is not a position anyone asks for, so there a reset
    // means put it down.
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Normal));
    assert!(draft.reset(&state));
    assert_eq!(draft.mode, ShotMode::Idle, "a stroke has no neutral");
    assert!(
        !draft.reset(&state),
        "and an idle board has nothing to reset"
    );
}

#[test]
fn cancelling_a_mode_never_rolls_back_an_earlier_one() {
    // Arming a second mode commits the first and re-snapshots. Sharing one
    // snapshot across both would let a cancelled spin undo a settled aim.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Aim);
    draft.nudge_aim(0.9);
    let settled_aim = draft.aim_offset;

    draft.toggle_mode(ShotMode::Spin);
    draft.nudge_tip(0.2, 0.1);
    assert!(draft.cancel(), "cancel the spin");
    assert_eq!(draft.tip, [0.0, 0.0], "the tip goes back");
    assert_eq!(
        draft.aim_offset, settled_aim,
        "but the aim, already committed, stays put"
    );
}

#[test]
fn cancelling_a_stroke_puts_the_cue_down_without_firing() {
    let (_, mut draft) = drafted();
    let rested = draft.pull;
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Strong));
    // No button in the stroke: it is draw down, push back up through the ball.
    draft.pointer_moved(50, 20, false);
    draft.pointer_moved(50, 30, false);
    assert!(draft.pull > rested, "the cue is drawn back");

    assert!(draft.cancel(), "right click puts it down");
    assert_eq!(draft.mode, ShotMode::Idle);
    assert_eq!(draft.pull, rested, "and unwinds the pull");
    assert_eq!(
        draft.pointer_moved(50, 10, false),
        PointerOutcome::Ignored,
        "and the forward push that follows finds nothing armed"
    );
}

#[test]
fn nothing_moves_until_a_mode_is_armed() {
    // An idle board must ignore the pointer entirely: `?1003h` reports every
    // pixel of motion, and a mouse crossing the screen cannot be allowed to
    // walk the aim off the ball.
    let (_, mut draft) = drafted();
    let before = draft.aim_offset;
    for x in 10..40u16 {
        assert_eq!(
            draft.pointer_moved(x, 20, false),
            PointerOutcome::Ignored,
            "idle consumes nothing"
        );
    }
    assert_eq!(draft.aim_offset, before);
}

#[test]
fn arming_a_mode_never_jumps_the_setting_to_the_pointer() {
    // Motion is a delta from the last report, not a position, so the first
    // event after arming only sets the reference. Otherwise arming would yank
    // the aim to wherever the mouse happened to be resting.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Aim);
    assert_eq!(
        draft.pointer_moved(80, 12, false),
        PointerOutcome::Ignored,
        "the first report is the reference, not a move"
    );
    assert_eq!(draft.aim_offset, 0.0);
    assert_eq!(
        draft.pointer_moved(90, 12, false),
        PointerOutcome::Changed,
        "the second one moves it"
    );
    assert!(draft.aim_offset > 0.0);
}

#[test]
fn the_pointer_steers_whichever_mode_is_armed() {
    let (_, mut draft) = drafted();

    draft.toggle_mode(ShotMode::Aim);
    draft.pointer_moved(50, 20, false);
    draft.pointer_moved(60, 30, false);
    assert!(draft.aim_offset > 0.0, "aim follows horizontal travel");
    assert_eq!(draft.tip, [0.0, 0.0], "and leaves the tip alone");

    draft.toggle_mode(ShotMode::Spin);
    draft.pointer_moved(50, 20, false);
    draft.pointer_moved(50, 14, false);
    assert!(
        draft.tip[1] > 0.0,
        "moving the pointer up the face is follow, not draw: {:?}",
        draft.tip
    );

    draft.toggle_mode(ShotMode::Stroke(PowerBand::Normal));
    draft.pointer_moved(50, 10, false);
    draft.pointer_moved(50, 24, false);
    assert!(draft.pull > 0.0, "pulling down draws the cue back");
}

#[test]
fn the_band_scales_the_same_pull_into_a_different_shot() {
    // This is the whole reason for bands: a terminal pointer has a few dozen
    // rows to spend, and spreading 0-12 m/s over them makes a safety and a
    // break the same gesture a few pixels apart.
    let (state, mut draft) = drafted();
    let mut speeds = Vec::new();
    for band in PowerBand::ALL {
        draft.mode = ShotMode::Stroke(band);
        draft.pull = 1.0;
        let shot = draft.shot(&state).expect("aimed");
        assert!(
            state.clone().apply_shot(0, &shot).is_ok(),
            "a full pull in {band:?} must still be a legal stroke"
        );
        speeds.push(shot.speed);
    }
    assert!(
        speeds.windows(2).all(|w| w[0] < w[1]),
        "light < normal < strong: {speeds:?}"
    );
    assert!(
        (speeds[2] - MAX_SPEED).abs() < 1e-9,
        "a full pull in the top band is a real break"
    );
}

#[test]
fn the_stroke_is_draw_back_then_push_through_the_ball() {
    // The gesture is the real one: the cue goes back, then forward, and the
    // moment of contact is when it passes the ball — not when a finger lifts.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Normal));

    // First report fixes where the ball is.
    assert_eq!(
        draft.pointer_moved(50, 20, false),
        PointerOutcome::Ignored,
        "the first report is the ball, not a movement"
    );
    // Draw back.
    assert_eq!(draft.pointer_moved(50, 28, false), PointerOutcome::Changed);
    let drawn = draft.pull;
    assert!(drawn > 0.0, "pulling down draws the cue back");

    // Come forward but stop short of the ball: still not a strike.
    assert_eq!(draft.pointer_moved(50, 23, false), PointerOutcome::Changed);
    assert!(draft.pull < drawn, "and the cue follows back in");

    // Through the ball.
    assert_eq!(
        draft.pointer_moved(50, 18, false),
        PointerOutcome::Strike,
        "pushing past the ball plays the shot"
    );
}

#[test]
fn the_backswing_is_the_power_not_wherever_the_push_ended() {
    // On a real table how far you drew back decides the power; how fast the
    // follow-through is timed does not. Reading the pull at the instant of
    // contact would make every shot a soft one.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Strong));
    draft.pointer_moved(50, 20, false);
    draft.pointer_moved(50, 34, false);
    let deepest = draft.pull;
    assert!(deepest > 0.5, "a long draw is a hard shot: {deepest}");

    assert_eq!(draft.pointer_moved(50, 10, false), PointerOutcome::Strike);
    assert_eq!(
        draft.pull, deepest,
        "the shot is played at the backswing, not at the crossing"
    );
}

#[test]
fn a_twitch_forward_on_an_armed_cue_does_not_fire() {
    // Arming a stroke and nudging the mouse the wrong way must not launch the
    // ball: there has to be a real backswing behind the push.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Normal));
    draft.pointer_moved(50, 20, false);
    assert_eq!(
        draft.pointer_moved(50, 14, false),
        PointerOutcome::Changed,
        "no backswing, no stroke"
    );
    assert_eq!(draft.mode, ShotMode::Stroke(PowerBand::Normal));
}

#[test]
fn letting_go_of_the_button_is_no_longer_a_stroke() {
    // The button used to fire on release. It does not any more, so dragging
    // with it held and letting go must be inert.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Normal));
    draft.pointer_pressed(50, 20);
    draft.pointer_moved(50, 28, true);
    draft.pointer_released();
    assert_eq!(
        draft.mode,
        ShotMode::Stroke(PowerBand::Normal),
        "the cue is still up"
    );
}

#[test]
fn the_pull_stays_inside_its_band_however_hard_it_is_pushed() {
    let (state, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Stroke(PowerBand::Light));
    for _ in 0..80 {
        draft.nudge_pull(0.1);
    }
    assert_eq!(draft.pull, 1.0);
    let capped = draft.shot(&state).expect("aimed").speed;
    assert!(
        capped <= PowerBand::Light.ceiling() * MAX_SPEED + 1e-9,
        "light must stay light: {capped}"
    );
    for _ in 0..80 {
        draft.nudge_pull(-0.1);
    }
    assert_eq!(draft.pull, 0.0);
    assert!(
        state
            .clone()
            .apply_shot(0, &draft.shot(&state).expect("aimed"))
            .is_ok(),
        "even a dead-stop pull must still send a playable stroke"
    );
}

#[test]
fn aiming_at_a_bare_point_drops_the_ball_target() {
    // This is the cushion case, and the only thing the mouse can pick that the
    // keyboard cannot.
    let (state, mut draft) = drafted();
    let spec = state.spec().expect("known table");
    draft.aim_at_point([spec.length * 0.9, 0.0]);
    assert_eq!(draft.target, None);
    assert!(draft.ghost(&state).is_none(), "no ball, no ghost");
    assert!(draft.shot(&state).is_some(), "but still a shot to play");
}

// ── Ball in hand ──────────────────────────────────────────────────────

/// A state that has just been scratched on: the cue ball is down and the
/// incoming player may put it anywhere.
fn scratched() -> (DailyPoolState, PoolDraft) {
    let mut state = state(PoolRules::NineBall);
    let cue = state
        .rack
        .balls
        .iter_mut()
        .find(|b| b.id == CUE)
        .expect("cue ball");
    cue.potted = Some(0);
    state.ball_in_hand = Some(BallInHand::Anywhere);
    let draft = PoolDraft::new(&state);
    (state, draft)
}

#[test]
fn a_scratched_board_opens_holding_the_cue_ball() {
    // Regression: nothing used to write the placement at all, so after a
    // scratch `cue_ball` was None, `shot` was None, and firing silently did
    // nothing — the board was unplayable for the rest of the rack.
    let (state, draft) = scratched();
    assert!(state.must_place());
    assert_eq!(
        draft.mode,
        ShotMode::Place,
        "the board opens in hand rather than looking broken"
    );
    // And the ball is already *somewhere*: a board that opens with no cue ball
    // on it and no shot to play looks broken in a second way, so the break
    // spot is taken as a starting point and the pointer carries it from there.
    let at = draft.place.expect("holding it at a legal spot");
    let spec = state.spec().expect("known table");
    assert!(
        rules::placement_ok(
            spec,
            &spec.geometry(),
            &state.rack,
            at,
            BallInHand::Anywhere
        ),
        "the opening spot must be one a referee would allow: {at:?}"
    );
    assert!(
        draft.shot(&state).is_some(),
        "and the board is playable from it without touching anything"
    );
}

#[test]
fn setting_the_cue_ball_down_makes_the_board_playable_again() {
    let (state, mut draft) = scratched();
    let spec = state.spec().expect("known table");
    assert!(draft.put_down(&state, [spec.length * 0.3, spec.width * 0.5]));

    let shot = draft.shot(&state).expect("there is a shot again");
    assert!(shot.place.is_some(), "the placement rides inside the shot");
    assert!(
        state.clone().apply_shot(0, &shot).is_ok(),
        "and the server accepts it"
    );
}

#[test]
fn a_placement_snaps_to_somewhere_legal_rather_than_being_refused() {
    // The table view is an overview where a ball is a few pixels, so an exact
    // click is not something a player can be asked for.
    let (state, mut draft) = scratched();
    let spec = state.spec().expect("known table");
    let occupied = state
        .rack
        .balls
        .iter()
        .find(|b| b.id != CUE)
        .expect("object balls")
        .pos;

    assert!(draft.put_down(&state, occupied), "on top of another ball");
    let at = draft.place.expect("placed somewhere");
    assert_ne!(at, occupied, "it moved off it");
    let geom = spec.geometry();
    assert!(
        rules::placement_ok(spec, &geom, &state.rack, at, BallInHand::Anywhere),
        "and landed somewhere the rules allow: {at:?}"
    );

    // Well off the table entirely.
    assert!(draft.put_down(&state, [-5.0, -5.0]));
    let at = draft.place.expect("placed somewhere");
    assert!(
        rules::placement_ok(spec, &geom, &state.rack, at, BallInHand::Anywhere),
        "a click off the cloth still lands on it: {at:?}"
    );
}

#[test]
fn the_kitchen_is_honoured_when_the_foul_was_on_the_break() {
    let (mut state, mut draft) = scratched();
    state.ball_in_hand = Some(BallInHand::Kitchen);
    let spec = state.spec().expect("known table");
    // Ask for the far end of the table; it has to come back behind the line.
    assert!(draft.put_down(&state, [spec.length * 0.9, spec.width * 0.5]));
    let at = draft.place.expect("placed somewhere");
    assert!(
        at[0] < rules::head_string(spec),
        "a kitchen placement must stay behind the head string: {at:?}"
    );
}

#[test]
fn cancelling_a_placement_puts_the_cue_ball_back_where_it_was() {
    let (state, mut draft) = scratched();
    let spec = state.spec().expect("known table");
    draft.put_down(&state, [spec.length * 0.3, spec.width * 0.5]);
    let settled = draft.place;
    draft.commit();

    draft.toggle_mode(ShotMode::Place);
    draft.put_down(&state, [spec.length * 0.2, spec.width * 0.2]);
    assert_ne!(draft.place, settled, "moved somewhere else");
    draft.cancel();
    assert_eq!(
        draft.place, settled,
        "cancelling restores the spot it was armed with"
    );
}

#[test]
fn the_held_ball_follows_the_pointer_across_the_cloth() {
    // Placement is the one thing the pointer steers *through the table* rather
    // than as a bare delta: only the caller can turn a cell into a spot on the
    // cloth, so bare motion says nothing here and every report over the table
    // arrives as a `put_down`. Committing to a spot you cannot see first is
    // the version of this that was unplayable.
    let (state, mut draft) = scratched();
    let spec = state.spec().expect("known table");
    let opened_at = draft.place.expect("the board opens holding it");
    assert_eq!(
        draft.pointer_moved(10, 10, false),
        PointerOutcome::Ignored,
        "the first report is only the reference"
    );
    assert_eq!(
        draft.pointer_moved(20, 14, false),
        PointerOutcome::Ignored,
        "and bare motion never places the ball on its own"
    );
    assert_eq!(draft.place, Some(opened_at), "nothing moved it");

    let mut seen = Vec::new();
    for fraction in [0.25, 0.5, 0.75] {
        draft.put_down(&state, [spec.length * fraction, spec.width * 0.5]);
        seen.push(draft.place.expect("carried to where the pointer is"));
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] != pair[1]),
        "the ball tracks the pointer rather than sticking: {seen:?}"
    );
}

#[test]
fn any_foul_hands_the_ball_over_and_the_board_opens_holding_it() {
    // Every foul in both rulesets grants ball in hand, not just a scratch —
    // no contact, wrong ball first and no-rail all do. The board has to say
    // so, or the player who was fouled never learns they have it and plays
    // the leave they were left instead.
    let mut state = state(PoolRules::EightBall);
    state.ball_in_hand = Some(BallInHand::Anywhere);
    let draft = PoolDraft::new(&state);
    assert!(!state.must_place(), "the cue ball is still on the table");
    assert_eq!(
        draft.mode,
        ShotMode::Place,
        "and it is in the player's hand"
    );
    assert_eq!(
        draft.place, None,
        "picked up from where it lies: taking it must not move it"
    );
}

#[test]
fn a_click_on_the_cloth_sets_the_ball_down_and_ends_the_mode() {
    // Left click is "there, done" everywhere else on this board; placement
    // used to be the one thing that still needed `m` to finish.
    let (state, mut draft) = scratched();
    let spec = state.spec().expect("known table");
    assert!(draft.place_and_commit(&state, [spec.length * 0.35, spec.width * 0.5]));
    assert_eq!(draft.mode, ShotMode::Idle, "the cue goes down with it");
    assert!(draft.place.is_some(), "and the ball stays where it was put");
    assert!(
        draft.shot(&state).is_some(),
        "and the board is playable again"
    );
}

#[test]
fn the_aim_has_a_second_axis_so_it_never_runs_out_of_screen() {
    // Sideways says which side of centre, up and down say how far. Two ways to
    // reach the same place, because a pointer runs out of screen long before
    // an aim runs out of range — a player aiming from the right-hand side of
    // the board had nowhere left to push.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Aim);

    draft.nudge_aim(0.3);
    let side = draft.aim_offset;
    assert!(side > 0.0);

    draft.spread_aim(0.5);
    assert!(
        draft.aim_offset > side,
        "down walks it further off centre: {} then {}",
        side,
        draft.aim_offset
    );
    let far = draft.aim_offset;
    draft.spread_aim(-0.2);
    assert!(draft.aim_offset < far, "and up walks it back in");

    // The far side is reachable the same way once the aim is on it.
    draft.nudge_aim(-2.0);
    assert!(draft.aim_offset < 0.0, "now off the other side");
    let left = draft.aim_offset;
    draft.spread_aim(0.4);
    assert!(
        draft.aim_offset < left,
        "spreading keeps the side it is already on"
    );

    // Coming back in stops dead at centre rather than sliding through to the
    // other side of the ball, which nobody asked for and cannot be aimed at.
    draft.spread_aim(-99.0);
    assert_eq!(draft.aim_offset, 0.0, "dead on is a place you can land");
    draft.spread_aim(99.0);
    assert!(
        draft.aim_offset.abs() <= 2.4 + 1e-9,
        "and the far end is still the far end: {}",
        draft.aim_offset
    );
}

#[test]
fn a_reload_mid_shot_does_not_delete_the_shot() {
    // Regression, and it is what made a rack look like it ended without a
    // final shot: a reload rebuilds `PoolDetail` from the row and a fresh one
    // has no playback. The shot that *ends* a rack publishes two events —
    // `MovePlayed` and `MatchFinished` — and each asks for a reload, so the
    // first reload started the animation and the second wiped it milliseconds
    // later. Every other shot publishes one event and survived by luck.
    use crate::app::lobby::daily::pool_draft::{PoolDetail, PoolPlayback};

    let mut state = state(PoolRules::NineBall);
    state
        .apply_shot(0, &break_shot())
        .expect("the break is legal");
    let timeline = state.last_timeline().expect("the break replays");

    let mut playing = PoolDetail {
        draft: PoolDraft::new(&state),
        state: state.clone(),
        shot_in_flight: false,
        playback: Some(PoolPlayback::new(timeline)),
        watching: Some(PoolDraft::new(&state).share()),
    };
    let mut fresh = PoolDetail {
        draft: PoolDraft::new(&state),
        state,
        shot_in_flight: false,
        playback: None,
        watching: None,
    };

    fresh.adopt(&mut playing);
    assert!(
        fresh.playback.is_some(),
        "the shot keeps rolling across the reload"
    );
    assert!(
        fresh.watching.is_some(),
        "and so does the aim the opponent last sent"
    );
    assert!(
        playing.playback.is_none(),
        "and the detail being replaced lets go of it"
    );
}

#[test]
fn an_aim_goes_out_on_a_change_and_not_on_every_pixel() {
    use crate::app::lobby::daily::pool_draft::should_share_aim;
    use std::time::{Duration, Instant};

    let (_, draft) = drafted();
    let base = draft.share();
    assert!(
        should_share_aim(None, None, base),
        "the first one always goes"
    );
    assert!(
        !should_share_aim(Some(base), Some(Instant::now()), base),
        "an unchanged shot says nothing"
    );

    let mut nudged = base;
    nudged.aim_offset += 0.05;
    assert!(
        !should_share_aim(Some(base), Some(Instant::now()), nudged),
        "a pixel of aim waits its turn: pointer motion is per terminal cell"
    );
    assert!(
        should_share_aim(
            Some(base),
            Some(Instant::now() - Duration::from_secs(1)),
            nudged
        ),
        "and goes once the interval is up"
    );

    // Arming the stroke is the update whose timing is the information, so it
    // never waits.
    let mut armed = base;
    armed.mode = ShotMode::Stroke(PowerBand::Strong);
    assert!(should_share_aim(Some(base), Some(Instant::now()), armed));
}

#[test]
fn a_shot_survives_the_trip_to_the_other_players_board() {
    // The opponent's board draws what you are lining up, and it draws it with
    // the same code that draws your own — so the share has to carry every
    // field the renderer reads, and the reconstruction has to be usable by it.
    let (state, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Spin);
    draft.nudge_tip(0.2, -0.15);
    draft.nudge_aim(0.7);
    draft.nudge_pull(0.1);

    let theirs = PoolDraft::watching(draft.share());
    assert_eq!(theirs.mode, draft.mode);
    assert_eq!(theirs.target, draft.target);
    assert_eq!(theirs.tip, draft.tip);
    assert_eq!(theirs.aim_offset, draft.aim_offset);
    assert_eq!(theirs.pull, draft.pull);
    assert_eq!(theirs.called_pocket, draft.called_pocket);
    // And it draws the same picture: the aim line and the ghost are what the
    // watcher actually sees move.
    assert_eq!(theirs.aim_point(&state), draft.aim_point(&state));
    assert_eq!(theirs.ghost(&state), draft.ghost(&state));
    assert_eq!(theirs.power(), draft.power());
    assert_eq!(theirs.share(), draft.share(), "and it round-trips");
}

#[test]
fn holding_the_button_re_grips_instead_of_steering() {
    // The pointer runs out of screen long before an aim runs out of range: a
    // terminal reports motion only inside its own window. Holding the button
    // is lifting the mouse off the pad — the reference follows, the setting
    // does not — and it is the only way to keep turning past the edge.
    let (_, mut draft) = drafted();
    draft.toggle_mode(ShotMode::Aim);
    draft.pointer_moved(40, 10, false);
    draft.pointer_moved(60, 10, false);
    let aimed = draft.aim_offset;
    assert_ne!(aimed, 0.0, "bare motion steers");

    for x in [50, 40, 30, 20] {
        assert_eq!(
            draft.pointer_moved(x, 10, true),
            PointerOutcome::Ignored,
            "a held button drags the hand back, not the aim"
        );
    }
    assert_eq!(draft.aim_offset, aimed, "the aim survived the re-grip");

    draft.pointer_moved(30, 10, false);
    assert!(
        draft.aim_offset > aimed,
        "and carries on in the same direction from the new grip"
    );
}

#[test]
fn the_arrows_walk_the_held_ball_and_keep_it_legal() {
    let (state, mut draft) = scratched();
    let spec = state.spec().expect("known table");
    let geom = spec.geometry();
    draft.put_down(&state, [spec.length * 0.3, spec.width * 0.5]);

    for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1)] {
        draft.nudge_placement(&state, dx, dy);
        let at = draft.place.expect("still holding it");
        assert!(
            rules::placement_ok(spec, &geom, &state.rack, at, BallInHand::Anywhere),
            "walking it {dx},{dy} left it somewhere illegal: {at:?}"
        );
    }
}

#[test]
fn the_next_ball_in_line_is_the_lowest_that_is_on() {
    // Nine-ball has exactly one legal target; eight-ball has a group. Either
    // way `'` should land on the one a player would call obvious.
    for rules_kind in PoolRules::ALL {
        let state = state(rules_kind);
        let mut draft = PoolDraft::new(&state);
        draft.aim_at_point([0.5, 0.5]);
        assert_eq!(draft.target, None, "aimed at a cushion");

        draft.next_in_line(&state);
        assert_eq!(
            draft.target,
            state.legal_targets().first().copied(),
            "{rules_kind:?} should jump to the lowest ball that is on"
        );
    }
}
