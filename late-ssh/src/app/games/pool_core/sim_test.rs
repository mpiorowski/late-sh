//! Behavioural tests for the physics kernel.
//!
//! These pin the things a player would notice — that draw comes back, that
//! follow runs through, that a stun shot leaves the cue ball where it hit —
//! rather than exact coordinates. A break is chaotic in the technical sense:
//! it is perfectly *reproducible* (see `determinism_test.rs`) but a one-ULP
//! change in the strike moves the final positions visibly, so asserting a
//! position after several collisions would be asserting noise.
//!
//! **Measure from the timeline, not from the settled rack.** A pool table is
//! small and a struck ball has enough energy to cross it several times, so by
//! the time everything stops the object ball has usually come back and hit
//! the cue ball again. Sampling a fixed interval after the contact event is
//! the only way to see what the contact itself did.
//!
//! Tolerances, loosest last:
//!
//! | what | tolerance |
//! |---|---|
//! | a single closed-form advance | `1e-9` |
//! | a single impulse | `1e-12` |
//! | multi-contact shot outcome | `1e-6` |
//! | behavioural geometry | 0.01 m, a few degrees |

use crate::app::games::pool_core::{
    ball::{Ball, CUE, Motion},
    cue::{NATURAL_ROLL_TIP, Strike},
    rack,
    shot::{RackState, ShotEvent},
    sim,
    table::{BAR_BOX_7FT, Geometry, TableSpec},
};

const SPEC: TableSpec = BAR_BOX_7FT;

fn geom() -> Geometry {
    SPEC.geometry()
}

/// A rack holding only the balls given, so a test can isolate one contact.
fn rack_of(balls: &[(u8, [f64; 2])]) -> RackState {
    RackState {
        balls: balls.iter().map(|(id, p)| Ball::resting(*id, *p)).collect(),
    }
}

fn shoot(start: &RackState, azimuth: f64, tip: [f64; 2], speed: f64) -> sim::SimResult {
    let strike = Strike::new(azimuth, tip[0], tip[1], speed).expect("valid strike");
    sim::simulate(&SPEC, &geom(), start, &strike)
}

/// Where ball `id` is `t` seconds into the shot.
fn at(result: &sim::SimResult, id: u8, t: f64) -> [f64; 2] {
    result
        .timeline
        .sample(t)
        .into_iter()
        .find(|f| f.id == id)
        .map(|f| f.pos)
        .expect("ball in the timeline")
}

/// When the cue ball first touched an object ball.
fn contact_time(result: &sim::SimResult) -> f64 {
    result
        .timeline
        .events
        .iter()
        .find_map(|e| match e.event {
            ShotEvent::BallHitBall { a, b, .. } if a == CUE || b == CUE => Some(e.t),
            _ => None,
        })
        .expect("the cue ball should have hit something")
}

/// When ball `id` first reached a cushion.
fn cushion_time(result: &sim::SimResult, id: u8) -> f64 {
    result
        .timeline
        .events
        .iter()
        .find_map(|e| match e.event {
            ShotEvent::BallHitCushion { ball, .. } if ball == id => Some(e.t),
            _ => None,
        })
        .expect("should have reached a cushion")
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn mid_table() -> [f64; 2] {
    [SPEC.length * 0.3, SPEC.width / 2.0]
}

// ── The strike model ──────────────────────────────────────────────────

#[test]
fn natural_roll_tip_rolls_from_the_off() {
    // The 0.4 radii figure is derived, not measured: if this fails the signs
    // in `Strike::apply` are wrong and every spin test below is meaningless.
    let mut ball = Ball::resting(CUE, mid_table());
    Strike::new(0.0, 0.0, NATURAL_ROLL_TIP, 2.0)
        .unwrap()
        .apply(&mut ball, &SPEC);
    let slip = ball.slip(SPEC.ball_radius);
    assert!(
        slip[0].abs() < 1e-9 && slip[1].abs() < 1e-9,
        "expected no slip at the natural-roll tip offset, got {slip:?}"
    );
    assert_eq!(ball.motion(SPEC.ball_radius), Motion::Rolling);
}

#[test]
fn centre_ball_strike_slides_first() {
    let mut ball = Ball::resting(CUE, mid_table());
    Strike::new(0.0, 0.0, 0.0, 2.0)
        .unwrap()
        .apply(&mut ball, &SPEC);
    assert_eq!(ball.motion(SPEC.ball_radius), Motion::Sliding);
    assert_eq!(ball.spin, [0.0; 3]);
}

#[test]
fn tip_left_gives_clockwise_english() {
    let mut ball = Ball::resting(CUE, mid_table());
    Strike::new(0.0, 0.3, 0.0, 2.0)
        .unwrap()
        .apply(&mut ball, &SPEC);
    assert!(
        ball.spin[2] < 0.0,
        "tip left of centre should spin clockwise from above"
    );
}

#[test]
fn miscue_is_refused() {
    assert!(
        Strike::new(0.0, 0.5, 0.5, 2.0).is_err(),
        "past the miscue limit"
    );
    assert!(Strike::new(0.0, 0.0, 0.0, 0.0).is_err(), "zero speed");
    assert!(Strike::new(0.0, 0.0, 0.0, 99.0).is_err(), "absurd speed");
    assert!(Strike::new(0.0, 0.3, 0.3, 2.0).is_ok(), "inside the limit");
}

// ── Spin on the cloth ─────────────────────────────────────────────────

/// How far the cue ball moved along the shot line in the 0.6 s after it hit
/// the object ball. Negative means it came back.
///
/// The window is what makes this measurable: the object ball is sent down the
/// long axis and needs well over 0.6 s to reach the far rail and return, so
/// nothing else touches the cue ball inside it.
fn cue_travel_after_contact(tip_vert: f64, gap: f64) -> f64 {
    let cue_at = [SPEC.length * 0.15, SPEC.width / 2.0];
    let object_at = [cue_at[0] + gap, cue_at[1]];
    let start = rack_of(&[(CUE, cue_at), (1, object_at)]);
    let result = shoot(&start, 0.0, [0.0, tip_vert], 2.0);
    assert!(!result.outcome.truncated, "shot should settle");
    assert_eq!(
        result.outcome.first_contact,
        Some(1),
        "should hit the object ball"
    );

    let t = contact_time(&result);
    at(&result, CUE, t + 0.6)[0] - at(&result, CUE, t)[0]
}

#[test]
fn draw_brings_the_cue_ball_back() {
    let travel = cue_travel_after_contact(-NATURAL_ROLL_TIP, 0.3);
    assert!(
        travel < -0.05,
        "draw should reverse the cue ball, travelled {travel:+.3} m"
    );
}

#[test]
fn follow_drives_the_cue_ball_through() {
    let travel = cue_travel_after_contact(NATURAL_ROLL_TIP, 0.3);
    assert!(
        travel > 0.05,
        "follow should carry the cue ball on, travelled {travel:+.3} m"
    );
}

#[test]
fn stun_leaves_the_cue_ball_near_the_contact() {
    // Close enough that the cue ball is still sliding with almost no spin
    // when it arrives: a stun only stops dead before the ball picks up roll.
    // Further out the same tip gives stun run-through, which is real pool and
    // is what `draw_follow_and_stun_are_ordered` measures instead.
    let travel = cue_travel_after_contact(0.0, 0.08);
    assert!(
        travel.abs() < 0.05,
        "stun should stop near the contact, travelled {travel:+.3} m"
    );
}

#[test]
fn draw_follow_and_stun_are_ordered() {
    let gap = 0.3;
    let draw = cue_travel_after_contact(-NATURAL_ROLL_TIP, gap);
    let stun = cue_travel_after_contact(0.0, gap);
    let follow = cue_travel_after_contact(NATURAL_ROLL_TIP, gap);
    assert!(
        draw < stun && stun < follow,
        "draw {draw:+.3} < stun {stun:+.3} < follow {follow:+.3}"
    );
}

// ── Geometry a player relies on ───────────────────────────────────────

#[test]
fn ninety_degree_rule_holds_on_a_stun_cut() {
    // A cue ball still sliding when it arrives leaves a cut at right angles
    // to the object ball. The gap has to be short: the rule holds for a *true*
    // stun, and a cue ball that has had room to pick up roll comes off the cut
    // noticeably forward of the tangent — at 10 cm the separation is already
    // down to about 81 degrees, which is real pool, not a bug.
    let cue_at = [SPEC.length * 0.3, SPEC.width * 0.5];
    let r = SPEC.ball_radius;
    // Half-ball cut: at contact the centres are 2R apart with a perpendicular
    // offset of R, so the object ball leaves 30 degrees off the shot line.
    let object_at = [cue_at[0] + 0.07, cue_at[1] + r];
    let start = rack_of(&[(CUE, cue_at), (1, object_at)]);
    let result = shoot(&start, 0.0, [0.0, 0.0], 2.2);
    assert_eq!(result.outcome.first_contact, Some(1));

    let t = contact_time(&result);
    let heading = |id: u8| {
        let a = at(&result, id, t + 0.02);
        let b = at(&result, id, t + 0.08);
        (b[1] - a[1]).atan2(b[0] - a[0])
    };
    let separation = (heading(CUE) - heading(1)).abs().to_degrees();
    // The rule is a rule of thumb, and two modelled effects eat into it in
    // exactly the way a real table does. Restitution below 1 leaves the cue
    // ball a few percent of its normal component, worth about 2.5 degrees on
    // its own; throw off the cut and the sliver of roll picked up on the way
    // in account for the rest. A band this wide still catches the failure
    // that matters — losing the tangent line entirely reads as 30 or 0.
    assert!(
        (separation - 90.0).abs() < 8.0,
        "expected roughly a right angle off a stun cut, got {separation:.1} degrees"
    );
}

#[test]
fn a_straight_shot_pots() {
    // Cue, object, and the middle of a corner pocket on one line.
    let g = geom();
    let pocket = g.pockets[2];
    let object_at = [pocket.center[0] - 0.35, pocket.center[1] - 0.35];
    let cue_at = [object_at[0] - 0.35, object_at[1] - 0.35];
    let aim = (pocket.center[1] - cue_at[1]).atan2(pocket.center[0] - cue_at[0]);
    let start = rack_of(&[(CUE, cue_at), (1, object_at)]);
    let result = shoot(&start, aim, [0.0, 0.0], 2.5);
    assert!(
        result.outcome.was_potted(1),
        "straight shot should drop, potted {:?}",
        result.outcome.potted
    );
}

#[test]
fn a_ball_along_the_rail_passes_the_side_pocket() {
    // Real tables let a ball hug the long rail past the middle pockets. If
    // this fails the side mouths are swallowing everything that rolls by.
    // Rolled just hard enough to clear the middle and stop short of the far
    // rail, so nothing but the side pocket is in play.
    let y = SPEC.ball_radius;
    let start = rack_of(&[(CUE, [SPEC.length * 0.15, y])]);
    let result = shoot(&start, 0.0, [0.0, NATURAL_ROLL_TIP], 0.6);
    let end = result.rack.get(CUE).expect("cue ball").pos;
    assert!(
        !result.outcome.cue_potted,
        "rail-hugging ball should not drop in the side"
    );
    assert!(
        end[0] > SPEC.length * 0.55,
        "should have rolled past the middle, stopped at x = {:.3}",
        end[0]
    );
}

#[test]
fn a_ball_into_the_corner_along_the_rail_drops() {
    let y = SPEC.ball_radius;
    let start = rack_of(&[(CUE, [SPEC.length * 0.5, y])]);
    let result = shoot(&start, std::f64::consts::PI, [0.0, NATURAL_ROLL_TIP], 2.0);
    assert!(result.outcome.cue_potted, "should reach the corner pocket");
}

/// Straight into the top rail from a spot that is *not* level with a side
/// pocket, and where the ball comes off it 0.3 s later.
fn rail_rebound_x(tip_side: f64) -> f64 {
    let start = rack_of(&[(CUE, [SPEC.length * 0.3, SPEC.width * 0.35])]);
    let result = shoot(&start, std::f64::consts::FRAC_PI_2, [tip_side, 0.0], 1.2);
    assert!(!result.outcome.cue_potted, "should bounce, not drop");
    let t = cushion_time(&result, CUE);
    at(&result, CUE, t + 0.3)[0]
}

#[test]
fn a_cushion_rebounds_roughly_mirror() {
    let x = rail_rebound_x(0.0);
    assert!(
        (x - SPEC.length * 0.3).abs() < 0.02,
        "no english should mean no sideways deflection, ended at x = {x:.3}"
    );
}

#[test]
fn english_deflects_a_cushion_rebound() {
    let straight = rail_rebound_x(0.0);
    let spun = rail_rebound_x(0.35);
    assert!(
        (spun - straight).abs() > 0.01,
        "english should change where the ball comes off the rail: \
         straight x = {straight:.3}, spun x = {spun:.3}"
    );
}

// ── Invariants over a lot of shots ────────────────────────────────────

/// Every ball is either potted or inside the cushions.
fn assert_all_contained(result: &sim::SimResult) {
    let g = geom();
    let r = SPEC.ball_radius;
    for ball in result.rack.on_table() {
        for cushion in &g.cushions {
            let (d, _) = Geometry::cushion_separation(cushion, ball.pos);
            assert!(
                d >= r - 1e-6,
                "ball {} sank into a cushion by {:.3e} m",
                ball.id,
                r - d
            );
        }
        assert!(
            ball.pos[0] > -0.2 && ball.pos[0] < SPEC.length + 0.2,
            "ball {} escaped along x at {:?}",
            ball.id,
            ball.pos
        );
        assert!(
            ball.pos[1] > -0.2 && ball.pos[1] < SPEC.width + 0.2,
            "ball {} escaped along y at {:?}",
            ball.id,
            ball.pos
        );
    }
}

/// No two balls left meaningfully overlapping.
///
/// A tenth of a millimetre, not zero: the stall guard parks creeping balls
/// where they stand, so a settled cluster can hold a micron of overlap that
/// no renderer can draw and no later shot can notice (a resting contact is
/// not "closing", so it is never resolved again).
fn assert_no_overlap(result: &sim::SimResult) {
    let on: Vec<_> = result.rack.on_table().collect();
    for i in 0..on.len() {
        for j in (i + 1)..on.len() {
            let d = dist(on[i].pos, on[j].pos);
            assert!(
                d >= 2.0 * SPEC.ball_radius - 1e-4,
                "balls {} and {} overlap by {:.3e} m",
                on[i].id,
                on[j].id,
                2.0 * SPEC.ball_radius - d
            );
        }
    }
}

#[test]
fn breaks_settle_and_stay_on_the_table() {
    for seed in 0..12u64 {
        let start = rack::build(&SPEC, rack::RackKind::EightBall, seed);
        let result = shoot(&start, 0.0, [0.0, 0.0], 7.0);
        assert!(!result.outcome.truncated, "break {seed} did not settle");
        assert!(
            result.outcome.duration < 40.0,
            "break {seed} ran {:.1}s",
            result.outcome.duration
        );
        assert_all_contained(&result);
        assert_no_overlap(&result);
    }
}

#[test]
fn fuzzed_shots_always_terminate_cleanly() {
    // A cheap stand-in for an rng: the seeds are arbitrary but fixed, so a
    // failure here is always reproducible.
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for i in 0..40u64 {
        let start = rack::build(&SPEC, rack::RackKind::NineBall, i);
        let azimuth = (next() % 1000) as f64 / 1000.0 * std::f64::consts::TAU;
        let side = ((next() % 1000) as f64 / 1000.0 - 0.5) * 0.6;
        let vert = ((next() % 1000) as f64 / 1000.0 - 0.5) * 0.6;
        let speed = 0.5 + (next() % 1000) as f64 / 1000.0 * 7.0;
        let Ok(strike) = Strike::new(azimuth, side, vert, speed) else {
            continue;
        };
        let result = sim::simulate(&SPEC, &geom(), &start, &strike);
        assert!(!result.outcome.truncated, "fuzz shot {i} hit the step cap");
        assert_all_contained(&result);
        assert_no_overlap(&result);
    }
}

#[test]
fn a_rolling_ball_runs_a_plausible_distance() {
    // Sanity on the rolling resistance. Gentle enough to stop before the far
    // rail, so this measures the cloth and nothing else: v² / 2μg with
    // μ_r = 0.015 is about 1.2 m.
    let start = rack_of(&[(CUE, [SPEC.ball_radius * 2.0, SPEC.width * 0.5])]);
    let result = shoot(&start, 0.0, [0.0, NATURAL_ROLL_TIP], 0.6);
    let travelled = result.rack.get(CUE).expect("cue ball").pos[0] - SPEC.ball_radius * 2.0;
    assert!(
        (0.9..1.6).contains(&travelled),
        "a 0.6 m/s roll travelled {travelled:.2} m, which is not a pool table"
    );
}

#[test]
fn no_impact_is_faster_than_the_strike() {
    // Energy can only leave the table. The bug this pins was subtle and total:
    // resolving the cushion's normal impulse along the horizontal while
    // building friction in the tilted contact frame let friction push the ball
    // *off* the rail, so a 5.6 m/s strike settled into bouncing between two
    // cushions at a constant 8.8 m/s and never stopped. Nothing else in the
    // suite noticed, because every individual shot still looked plausible.
    for (azimuth, tip) in [
        (0.7, [0.0, 0.0]),
        (0.7, [0.4, 0.0]),
        (2.959, [-0.3, 0.2]),
        (1.2, [0.35, -0.35]),
    ] {
        let speed = 5.6;
        let start = rack_of(&[(CUE, [SPEC.length * 0.3, SPEC.width * 0.4])]);
        let result = shoot(&start, azimuth, tip, speed);
        assert!(!result.outcome.truncated, "shot at {azimuth} should settle");
        for event in &result.timeline.events {
            let (what, got) = match event.event {
                ShotEvent::BallHitCushion { speed, .. } => ("cushion", speed),
                ShotEvent::BallHitBall { speed, .. } => ("ball", speed),
                ShotEvent::BallPotted { .. } => continue,
            };
            assert!(
                got <= speed + 1e-6,
                "{what} impact at {got:.3} m/s exceeds the {speed:.1} m/s strike \
                 (t = {:.3}, azimuth {azimuth})",
                event.t
            );
        }
    }
}
