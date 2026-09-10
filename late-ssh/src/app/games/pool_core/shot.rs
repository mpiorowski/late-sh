//! What a player sends, what the simulator hands back, and what the renderer
//! animates.
//!
//! The split matters for persistence. A shot can generate thousands of
//! integration steps and hundreds of contacts; storing that per move would put
//! megabytes into a Postgres JSON column. So **only `Shot` and the resulting
//! `RackState` are persisted**, and the `Timeline` is re-derived by
//! re-simulating whenever someone needs to watch it. That is safe because the
//! simulation is deterministic and runs entirely server-side — there is no
//! client that could disagree about the result.

use serde::{Deserialize, Serialize};

use crate::app::games::pool_core::ball::Ball;

/// One player's move: everything needed to reproduce the shot exactly.
///
/// Angles are radians in table coordinates and offsets are in ball radii, so
/// the struct is independent of the table it is played on — replaying a shot
/// against a different `TableSpec` is meaningful rather than nonsense.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    /// Ball-in-hand placement applied before the strike. Folding this into
    /// the shot rather than making it its own move keeps a foul from costing
    /// the fouled player an extra turn of the correspondence clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<[f64; 2]>,
    pub azimuth: f64,
    /// `[across, up]` on the cue ball's face, in ball radii.
    pub tip: [f64; 2],
    pub speed: f64,
    /// Which pocket the 8 was called into. Only 8-ball uses it, and only for
    /// the 8 — calling every shot is input cost a terminal cannot afford.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub called_pocket: Option<u8>,
    /// Snooker: not a shot at all, but the fouled player exercising their
    /// right to make the offender play it again. Carried on `Shot` rather than
    /// as a second move channel because it *is* the move — it consumes the
    /// turn, records in the history, and resets the clock like any other.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub play_again: bool,
}

impl Default for Shot {
    /// A shot that does nothing, for callers that fill in only the fields they
    /// care about. Speed zero is refused by `Strike::new`, so this can never
    /// be mistaken for a playable shot that someone forgot to finish.
    fn default() -> Self {
        Self {
            place: None,
            azimuth: 0.0,
            tip: [0.0, 0.0],
            speed: 0.0,
            called_pocket: None,
            play_again: false,
        }
    }
}

/// Decimal places kept when a rack is written to the database. Six is a
/// micron on a two-metre table — below the simulator's own separation slop,
/// so rounding can never turn a resolved contact back into an overlap.
pub const PERSIST_DECIMALS: i32 = 6;

/// Every ball's position at one instant. Persisted as the post-shot rack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RackState {
    pub balls: Vec<Ball>,
}

impl RackState {
    pub fn get(&self, id: u8) -> Option<&Ball> {
        self.balls.iter().find(|b| b.id == id)
    }

    pub fn on_table(&self) -> impl Iterator<Item = &Ball> {
        self.balls.iter().filter(|b| b.potted.is_none())
    }

    /// Round every stored coordinate to `places` decimals and zero the
    /// velocities.
    ///
    /// Called before persisting. It is *also* what a determinism test must
    /// apply to its reference run: rounding on the way to the database is
    /// part of the replay contract, so a test that skips it is not testing
    /// the round trip.
    pub fn rounded(&self, places: i32) -> Self {
        let scale = 10f64.powi(places);
        Self {
            balls: self
                .balls
                .iter()
                .map(|b| Ball {
                    id: b.id,
                    pos: [
                        (b.pos[0] * scale).round() / scale,
                        (b.pos[1] * scale).round() / scale,
                    ],
                    vel: [0.0, 0.0],
                    spin: [0.0; 3],
                    potted: b.potted,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShotEvent {
    BallHitBall { a: u8, b: u8, speed: f64 },
    BallHitCushion { ball: u8, speed: f64 },
    BallPotted { ball: u8, pocket: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedEvent {
    pub t: f64,
    pub event: ShotEvent,
}

/// Where one ball is at one sampled instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallFrame {
    pub id: u8,
    pub pos: [f64; 2],
    pub potted: bool,
}

/// Positions sampled on a fixed grid, plus the contact events.
///
/// Never persisted — always re-derived from `Shot` + the pre-shot rack.
#[derive(Clone, Debug)]
pub struct Timeline {
    pub hz: f64,
    pub duration: f64,
    frames: Vec<Vec<BallFrame>>,
    pub events: Vec<TimedEvent>,
}

impl Timeline {
    pub(super) fn new(hz: f64) -> Self {
        Self {
            hz,
            duration: 0.0,
            frames: Vec::new(),
            events: Vec::new(),
        }
    }

    pub(super) fn push_frame(&mut self, frame: Vec<BallFrame>) {
        self.frames.push(frame);
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Seconds until the last frame in which anything visibly moved.
    ///
    /// The physics runs until the balls are *stopped*, and the last stretch of
    /// that is a ball creeping a few millimetres — invisible on a table drawn
    /// a hundred pixels wide, but real seconds during which the board looks
    /// settled and is not yet the player's again. Playback ends when the
    /// picture stops changing, which is the only part of it anyone can see.
    ///
    /// The physical duration is untouched: this is a property of *watching*
    /// the shot, not of the shot.
    pub fn visible_duration(&self) -> f64 {
        /// Well under half a pixel of table at any terminal size.
        const STILL: f64 = 0.004;
        let Some(last) = self.frames.last() else {
            return 0.0;
        };
        let moved = |frame: &Vec<BallFrame>| {
            frame.iter().zip(last.iter()).any(|(a, b)| {
                a.potted != b.potted
                    || (a.pos[0] - b.pos[0]).abs() > STILL
                    || (a.pos[1] - b.pos[1]).abs() > STILL
            })
        };
        match self.frames.iter().rposition(moved) {
            // One frame past the last that still differs, so the settled
            // position is reached rather than merely approached.
            Some(index) => ((index + 1) as f64 / self.hz).min(self.duration),
            None => 0.0,
        }
    }

    /// Positions at time `t` seconds into the shot, linearly interpolated
    /// between the two nearest samples. Clamped at both ends, so sampling
    /// past the end returns the settled rack.
    pub fn sample(&self, t: f64) -> Vec<BallFrame> {
        if self.frames.is_empty() {
            return Vec::new();
        }
        let last = self.frames.len() - 1;
        if !t.is_finite() || t <= 0.0 {
            return self.frames[0].clone();
        }
        let x = t * self.hz;
        if x >= last as f64 {
            return self.frames[last].clone();
        }
        let i = x.floor() as usize;
        let f = x - i as f64;
        let (a, b) = (&self.frames[i], &self.frames[i + 1]);
        a.iter()
            .zip(b.iter())
            .map(|(p, q)| BallFrame {
                id: p.id,
                // A ball reads as potted from the first frame it is gone, so
                // the renderer never draws it sliding into the pocket twice.
                potted: p.potted || q.potted,
                pos: [
                    p.pos[0] + (q.pos[0] - p.pos[0]) * f,
                    p.pos[1] + (q.pos[1] - p.pos[1]) * f,
                ],
            })
            .collect()
    }
}

/// One ball going down one pocket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pot {
    pub ball: u8,
    pub pocket: u8,
}

/// What the rules layer sees. Deliberately free of physics types: a ruleset
/// is testable with plain struct literals and never needs a table.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShotOutcome {
    /// First object ball the cue ball touched, if any.
    pub first_contact: Option<u8>,
    /// Balls potted, in the order they dropped. Includes the cue ball.
    ///
    /// Each carries the pocket it went down, because eight-ball's called shot
    /// needs to know not just that the eight dropped but *where*.
    pub potted: Vec<Pot>,
    /// Whether any ball reached a cushion *after* the cue ball's first
    /// contact — the "no rail" foul turns on exactly this.
    pub cushion_after_contact: bool,
    /// Distinct balls that touched a cushion at any point, in the order they
    /// first did. Break legality is counted off this ("four balls to a rail").
    pub balls_to_rail: Vec<u8>,
    /// The cue ball went down. A scratch under every ruleset we care about.
    pub cue_potted: bool,
    /// Seconds of simulated play.
    pub duration: f64,
    /// The simulator hit its step or time cap. Should never happen in play;
    /// surfaced so a test can assert it never does.
    pub truncated: bool,
}

impl ShotOutcome {
    /// Every ball that dropped, cue ball included, in the order they did.
    pub fn potted_ids(&self) -> impl Iterator<Item = u8> + '_ {
        self.potted.iter().map(|p| p.ball)
    }

    pub fn potted_object_balls(&self) -> impl Iterator<Item = u8> + '_ {
        self.potted_ids().filter(|id| *id != super::ball::CUE)
    }

    pub fn was_potted(&self, id: u8) -> bool {
        self.potted.iter().any(|p| p.ball == id)
    }

    /// Which pocket `id` went down, if it did.
    pub fn pocket_of(&self, id: u8) -> Option<u8> {
        self.potted.iter().find(|p| p.ball == id).map(|p| p.pocket)
    }
}
