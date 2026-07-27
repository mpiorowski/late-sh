//! Pacing: how much game time a player is allowed to accrue, and how fast the
//! village half of the game runs. This layer is **ours**, not upstream's, so
//! it is the one module in this door that carries no MPL obligation.
//!
//! Upstream A Dark Room has no offline progress at all: every timer is wall
//! clock while the browser tab is open, so a player finishes the arc in a
//! couple of sittings. That shape fights an SSH clubhouse, where the pitch is
//! "log in, check on the thing, log out". Three rules reshape it:
//!
//! 1. **Credit accrues while the SSH session is connected**, not while the
//!    door screen is focused. The village grows while you are in the lounge;
//!    it does not grow while you are logged out. Time between sessions is
//!    worth nothing, which is what keeps this a game you visit.
//! 2. **The village half runs at [`SLOWDOWN`]×.** Only worker income and new
//!    arrivals are slowed. Every cooldown, the fire, and the room temperature
//!    stay at upstream speed, because the opening act is a click loop, not an
//!    idle loop: stretching it produces dead air, not a longer game.
//! 3. **At most [`DAILY_CREDIT_SECS`] of village time lands per UTC day.**
//!    Without this, "the session must be connected" is not pacing, it just
//!    rewards whoever parks a terminal on a spare monitor. With it, the arc
//!    spans weeks whatever anyone does with their idle sessions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How much slower the village runs than upstream. Applied to worker income
/// and population growth only; see the module note for why the room is exempt.
pub const SLOWDOWN: u32 = 5;

/// Village time credited per UTC day, in seconds (3 hours).
pub const DAILY_CREDIT_SECS: u32 = 3 * 60 * 60;

/// Seconds in a day, for the UTC day number the cap resets on.
const SECS_PER_DAY: i64 = 24 * 60 * 60;

/// The daily credit counter, persisted with the save.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pace {
    /// UTC day number `spent` belongs to. A different day zeroes the counter.
    #[serde(default)]
    day: i64,
    /// Village seconds already credited on `day`.
    #[serde(default)]
    spent: u32,
}

impl Pace {
    /// Grant up to `elapsed` seconds of village time, capped by what is left
    /// in today's allowance. Returns what the sim may actually advance.
    ///
    /// A session that spans midnight UTC rolls onto the new day's full
    /// allowance rather than trying to split the elapsed span across two
    /// counters: the player was present for all of it, and erring generous at
    /// a boundary is cheaper to reason about than a proportional split.
    pub fn grant(&mut self, now: DateTime<Utc>, elapsed: u32) -> u32 {
        let today = day_number(now);
        if self.day != today {
            self.day = today;
            self.spent = 0;
        }
        let granted = elapsed.min(DAILY_CREDIT_SECS - self.spent);
        self.spent += granted;
        granted
    }

    /// Village seconds left in today's allowance, for the status line.
    pub fn remaining(&self, now: DateTime<Utc>) -> u32 {
        if self.day != day_number(now) {
            DAILY_CREDIT_SECS
        } else {
            DAILY_CREDIT_SECS - self.spent
        }
    }

    /// Whether today's allowance is used up, so the UI can say so plainly
    /// rather than letting the village look silently broken.
    pub fn exhausted(&self, now: DateTime<Utc>) -> bool {
        self.remaining(now) == 0
    }
}

/// Slow a delay down by [`SLOWDOWN`]. Used for the village clocks only.
pub fn slowed(delay_secs: u32) -> u32 {
    delay_secs * SLOWDOWN
}

fn day_number(now: DateTime<Utc>) -> i64 {
    now.timestamp().div_euclid(SECS_PER_DAY)
}
