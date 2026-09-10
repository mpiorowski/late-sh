//! The ruleset-agnostic pieces: cue ball placement, and one game played out
//! end to end by a trivial bot.
//!
//! The self-played rack is the only test that puts the physics and the rules
//! together. It does not check that the bot plays well — it checks that the
//! two layers agree well enough that a rack *finishes*: that legal targets
//! stay reachable, that fouls hand the table over, that spotting works, and
//! that nothing wedges.

use crate::app::games::pool_core::{
    ball::{Ball, CUE},
    cue::Strike,
    rack,
    rules::{
        BallInHand, GameState, PoolRules, Turn, break_was_legal, free_spot, head_string,
        other_seat, placement_ok,
    },
    shot::{Pot, RackState, ShotOutcome},
    sim,
    table::{BAR_BOX_7FT, Geometry, TableSpec},
};

const SPEC: TableSpec = BAR_BOX_7FT;

fn geom() -> Geometry {
    SPEC.geometry()
}

// ── Cue ball placement ────────────────────────────────────────────────

fn empty_rack() -> RackState {
    RackState {
        balls: vec![Ball::resting(CUE, [0.5, 0.5])],
    }
}

#[test]
fn the_middle_of_the_table_is_always_a_legal_spot() {
    let at = [SPEC.length / 2.0, SPEC.width / 2.0];
    assert!(placement_ok(
        &SPEC,
        &geom(),
        &empty_rack(),
        at,
        BallInHand::Anywhere
    ));
}

#[test]
fn a_spot_off_the_table_is_refused() {
    let g = geom();
    for at in [
        [-0.1, SPEC.width / 2.0],
        [SPEC.length + 0.1, SPEC.width / 2.0],
        [SPEC.length / 2.0, -0.1],
        [SPEC.length / 2.0, SPEC.width + 0.1],
        // Lined up with a side pocket mouth, which is a gap in the rails —
        // the cushion loop alone would wave this through.
        [SPEC.length / 2.0, -0.02],
    ] {
        assert!(
            !placement_ok(&SPEC, &g, &empty_rack(), at, BallInHand::Anywhere),
            "{at:?} is not on the table"
        );
    }
}

#[test]
fn a_spot_inside_a_cushion_is_refused() {
    let at = [SPEC.length / 2.0, SPEC.ball_radius * 0.5];
    assert!(!placement_ok(
        &SPEC,
        &geom(),
        &empty_rack(),
        at,
        BallInHand::Anywhere
    ));
}

#[test]
fn a_spot_on_top_of_another_ball_is_refused() {
    let mut rack = empty_rack();
    let occupied = [SPEC.length / 2.0, SPEC.width / 2.0];
    rack.balls.push(Ball::resting(3, occupied));
    let g = geom();
    assert!(
        !placement_ok(&SPEC, &g, &rack, occupied, BallInHand::Anywhere),
        "cannot sit on the 3"
    );
    let touching = [occupied[0] + SPEC.ball_radius, occupied[1]];
    assert!(
        !placement_ok(&SPEC, &g, &rack, touching, BallInHand::Anywhere),
        "overlapping the 3 is still overlapping"
    );
    let clear = [occupied[0] + 3.0 * SPEC.ball_radius, occupied[1]];
    assert!(placement_ok(&SPEC, &g, &rack, clear, BallInHand::Anywhere));
}

#[test]
fn the_kitchen_restriction_is_the_head_string() {
    let g = geom();
    let behind = [head_string(&SPEC) - 0.05, SPEC.width / 2.0];
    let past = [head_string(&SPEC) + 0.05, SPEC.width / 2.0];
    assert!(placement_ok(
        &SPEC,
        &g,
        &empty_rack(),
        behind,
        BallInHand::Kitchen
    ));
    assert!(!placement_ok(
        &SPEC,
        &g,
        &empty_rack(),
        past,
        BallInHand::Kitchen
    ));
    assert!(
        placement_ok(&SPEC, &g, &empty_rack(), past, BallInHand::Anywhere),
        "the same spot is fine without the restriction"
    );
}

// ── Break legality ────────────────────────────────────────────────────

#[test]
fn four_balls_to_a_rail_or_one_in_a_pocket() {
    let base = ShotOutcome {
        first_contact: Some(1),
        cushion_after_contact: true,
        duration: 3.0,
        ..ShotOutcome::default()
    };

    let three_rails = ShotOutcome {
        balls_to_rail: vec![0, 1, 2],
        ..base.clone()
    };
    assert!(!break_was_legal(&three_rails));

    let four_rails = ShotOutcome {
        balls_to_rail: vec![0, 1, 2, 3],
        ..base.clone()
    };
    assert!(break_was_legal(&four_rails));

    let one_potted = ShotOutcome {
        balls_to_rail: vec![0],
        potted: vec![Pot { ball: 5, pocket: 1 }],
        ..base.clone()
    };
    assert!(break_was_legal(&one_potted), "a pot is always enough");

    let only_the_cue = ShotOutcome {
        balls_to_rail: vec![0],
        potted: vec![Pot {
            ball: CUE,
            pocket: 1,
        }],
        cue_potted: true,
        ..base
    };
    assert!(
        !break_was_legal(&only_the_cue),
        "scratching is not 'potting a ball'"
    );
}

// ── A whole rack, played by a bot ─────────────────────────────────────

/// Aim at the ghost-ball point that would send `target` toward `pocket`.
fn ghost_ball_aim(rack: &RackState, target: u8, pocket: [f64; 2]) -> Option<f64> {
    let cue = rack.get(CUE)?.pos;
    let ball = rack.get(target)?.pos;
    let to_pocket = [pocket[0] - ball[0], pocket[1] - ball[1]];
    let len = (to_pocket[0].powi(2) + to_pocket[1].powi(2)).sqrt();
    if len == 0.0 {
        return None;
    }
    // The cue ball must arrive one diameter back along the pot line.
    let ghost = [
        ball[0] - to_pocket[0] / len * 2.0 * SPEC.ball_radius,
        ball[1] - to_pocket[1] / len * 2.0 * SPEC.ball_radius,
    ];
    Some((ghost[1] - cue[1]).atan2(ghost[0] - cue[0]))
}

/// Play a rack out with a bot that shoots the first legal target at the
/// nearest pocket. Returns the number of shots taken, or `None` if the rack
/// never finished.
///
/// The aim carries a small per-shot wobble. Without it the bot is a pure
/// function of the position, so the moment it fouls into a position it has
/// already seen it replays the same miss forever — deterministic physics and a
/// deterministic bot make a perfect loop. The wobble is derived from the shot
/// counter, so the test stays reproducible.
fn play_out(rules: PoolRules, seed: u64) -> Option<u32> {
    let g = geom();
    let mut state = GameState {
        rack: rack::build(&SPEC, rules.rack_kind(), seed),
        turn: 0,
        groups: None,
        shots_taken: 0,
        ball_in_hand: None,
        on_colour: false,
        free_ball: false,
    };

    for shot_number in 0..300u32 {
        // Re-spot the cue ball if it is down, and honour ball-in-hand by
        // simply putting it back on the break spot when that is legal.
        let cue_down = state.rack.get(CUE).is_none_or(|b| b.potted.is_some());
        if cue_down || state.ball_in_hand.is_some() {
            let zone = state.ball_in_hand.unwrap_or(BallInHand::Anywhere);
            let spot = free_spot(&SPEC, &g, &state.rack, rack::break_spot(&SPEC), zone)
                .expect("a table with room for the cue ball");
            if let Some(cue) = state.rack.balls.iter_mut().find(|b| b.id == CUE) {
                cue.potted = None;
                cue.pos = spot;
            }
            state.ball_in_hand = None;
        }

        let targets = rules.legal_targets(&state);
        let Some(&target) = targets.first() else {
            break;
        };

        // Nearest pocket to the target, so the bot at least sometimes pots.
        let ball_pos = state.rack.get(target).expect("target on table").pos;
        let (pocket_index, pocket) = g
            .pockets
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = (a.center[0] - ball_pos[0]).powi(2) + (a.center[1] - ball_pos[1]).powi(2);
                let db = (b.center[0] - ball_pos[0]).powi(2) + (b.center[1] - ball_pos[1]).powi(2);
                da.total_cmp(&db)
            })
            .expect("six pockets");

        let wobble = ((shot_number % 7) as f64 - 3.0) * 0.012;
        let azimuth = ghost_ball_aim(&state.rack, target, pocket.center).unwrap_or(0.0) + wobble;
        let speed = 2.2 + (shot_number % 5) as f64 * 0.15;
        let strike = Strike::new(azimuth, 0.0, 0.0, speed).expect("valid strike");
        let result = sim::simulate(&SPEC, &g, &state.rack, &strike);
        assert!(
            !result.outcome.truncated,
            "{rules:?} seed {seed} shot {shot_number} never settled"
        );

        let call = if rules.requires_call(&state) {
            Some(pocket_index as u8)
        } else {
            None
        };
        let ruling = rules.judge(&state, &result.outcome, call, false);

        state.rack = result.rack;
        state.shots_taken += 1;
        if let Some(groups) = ruling.group_assignment {
            state.groups = Some(groups);
        }
        // Re-spot anything the rules sent back up, on the foot spot.
        for id in &ruling.balls_to_spot {
            let Some(at) = free_spot(
                &SPEC,
                &g,
                &state.rack,
                rack::foot_spot(&SPEC),
                BallInHand::Anywhere,
            ) else {
                continue;
            };
            if let Some(ball) = state.rack.balls.iter_mut().find(|b| b.id == *id) {
                ball.potted = None;
                ball.pos = at;
            }
        }
        state.ball_in_hand = ruling.ball_in_hand;
        if ruling.winner.is_some() {
            return Some(state.shots_taken);
        }
        if ruling.turn == Turn::Pass {
            state.turn = other_seat(state.turn);
        }
    }
    None
}

#[test]
fn a_bot_can_play_eight_ball_to_a_finish() {
    for seed in 0..3u64 {
        let shots = play_out(PoolRules::EightBall, seed)
            .unwrap_or_else(|| panic!("eight-ball seed {seed} never reached a result"));
        assert!(shots > 0, "a rack cannot end before it starts");
    }
}

#[test]
fn a_bot_can_play_nine_ball_to_a_finish() {
    for seed in 0..3u64 {
        let shots = play_out(PoolRules::NineBall, seed)
            .unwrap_or_else(|| panic!("nine-ball seed {seed} never reached a result"));
        assert!(shots > 0, "a rack cannot end before it starts");
    }
}
