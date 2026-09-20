//! A ball's state and how it moves between impacts.
//!
//! This is the load-bearing file: everything else in the kernel assumes the
//! evolution here is exact. It follows the standard event-based billiard
//! model (Han 2005, the same one `pooltool` implements), which pairs a *2D*
//! velocity with a *3D* angular velocity. That pairing is the whole point —
//! draw and follow are spin about a horizontal axis, so a physics engine that
//! only carries a scalar 2D rotation (as `rapier2d` does) cannot express them
//! at all.
//!
//! ## Motion states
//!
//! The contact point is the bottom of the ball, so the velocity of the ball's
//! material at the cloth is
//!
//! ```text
//! u = v + R (ẑ × ω) = (vx - R·ωy, vy + R·ωx)
//! ```
//!
//! and everything follows from whether that slip is zero:
//!
//! | state | condition | what decays |
//! |---|---|---|
//! | `Sliding` | `\|u\| > 0` | slip, linearly at `7/2 μ_s g` |
//! | `Rolling` | `u = 0`, `\|v\| > 0` | speed, at `μ_r g` |
//! | `Spinning` | `v = 0`, `ωz ≠ 0` | `ωz`, at `5 μ_sp g / 2R` |
//! | `Stationary` | everything zero | nothing |
//!
//! Within a state the motion is closed-form, so `advance` never integrates
//! and never accumulates error. The `7/2` in the sliding rate is the linear
//! `1` plus the angular `5/2` contributed by the friction torque; the useful
//! consequence is that the slip *direction* is constant while sliding, which
//! is what makes the closed form possible.

use serde::{Deserialize, Serialize};

use crate::app::games::pool_core::table::TableSpec;

/// Below these, a quantity is treated as exactly zero.
///
/// These are *physical* thresholds, not floating-point ones. A tenth of a
/// millimetre per second is not a moving ball on any table anyone has played
/// on, and picking round-off-scale values instead (`1e-9`) is a real trap:
/// friction is asymptotic, so a ball never formally stops, and a cluster of
/// balls creeping at `1e-12` m/s keeps generating contacts forever while the
/// shot clock never advances. Ask for less resolution than the game has and
/// the simulation terminates on its own.
pub const EPS_SLIP: f64 = 1e-5;
pub const EPS_VEL: f64 = 1e-4;
pub const EPS_SPIN: f64 = 1e-2;

/// The cue ball's id. Object balls are 1..=15 and carry their printed number.
pub const CUE: u8 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Stationary,
    Spinning,
    Sliding,
    Rolling,
}

/// A ball is only ever persisted at rest (see `RackState::rounded`), so the
/// three fields that are zero at rest are skipped on the way out. That is
/// worth doing: a rack is sixteen of these and two racks live in every stored
/// match, and `0.0` repeated eighty times is most of the JSON.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ball {
    pub id: u8,
    pub pos: [f64; 2],
    #[serde(default, skip_serializing_if = "is_still")]
    pub vel: [f64; 2],
    /// Angular velocity about (x, y, z). `z` is english; the horizontal
    /// components are draw and follow.
    #[serde(default, skip_serializing_if = "is_unspun")]
    pub spin: [f64; 3],
    /// `Some(pocket index)` once potted. A potted ball is parked at the
    /// pocket's mouth centre and takes no further part in the shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potted: Option<u8>,
}

fn is_still(vel: &[f64; 2]) -> bool {
    vel == &[0.0, 0.0]
}

fn is_unspun(spin: &[f64; 3]) -> bool {
    spin == &[0.0; 3]
}

impl Ball {
    pub fn resting(id: u8, pos: [f64; 2]) -> Self {
        Self {
            id,
            pos,
            vel: [0.0, 0.0],
            spin: [0.0; 3],
            potted: None,
        }
    }

    /// Velocity of the ball's material at the cloth contact point.
    pub fn slip(&self, radius: f64) -> [f64; 2] {
        [
            self.vel[0] - radius * self.spin[1],
            self.vel[1] + radius * self.spin[0],
        ]
    }

    pub fn speed(&self) -> f64 {
        (self.vel[0] * self.vel[0] + self.vel[1] * self.vel[1]).sqrt()
    }

    pub fn is_moving(&self, radius: f64) -> bool {
        self.potted.is_none() && self.motion(radius) != Motion::Stationary
    }

    pub fn motion(&self, radius: f64) -> Motion {
        if self.potted.is_some() {
            return Motion::Stationary;
        }
        let u = self.slip(radius);
        if (u[0] * u[0] + u[1] * u[1]).sqrt() > EPS_SLIP {
            Motion::Sliding
        } else if self.speed() > EPS_VEL {
            Motion::Rolling
        } else if self.spin[2].abs() > EPS_SPIN {
            Motion::Spinning
        } else {
            Motion::Stationary
        }
    }

    /// How long the current motion state lasts. `f64::INFINITY` when the ball
    /// is already at rest.
    ///
    /// Capping a step at this time is what keeps the closed form honest: a
    /// step that ran past the slide-to-roll transition would keep applying
    /// sliding friction to a ball that had already settled into a roll.
    pub fn time_to_transition(&self, spec: &TableSpec) -> f64 {
        if self.potted.is_some() {
            return f64::INFINITY;
        }
        let spin_end = if self.spin[2].abs() > EPS_SPIN {
            self.spin[2].abs() / spec.spin_decel()
        } else {
            f64::INFINITY
        };
        let motion_end = match self.motion(spec.ball_radius) {
            Motion::Stationary => f64::INFINITY,
            Motion::Spinning => f64::INFINITY,
            Motion::Sliding => {
                let u = self.slip(spec.ball_radius);
                (u[0] * u[0] + u[1] * u[1]).sqrt() / spec.slip_decel()
            }
            Motion::Rolling => self.speed() / spec.roll_decel(),
        };
        spin_end.min(motion_end)
    }

    /// Advance exactly `dt` under the current motion state.
    ///
    /// The caller must not pass a `dt` beyond `time_to_transition`; `sim`
    /// enforces that. Passing a longer one is not unsafe, it is just wrong —
    /// the ball would keep decelerating past zero.
    pub fn advance(&mut self, spec: &TableSpec, dt: f64) {
        if self.potted.is_some() || dt <= 0.0 {
            return;
        }
        let r = spec.ball_radius;
        match self.motion(r) {
            Motion::Stationary => {}
            Motion::Spinning => self.decay_spin(spec, dt),
            Motion::Sliding => {
                let u = self.slip(r);
                let mag = (u[0] * u[0] + u[1] * u[1]).sqrt();
                let dir = [u[0] / mag, u[1] / mag];
                let a = spec.mu_slide * spec.gravity;

                self.pos[0] += self.vel[0] * dt - 0.5 * a * dt * dt * dir[0];
                self.pos[1] += self.vel[1] * dt - 0.5 * a * dt * dt * dir[1];
                self.vel[0] -= a * dt * dir[0];
                self.vel[1] -= a * dt * dir[1];

                // Friction acts at the contact point, so it also spins the
                // ball up about the horizontal axis: torque ∝ ẑ × û.
                let alpha = 2.5 * a / r;
                self.spin[0] += alpha * dt * -dir[1];
                self.spin[1] += alpha * dt * dir[0];
                self.decay_spin(spec, dt);

                // Land exactly on the roll if this step consumed the slide.
                if self.slip_magnitude(r) <= EPS_SLIP {
                    self.snap_to_roll(r);
                }
            }
            Motion::Rolling => {
                let v = self.speed();
                let dir = [self.vel[0] / v, self.vel[1] / v];
                let a = spec.roll_decel();

                self.pos[0] += self.vel[0] * dt - 0.5 * a * dt * dt * dir[0];
                self.pos[1] += self.vel[1] * dt - 0.5 * a * dt * dt * dir[1];
                self.vel[0] -= a * dt * dir[0];
                self.vel[1] -= a * dt * dir[1];
                if self.vel[0] * dir[0] + self.vel[1] * dir[1] <= 0.0 {
                    self.vel = [0.0, 0.0];
                }
                self.decay_spin(spec, dt);
                self.snap_to_roll(r);
            }
        }
    }

    fn slip_magnitude(&self, radius: f64) -> f64 {
        let u = self.slip(radius);
        (u[0] * u[0] + u[1] * u[1]).sqrt()
    }

    /// Force the rolling constraint. A rolling ball's horizontal spin is not
    /// free: it is whatever makes the contact point stand still.
    fn snap_to_roll(&mut self, radius: f64) {
        self.spin[0] = -self.vel[1] / radius;
        self.spin[1] = self.vel[0] / radius;
    }

    fn decay_spin(&mut self, spec: &TableSpec, dt: f64) {
        let drop = spec.spin_decel() * dt;
        if self.spin[2].abs() <= drop {
            self.spin[2] = 0.0;
        } else {
            self.spin[2] -= drop * self.spin[2].signum();
        }
    }
}
