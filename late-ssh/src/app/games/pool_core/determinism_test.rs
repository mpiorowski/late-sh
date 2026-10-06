//! The replay contract.
//!
//! This is the only place in the kernel that asserts on raw bit patterns, and
//! it is deliberate. Everything the game does with a shot — showing the
//! opponent what happened, letting a spectator watch, reloading a board after
//! a restart — rests on re-simulating from the stored inputs and getting the
//! same answer. "Close enough" is not a contract; `f64::to_bits` is.
//!
//! Note what `round_trip_survives_the_database` does with rounding. Coordinates
//! are rounded on the way into Postgres, so the *reference* run has to be
//! rounded too. Skipping that turns the test into "does rounding change
//! anything", which it obviously does, instead of "does a rack that has been
//! through the database replay the same way", which is the thing that matters.

use crate::app::games::pool_core::{
    ball::CUE,
    cue::Strike,
    rack,
    shot::{PERSIST_DECIMALS, RackState, Shot},
    sim,
    table::{BAR_BOX_7FT, Geometry, TableSpec},
};

const SPEC: TableSpec = BAR_BOX_7FT;

fn geom() -> Geometry {
    SPEC.geometry()
}

fn play(start: &RackState, strike: &Strike) -> RackState {
    sim::simulate(&SPEC, &geom(), start, strike).rack
}

/// Every coordinate, as raw bits.
fn fingerprint(rack: &RackState) -> Vec<(u8, u64, u64, Option<u8>)> {
    rack.balls
        .iter()
        .map(|b| (b.id, b.pos[0].to_bits(), b.pos[1].to_bits(), b.potted))
        .collect()
}

#[test]
fn the_same_shot_always_lands_in_the_same_place() {
    let start = rack::build(&SPEC, rack::RackKind::EightBall, 7);
    let strike = Strike::new(0.02, 0.1, -0.2, 6.5).unwrap();
    let reference = fingerprint(&play(&start, &strike));
    for run in 1..100 {
        assert_eq!(
            fingerprint(&play(&start, &strike)),
            reference,
            "run {run} diverged from the first"
        );
    }
}

#[test]
fn a_seeded_rack_is_always_the_same_rack() {
    for seed in [0u64, 1, 42, u64::MAX] {
        let a = rack::build(&SPEC, rack::RackKind::EightBall, seed);
        let b = rack::build(&SPEC, rack::RackKind::EightBall, seed);
        assert_eq!(fingerprint(&a), fingerprint(&b), "seed {seed}");
    }
    let a = rack::build(&SPEC, rack::RackKind::EightBall, 1);
    let b = rack::build(&SPEC, rack::RackKind::EightBall, 2);
    assert_ne!(
        fingerprint(&a),
        fingerprint(&b),
        "different seeds should give different racks"
    );
}

#[test]
fn round_trip_survives_the_database() {
    let start = rack::build(&SPEC, rack::RackKind::NineBall, 3);
    let strike = Strike::new(0.05, -0.15, 0.25, 5.0).unwrap();

    // What actually gets stored.
    let stored = play(&start, &strike).rounded(PERSIST_DECIMALS);
    let json = serde_json::to_string(&stored).expect("serialises");
    let loaded: RackState = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(
        fingerprint(&loaded),
        fingerprint(&stored),
        "JSON is lossless"
    );

    // And the next shot off the reloaded rack matches the next shot off the
    // in-memory one — which is the property a reloaded board depends on.
    let next = Strike::new(1.9, 0.0, 0.3, 3.0).unwrap();
    assert_eq!(
        fingerprint(&play(&loaded, &next)),
        fingerprint(&play(&stored, &next)),
        "a rack that has been through the database must replay identically"
    );
}

#[test]
fn a_stored_game_replays_shot_for_shot() {
    // The shape a match actually takes: a list of shots against an opening
    // rack. Replaying it must reproduce every intermediate position, not just
    // the final one — a spectator opening the board mid-match re-derives the
    // state exactly this way.
    let start = rack::build(&SPEC, rack::RackKind::NineBall, 11);
    let shots: Vec<Shot> = (0..20)
        .map(|i| {
            let f = i as f64;
            Shot {
                place: None,
                azimuth: 0.3 + f * 0.41,
                tip: [(f * 0.17).sin() * 0.3, (f * 0.23).cos() * 0.3],
                speed: 1.5 + (f * 0.31).sin().abs() * 4.0,
                called_pocket: None,
                play_again: false,
            }
        })
        .collect();

    let replay = |shots: &[Shot]| {
        let mut state = start.clone();
        let mut trail = Vec::new();
        for shot in shots {
            let Ok(strike) = Strike::new(shot.azimuth, shot.tip[0], shot.tip[1], shot.speed) else {
                continue;
            };
            // A real match re-spots the cue ball when it is potted; do the
            // same here so the sequence keeps going rather than dead-ending.
            if let Some(cue) = state.balls.iter_mut().find(|b| b.id == CUE)
                && cue.potted.is_some()
            {
                cue.potted = None;
                cue.pos = rack::break_spot(&SPEC);
            }
            state = play(&state, &strike).rounded(PERSIST_DECIMALS);
            trail.push(fingerprint(&state));
        }
        trail
    };

    let first = replay(&shots);
    assert_eq!(
        first.len(),
        shots.len(),
        "every shot should have been played"
    );
    assert_eq!(
        replay(&shots),
        first,
        "the whole game must replay identically"
    );
}

#[test]
fn shots_serialise_losslessly() {
    let shot = Shot {
        place: Some([0.4123456, 0.2987654]),
        azimuth: 1.234_567_890_123,
        tip: [-0.321, 0.456],
        speed: 4.567_891_234,
        called_pocket: Some(3),
        play_again: false,
    };
    let round_tripped: Shot = serde_json::from_str(&serde_json::to_string(&shot).unwrap()).unwrap();
    assert_eq!(round_tripped, shot);
    assert_eq!(round_tripped.azimuth.to_bits(), shot.azimuth.to_bits());
    assert_eq!(round_tripped.speed.to_bits(), shot.speed.to_bits());
}
