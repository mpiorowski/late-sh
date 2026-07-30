/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The ascent below
 * is transcribed from `script/space.js` and `script/ship.js`. See LICENSING.md
 * and NOTICE. */

//! The way off this rock.
//!
//! Upstream flies the ship in a 700px box on a 33ms timer. This is the same
//! sixty seconds in a character grid, stepped from the app's hot tick. Two
//! deliberate changes, both forced by the terminal:
//!
//! - **Arrows are impulses, not held keys.** SSH delivers key repeats and no
//!   key-up at all, so each press nudges the ship by a fixed step rather than
//!   setting a velocity. Better thrusters make the step bigger, which is what
//!   the upgrade meant in the original.
//! - **Asteroids fall about three times slower** in wall-clock terms, because
//!   a character cell is far coarser than a pixel and upstream's half-second
//!   crossings are unreadable at fifteen frames a second.

use rand::Rng;

/// The grid the flight is drawn in.
pub const WIDTH: f64 = 60.0;
pub const HEIGHT: f64 = 20.0;

/// Alien alloy per point of hull, and per engine upgrade.
pub const ALLOY_PER_HULL: i64 = 1;
pub const ALLOY_PER_THRUSTER: i64 = 1;
/// Seconds between liftoffs.
pub const LIFTOFF_COOLDOWN: u32 = 120;
pub const BASE_THRUSTERS: i64 = 1;

/// How far one nudge moves the ship, before thrusters.
const BASE_STEP: f64 = 1.4;
const STEP_PER_THRUSTER: f64 = 0.35;

/// Seconds an asteroid takes to cross the screen, fastest and slowest.
const FALL_FAST: f64 = 1.5;
const FALL_SLOW: f64 = 4.5;

/// Altitude in km at which the flight is won.
pub const WIN_ALTITUDE: i64 = 60;

/// One rock on its way past.
#[derive(Clone, Copy, Debug)]
pub struct Asteroid {
    pub x: f64,
    pub y: f64,
    /// Cells per second.
    pub speed: f64,
    pub glyph: char,
}

/// How the flight ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flight {
    Crashed,
    Won,
}

/// A flight in progress. Never persisted: a dropped session is a crash, the
/// same as closing the tab was.
#[derive(Clone, Debug)]
pub struct Space {
    pub ship_x: f64,
    pub ship_y: f64,
    pub hull: i64,
    pub max_hull: i64,
    pub thrusters: i64,
    /// Kilometres up.
    pub altitude: i64,
    pub asteroids: Vec<Asteroid>,
    spawn_timer: f64,
    altitude_timer: f64,
    pub outcome: Option<Flight>,
}

impl Space {
    pub fn new(hull: i64, thrusters: i64) -> Self {
        Self {
            ship_x: WIDTH / 2.0,
            ship_y: HEIGHT - 3.0,
            hull,
            max_hull: hull,
            thrusters,
            altitude: 0,
            asteroids: Vec::new(),
            spawn_timer: 0.0,
            altitude_timer: 1.0,
            outcome: None,
        }
    }

    /// What the sky is called up here.
    pub fn layer(&self) -> &'static str {
        match self.altitude {
            a if a < 10 => "Troposphere",
            a if a < 20 => "Stratosphere",
            a if a < 30 => "Mesosphere",
            a if a < 45 => "Thermosphere",
            a if a < 60 => "Exosphere",
            _ => "Space",
        }
    }

    /// Nudge the ship. Upstream reads held keys every 33ms; over SSH there is
    /// no key-up, so a press is a step.
    pub fn nudge(&mut self, dx: f64, dy: f64) {
        let step = BASE_STEP + STEP_PER_THRUSTER * self.thrusters as f64;
        self.ship_x = (self.ship_x + dx * step).clamp(0.0, WIDTH - 1.0);
        self.ship_y = (self.ship_y + dy * step).clamp(0.0, HEIGHT - 1.0);
    }

    /// One step of the flight.
    pub fn tick(&mut self, seconds: f64, rng: &mut impl Rng) {
        if self.outcome.is_some() {
            return;
        }

        self.altitude_timer -= seconds;
        if self.altitude_timer <= 0.0 {
            self.altitude_timer += 1.0;
            self.altitude += 1;
            if self.altitude > WIN_ALTITUDE {
                self.outcome = Some(Flight::Won);
                return;
            }
        }

        for asteroid in &mut self.asteroids {
            asteroid.y += asteroid.speed * seconds;
        }
        self.asteroids.retain(|asteroid| asteroid.y < HEIGHT);

        // Collisions: an asteroid sharing the ship's cell takes a point of
        // hull with it.
        let (ship_x, ship_y) = (self.ship_x.round(), self.ship_y.round());
        let before = self.asteroids.len();
        self.asteroids
            .retain(|a| !(a.x.round() == ship_x && a.y.round() == ship_y));
        let hits = (before - self.asteroids.len()) as i64;
        if hits > 0 {
            self.hull = (self.hull - hits).max(0);
            if self.hull == 0 {
                self.outcome = Some(Flight::Crashed);
                return;
            }
        }

        // Upstream's spawn cadence: one every `1000 - altitude * 10` ms, with
        // extra rocks thrown in the higher you get.
        self.spawn_timer -= seconds;
        if self.spawn_timer <= 0.0 {
            self.spawn_timer = (1000.0 - self.altitude as f64 * 10.0).max(100.0) / 1000.0;
            let mut count = 1;
            if self.altitude > 10 {
                count += 1;
            }
            if self.altitude > 20 {
                count += 2;
            }
            if self.altitude > 40 {
                count += 2;
            }
            for _ in 0..count {
                self.spawn(rng);
            }
        }
    }

    fn spawn(&mut self, rng: &mut impl Rng) {
        let roll: f64 = rng.r#gen();
        let glyph = match roll {
            r if r < 0.2 => '#',
            r if r < 0.4 => '$',
            r if r < 0.6 => '%',
            r if r < 0.8 => '&',
            _ => 'H',
        };
        let fall = FALL_FAST + rng.r#gen::<f64>() * (FALL_SLOW - FALL_FAST);
        self.asteroids.push(Asteroid {
            x: (rng.r#gen::<f64>() * WIDTH).floor(),
            y: 0.0,
            speed: HEIGHT / fall,
            glyph,
        });
    }

    /// Give up mid-flight. Upstream has no such button; over SSH, leaving has
    /// to mean something, and a scuttled flight is a crash.
    pub fn abort(&mut self) {
        if self.outcome.is_none() {
            self.outcome = Some(Flight::Crashed);
        }
    }
}

/// The line the ship prints when it first stands ready.
pub const MSG_SEEN_SHIP: &str =
    "somewhere above the debris cloud, the wanderer fleet hovers. been on this rock too long.";

/// The warning before the one-way trip.
pub const MSG_READY_TO_LEAVE: &str = "time to get out of this place. won't be coming back.";

/// What a crash costs: nothing but the wait.
pub const MSG_CRASH: &str = "the ship is torn apart. the wanderer wakes in the dust, again.";

/// Upstream ends on a wordless fade and a score. There is no score here, so
/// the ending says the only thing left to say.
pub const ENDING: [&str; 3] = [
    "the ship rises through the debris cloud, and the dust of this world falls away.",
    "the fire in the room burns down to nothing. nobody is left to stoke it.",
    "the wanderer is gone.",
];
