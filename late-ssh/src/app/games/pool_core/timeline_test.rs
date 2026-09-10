//! The animation contract.
//!
//! The board screen samples a shot by wall-clock time, so `sample(t)` has to
//! agree with the frame grid it was built on. The invariant that matters is
//! that frame `i` is exactly `i / hz` seconds in — if the simulator ever
//! pushed a frame at an off-grid moment the whole animation would shear,
//! silently and only for shots that ended between frames.

use crate::app::games::pool_core::{
    ball::CUE,
    cue::Strike,
    rack,
    sim::{self, TIMELINE_HZ},
    table::{BAR_BOX_7FT, Geometry, TableSpec},
};

const SPEC: TableSpec = BAR_BOX_7FT;

fn geom() -> Geometry {
    SPEC.geometry()
}

fn a_break() -> sim::SimResult {
    let start = rack::build(&SPEC, rack::RackKind::EightBall, 5);
    let strike = Strike::new(0.01, 0.0, 0.1, 6.0).unwrap();
    sim::simulate(&SPEC, &geom(), &start, &strike)
}

#[test]
fn the_frame_grid_covers_the_whole_shot() {
    let result = a_break();
    let expected = (result.timeline.duration * TIMELINE_HZ).ceil() as usize + 1;
    assert_eq!(
        result.timeline.frame_count(),
        expected,
        "duration {:.4}s should map onto {expected} frames",
        result.timeline.duration
    );
}

#[test]
fn watching_the_shot_ends_before_the_physics_does() {
    // Friction is asymptotic, so the last stretch of a shot is balls creeping
    // fractions of a millimetre. Played back in full, that is seconds of a
    // table that looks settled while the board still refuses a shot — the
    // player concludes it is stuck. Playback ends when the picture stops.
    let result = a_break();
    let visible = result.timeline.visible_duration();
    assert!(
        visible > 0.0 && visible <= result.timeline.duration,
        "visible {visible} against a {}s shot",
        result.timeline.duration
    );

    // Everything that is worth seeing has already happened by then: the rack
    // at the end of the visible stretch is the settled rack, to well under a
    // pixel of table.
    for (seen, settled) in result
        .timeline
        .sample(visible)
        .iter()
        .zip(result.rack.balls.iter())
    {
        assert_eq!(seen.id, settled.id);
        if settled.potted.is_some() {
            continue;
        }
        let off = (seen.pos[0] - settled.pos[0]).hypot(seen.pos[1] - settled.pos[1]);
        assert!(off < 0.006, "ball {} still had {off}m to travel", seen.id);
    }
}

#[test]
fn sampling_past_the_end_returns_the_settled_rack() {
    let result = a_break();
    let settled = result.timeline.sample(result.timeline.duration + 5.0);
    for ball in &result.rack.balls {
        let frame = settled
            .iter()
            .find(|f| f.id == ball.id)
            .expect("ball present");
        assert_eq!(frame.potted, ball.potted.is_some(), "ball {}", ball.id);
        if ball.potted.is_none() {
            assert!(
                (frame.pos[0] - ball.pos[0]).abs() < 1e-9
                    && (frame.pos[1] - ball.pos[1]).abs() < 1e-9,
                "ball {} settled at {:?} but the last frame says {:?}",
                ball.id,
                ball.pos,
                frame.pos
            );
        }
    }
}

#[test]
fn sampling_before_the_start_returns_the_opening_rack() {
    let start = rack::build(&SPEC, rack::RackKind::EightBall, 5);
    let result = a_break();
    for t in [-1.0, 0.0] {
        let frame = result.timeline.sample(t);
        let cue = frame.iter().find(|f| f.id == CUE).expect("cue ball");
        let expected = start.get(CUE).unwrap().pos;
        assert!(
            (cue.pos[0] - expected[0]).abs() < 1e-9,
            "sample({t}) should be the opening rack"
        );
    }
}

#[test]
fn samples_land_on_their_own_frames() {
    let result = a_break();
    // Sampling exactly on a frame boundary must return that frame untouched,
    // not a blend of it with its neighbour.
    for i in [0usize, 1, 7, 40] {
        if i >= result.timeline.frame_count() {
            continue;
        }
        let t = i as f64 / TIMELINE_HZ;
        let sampled = result.timeline.sample(t);
        let direct = result.timeline.sample(t + 1e-12);
        for (a, b) in sampled.iter().zip(direct.iter()) {
            assert!(
                (a.pos[0] - b.pos[0]).abs() < 1e-6 && (a.pos[1] - b.pos[1]).abs() < 1e-6,
                "frame {i} is not stable under a nudge: {:?} vs {:?}",
                a.pos,
                b.pos
            );
        }
    }
}

#[test]
fn positions_move_continuously_between_samples() {
    let result = a_break();
    let step = 1.0 / TIMELINE_HZ;
    let mut previous = result.timeline.sample(0.0);
    let mut t = step;
    while t <= result.timeline.duration {
        let current = result.timeline.sample(t);
        for (a, b) in previous.iter().zip(current.iter()) {
            if a.potted || b.potted {
                // A potted ball teleports to the pocket mouth by design.
                continue;
            }
            let moved = ((b.pos[0] - a.pos[0]).powi(2) + (b.pos[1] - a.pos[1]).powi(2)).sqrt();
            // Nothing on a pool table outruns the cue's speed cap, so one
            // frame can never cover more than that times the frame time.
            assert!(
                moved < 12.0 * step * 1.5,
                "ball {} jumped {moved:.4} m between frames at t = {t:.3}",
                a.id
            );
        }
        previous = current;
        t += step;
    }
}

#[test]
fn every_event_lands_inside_the_shot() {
    let result = a_break();
    assert!(!result.timeline.events.is_empty(), "a break makes contacts");
    let mut last = 0.0;
    for event in &result.timeline.events {
        assert!(
            event.t >= 0.0 && event.t <= result.timeline.duration + 1e-9,
            "event at {:.4}s is outside a {:.4}s shot",
            event.t,
            result.timeline.duration
        );
        assert!(
            event.t >= last - 1e-9,
            "events should be in time order: {:.4} came after {:.4}",
            event.t,
            last
        );
        last = event.t;
    }
}
