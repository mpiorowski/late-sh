//! The cue strike: what the player's aim, tip placement and stroke speed do
//! to the cue ball.
//!
//! The cue is always level — no elevation, so no masse and no jump shots.
//! That keeps every ball on the cloth, which the rest of the kernel assumes.
//!
//! ## The algebra
//!
//! With shot direction `d̂`, left-hand tangent `t̂ = ẑ × d̂`, and a tip offset
//! `(a, b)` in ball radii (`a` across the face, `b` up it), the tip meets the
//! back of the ball at
//!
//! ```text
//! c = R(−w·d̂ + a·t̂ + b·ẑ),   w = √(1 − a² − b²)
//! ```
//!
//! A level cue's impulse is horizontal, `J = m·V·d̂`, so `v = V·d̂` and
//! `ω = (c × J)/I` works out to
//!
//! ```text
//! ω = (5V / 2R)·(−b·d̂y, b·d̂x, −a)
//! ```
//!
//! Two checks fall out of that, and they are the cheapest way to know the
//! signs are right:
//!
//! - **Natural roll at `b = 0.4`.** Rolling needs `ωx = −vy/R`, i.e.
//!   `5b/2R = 1/R`, i.e. `b = 2/5`. Strike two fifths of a radius above
//!   centre and the ball rolls from the moment it leaves the tip.
//! - **Tip left gives clockwise english.** `a > 0` puts the tip left of
//!   centre and yields `ωz < 0`, which is clockwise seen from above.
//!
//! Anything past half a radius from centre miscues in real life, so
//! `Strike::new` refuses it rather than modelling a bad hit.

use crate::app::games::pool_core::{ball::Ball, table::TableSpec};

/// Tip offsets beyond this fraction of the radius miscue. Half a radius is
/// the usual quoted limit for a chalked tip.
pub const MISCUE_LIMIT: f64 = 0.5;

/// Tip offset that produces immediate natural roll — see the module docs.
pub const NATURAL_ROLL_TIP: f64 = 0.4;

/// Fastest stroke we accept. A hard break is around 8 m/s; the cap exists so
/// a malformed shot cannot hand the simulator an absurd amount of energy.
pub const MAX_SPEED: f64 = 12.0;

/// How hard a full pull of the cue hits.
///
/// Three bands rather than one continuous scale because a terminal pointer has
/// perhaps thirty rows of travel to spend, and spreading the whole 0-12 m/s
/// range over them makes a delicate safety and a hard break the same gesture a
/// few pixels apart. Picking the band first means the whole of the pointer's
/// travel is spent inside the range the player actually wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerBand {
    Light,
    Normal,
    Strong,
}

impl PowerBand {
    pub const ALL: [Self; 3] = [Self::Light, Self::Normal, Self::Strong];

    /// Fraction of `MAX_SPEED` a full pull reaches in this band. The bands
    /// overlap on purpose: every speed is reachable from more than one, so
    /// there is no gap a player can fall into between them.
    ///
    /// The numbers are pinned to how hard people actually hit, not spread
    /// evenly over the range. A pot at conversational pace is 2-3 m/s, a
    /// firm shot down the table 4-5, and only a break approaches the cap — so
    /// `normal` tops out around 4 m/s and reaches its middle, where an
    /// untouched cue sits, at a natural rolling shot. The first cut of these
    /// put `normal` at two thirds of a break, which made every shot feel like
    /// a slam; the second was still a fifth too hard to play position with.
    pub fn ceiling(self) -> f64 {
        match self {
            Self::Light => 0.12,
            Self::Normal => 0.29,
            Self::Strong => 1.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Normal => "normal",
            Self::Strong => "strong",
        }
    }

    /// The key that arms this band. Laid out low-to-high on the keyboard so
    /// the row reads as a strength dial: `x` under `s` under `w`.
    pub fn key(self) -> char {
        match self {
            Self::Light => 'x',
            Self::Normal => 's',
            Self::Strong => 'w',
        }
    }

    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|band| band.key() == key)
    }
}

/// What the pointer is currently doing to the shot.
///
/// Not a sequence. Every part of a shot stays adjustable at every moment and
/// the player may strike whenever they like; a mode only says which of them
/// the mouse is wired to right now. That matters because a terminal cannot
/// report a key being *held* — there is no key-up event without the Kitty
/// keyboard protocol — so a key arms a mode and the same key, Esc, or arming
/// another one drops it.
///
/// Kept here rather than in the board screen because it is part of the shot
/// *model*, not of one surface's input handling: a live pool table reusing
/// this kernel would offer the same four wirings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotMode {
    /// Nothing armed. Clicks pick a target and the brackets cycle one.
    Idle,
    /// Pointer motion walks the aim across the target ball.
    Aim,
    /// Pointer motion walks the tip across the cue ball's face.
    Spin,
    /// Ball in hand: the pointer carries the cue ball, a click sets it down.
    /// Only reachable when a foul has actually granted one.
    Place,
    /// Armed to strike: the pointer draws the cue back and pushing it forward
    /// through the ball is the stroke.
    Stroke(PowerBand),
}

impl ShotMode {
    /// Every mode, with `Stroke` shown at its middle band.
    pub const ALL: [Self; 5] = [
        Self::Idle,
        Self::Aim,
        Self::Spin,
        Self::Place,
        Self::Stroke(PowerBand::Normal),
    ];

    /// How the status line names the armed mode. Spelled out rather than
    /// deferring to `PowerBand::label`, because a bare "normal" on the status
    /// row says nothing about what it is normal *for*.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "ready",
            Self::Aim => "aiming",
            Self::Spin => "spin",
            Self::Place => "ball in hand",
            Self::Stroke(PowerBand::Light) => "light stroke",
            Self::Stroke(PowerBand::Normal) => "normal stroke",
            Self::Stroke(PowerBand::Strong) => "strong stroke",
        }
    }

    pub fn band(self) -> Option<PowerBand> {
        match self {
            Self::Stroke(band) => Some(band),
            _ => None,
        }
    }

    /// What the player can do right now. The board screen shows this verbatim,
    /// so nobody has to already know the controls.
    pub fn hint(self) -> &'static str {
        match self {
            // The panel is divided the way the shot is: the target ball and
            // the sighting line up top arm the aim, the cue ball's face arms
            // spin, and the cue itself arms the stroke. Saying so here means a
            // player never has to learn the key map to start.
            Self::Idle => {
                "[ ] target · click a ball to aim, the cue ball for spin, the cue to stroke · a e x/s/w · c clears"
            }
            Self::Aim => {
                "aiming: mouse left/right for the side, up/down for how far off · h/l H/L by key · right-click straightens"
            }
            Self::Spin => {
                "spin: mouse or arrows · click the face to set it · right-click centres it · esc undoes"
            }
            Self::Place => {
                "ball in hand: the cue ball follows the pointer · click sets it down · right-click resets"
            }
            Self::Stroke(_) => {
                "draw down, then push back up through the ball to strike · esc or right-click to stop"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeError {
    /// Tip offset past `MISCUE_LIMIT` from centre.
    Miscue,
    /// Speed not finite, not positive, or past `MAX_SPEED`.
    BadSpeed,
    /// Azimuth not finite.
    BadAim,
}

/// A validated strike. Construct through `new` — the invariants are what let
/// the simulator assume the shot is playable.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub azimuth: f64,
    /// Across the face, in ball radii. Positive is left of the shot line.
    pub tip_side: f64,
    /// Up the face, in ball radii. Positive is follow, negative is draw.
    pub tip_vert: f64,
    pub speed: f64,
}

impl Strike {
    pub fn new(
        azimuth: f64,
        tip_side: f64,
        tip_vert: f64,
        speed: f64,
    ) -> Result<Self, StrikeError> {
        if !azimuth.is_finite() {
            return Err(StrikeError::BadAim);
        }
        if !speed.is_finite() || speed <= 0.0 || speed > MAX_SPEED {
            return Err(StrikeError::BadSpeed);
        }
        if !tip_side.is_finite() || !tip_vert.is_finite() {
            return Err(StrikeError::Miscue);
        }
        if (tip_side * tip_side + tip_vert * tip_vert).sqrt() > MISCUE_LIMIT {
            return Err(StrikeError::Miscue);
        }
        Ok(Self {
            azimuth,
            tip_side,
            tip_vert,
            speed,
        })
    }

    /// Apply the strike to a resting cue ball.
    pub fn apply(&self, cue: &mut Ball, spec: &TableSpec) {
        let (sin_a, cos_a) = self.azimuth.sin_cos();
        let d = [cos_a, sin_a];
        let k = 2.5 * self.speed / spec.ball_radius;

        cue.vel = [self.speed * d[0], self.speed * d[1]];
        cue.spin = [
            -k * self.tip_vert * d[1],
            k * self.tip_vert * d[0],
            -k * self.tip_side,
        ];
    }
}
