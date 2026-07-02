//! Track data model for the Traffic game.
//!
//! A **track** is a named, multi-stage course. Each [`Stage`] defines the road
//! geometry, traffic, scenery, and theme that apply while the player is in that
//! stage. Stages succeed each other when the player covers their `distance`
//! budget; the next stage can have completely different lane counts, themes,
//! sceneries, and dividers.
//!
//! Tracks are hard-coded as `&'static` Rust data (no file I/O), so authoring a
//! new one is a code change. See `tracks/sample.rs` for an end-to-end example
//! and `tracks/presets.rs` for reusable building blocks.
//!
//! # Extensibility
//!
//! Aspects (lane surfaces, dividers, scenery, shoulders, obstacles, objects) are
//! **open** — each is a small struct holding fn-pointer(s) for rendering.  The
//! standard library lives in `theme.rs` as `pub const` instances (e.g.
//! `theme::LANE_ASPHALT_PREMIUM`).  A new track can define its own instances
//! inline; no changes to `track.rs` or `theme.rs` are needed.
//!
//! [`Theme`] remains a closed enum for now.  Extending it means adding a variant
//! here and a `tint` branch in `theme.rs`.
//!
//! # Coordinate units
//!
//! All distance/speed values are in **displayed** units (the numbers the player
//! sees).  [`Track::distance_scale`] and [`Track::speed_scale`] convert between
//! these and the internal physics units used by `traffic::state`.

#![allow(dead_code)]

use ratatui::style::Color;

// ─── Rendering primitives (used by style descriptors) ───────────────────────

/// One rendered terminal cell: glyph + foreground + background.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub sym: &'static str,
    pub fg: Color,
    pub bg: Color,
}

impl Cell {
    pub const fn new(sym: &'static str, fg: Color, bg: Color) -> Self {
        Self { sym, fg, bg }
    }
}

/// A multi-row scenery sprite anchored at its bottom-left cell; extends
/// `glyphs.len() - 1` rows upward.  Each row is a slice of single-cell strings,
/// one per column (`len == width`).
#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    pub width: u8,
    /// Rows ordered top-to-bottom; `glyphs[0]` is the topmost row.
    pub glyphs: &'static [&'static [&'static str]],
    pub fg: Color,
}

// ─── Style descriptor types ─────────────────────────────────────────────────
//
// Each "aspect" is now a small struct with fn-pointer field(s) rather than an
// enum variant.  This lets track authors define custom looks inline without
// touching any shared file.  All standard instances live in `theme.rs`.

/// Rendering descriptor for a lane surface.
#[derive(Clone, Copy, Debug)]
pub struct LaneStyle {
    pub cell: fn(theme: Theme, row: i32, col: u16) -> Cell,
    pub bg: fn(theme: Theme) -> Color,
}

/// Rendering descriptor for a lane divider column.  Theme-independent.
#[derive(Clone, Copy, Debug)]
pub struct DividerStyle {
    pub cell: fn(row: i32, bg: Color) -> Cell,
}

/// Rendering descriptor for a scenery background fill.
#[derive(Clone, Copy, Debug)]
pub struct SceneryStyle {
    pub bg: fn(theme: Theme) -> Color,
}

/// Rendering and identity descriptor for a stationary obstacle.
///
/// `id` **must** be unique among all `ObstacleStyle` instances that can appear
/// in the same track — it seeds the deterministic placement hash.  Standard
/// library IDs are 1–6 (`theme::OBSTACLE_*`).  Custom obstacles should start
/// from 100 or higher to avoid collisions.
#[derive(Clone, Copy, Debug)]
pub struct ObstacleStyle {
    /// Stable integer used in the placement hash.  Never change once deployed.
    pub id: i64,
    /// Short label shown in the right-panel effects log on crossing.
    pub label: &'static str,
    /// Per-column glyphs (3 columns = car body width) + foreground colour.
    /// Theme-independent.
    pub glyphs: ([&'static str; 3], Color),
}

/// Rendering descriptor for a scenery object (tree, building, etc.).
#[derive(Clone, Copy, Debug)]
pub struct ObjectStyle {
    pub sprite: fn(theme: Theme) -> Sprite,
    /// When true, the bottom row of the sprite renders with `theme::trunk_color`
    /// instead of `sprite.fg` to visually separate trunk from canopy.
    pub has_trunk: bool,
}

/// Rendering descriptor for one shoulder column.
#[derive(Clone, Copy, Debug)]
pub struct ShoulderStyle {
    /// `repeat` is forwarded from [`Shoulder::repeat`]; the fn is responsible
    /// for honouring it (blank on off-rows) and for the empty/transparent case.
    pub cell: fn(theme: Theme, row: i32, repeat: u8, fallback_bg: Color) -> Cell,
}

// ─── Track / Stage ──────────────────────────────────────────────────────────

/// A complete drivable course composed of one or more [`Stage`]s.
#[derive(Debug, Clone, Copy)]
pub struct Track {
    /// Title shown in the picker and above the road.
    pub name: &'static str,
    pub author: &'static str,
    /// Long-form description shown in the picker only.
    pub description: &'static str,
    /// Ordered stages.  Must contain at least one entry.
    pub stages: &'static [Stage],
    /// Multiplies displayed distance vs. physics distance.
    pub distance_scale: f32,
    /// Multiplies the displayed speedometer reading vs. physics speed.
    pub speed_scale: f32,
    /// Lives before the run ends.
    pub lives: u8,
}

/// A contiguous segment of a [`Track`] with its own road, theme, and scenery.
#[derive(Debug, Clone, Copy)]
pub struct Stage {
    pub name: &'static str,
    /// Short description shown in the bottom-right of the HUD during play.
    pub description: &'static str,
    /// Environmental icon glyph shown beside the track name (e.g. `"🏙"`).
    /// Use any single-width emoji or ASCII character.
    pub icon: &'static str,
    pub theme: Theme,
    /// Displayed kilometres the player must travel before the next stage.
    pub distance_km: f32,
    pub road: Road,
}

// ─── Theme ──────────────────────────────────────────────────────────────────

/// Visual theme applied across all themed rendering calls for a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Standard,
    Winter,
    Desert,
}

impl Theme {
    pub fn name(self) -> &'static str {
        match self {
            Theme::Standard => "Normal",
            Theme::Winter => "Winter",
            Theme::Desert => "Desert",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Theme::Standard => "☀",
            Theme::Winter => "❄",
            Theme::Desert => "🌵",
        }
    }
}

// ─── Road ───────────────────────────────────────────────────────────────────

/// Geometry, traffic, and scenery for a single stage.
#[derive(Debug, Clone, Copy)]
pub struct Road {
    pub aspect: RoadAspect,
    pub lanes: Lanes,
    pub sceneries: Sceneries,
    pub shoulders: Shoulders,
}

/// Road-wide aspects that don't apply per-lane.
#[derive(Debug, Clone, Copy)]
pub struct RoadAspect {
    pub dividers: Divider,
}

/// Divider styling between groups (primary) and within a group (lane).
#[derive(Debug, Clone, Copy)]
pub struct Divider {
    /// Between the two traffic directions (incoming vs. outgoing).
    pub primary: DividerStyle,
    /// Between two lanes in the same direction.
    pub lane: DividerStyle,
}

// ─── Lanes ──────────────────────────────────────────────────────────────────

/// Lane configuration for both traffic directions.
///
/// Both slices are ordered **inward → outward** (index 0 = closest to the
/// center divider, last = outermost/shoulder lane).  A symmetric definition
/// produces a symmetric road: `incoming: &[FAST, SLOW]` mirrors
/// `outgoing: &[FAST, SLOW]` with the fast lanes adjacent to the center.
///
/// **Invariants:** at least one `outgoing` lane; at least 2 lanes total.
#[derive(Debug, Clone, Copy)]
pub struct Lanes {
    pub incoming: &'static [Lane],
    pub outgoing: &'static [Lane],
}

/// One lane: surface, speed bounds, traffic, and obstacles.
#[derive(Debug, Clone, Copy)]
pub struct Lane {
    pub style: LaneStyle,
    /// Minimum displayed km/h the player may drive on this lane.
    pub own_min_speed: f32,
    /// Maximum displayed km/h the player may drive on this lane.
    pub own_max_speed: f32,
    /// Passive deceleration (displayed km/h per second) when not accelerating.
    pub passive_decel: f32,
    pub traffic_min_speed: f32,
    pub traffic_max_speed: f32,
    /// Traffic density `[0.0, 1.0]`.  `0.0` = empty; `1.0` = one car every
    /// `AI_MIN_SEPARATION_M` across the full spawn horizon (maximum packing
    /// without overlap).  Values above `1.0` are clamped internally.
    pub traffic_density: f32,
    pub traffic_cars: &'static [Car],
    pub obstacles: &'static [Obstacle],
}

/// AI car body shape.  Width is always 3 chars; `height` is in rows.
#[derive(Debug, Clone, Copy)]
pub struct Car {
    pub height: u8,
    /// Relative weight at spawn-time shape selection.  Higher = more common.
    pub incidence: f32,
}

// ─── Obstacles ──────────────────────────────────────────────────────────────

/// Stationary hazard sprinkled along a lane.
#[derive(Debug, Clone, Copy)]
pub struct Obstacle {
    pub style: ObstacleStyle,
    /// Average fraction of lane-length occupied by this obstacle type `[0, 1]`.
    pub frequency: f32,
    /// Effects that fire once when the player drives over the obstacle.
    pub effects: &'static [ObstacleEffect],
}

impl Obstacle {
    pub fn has_crash(&self) -> bool {
        self.effects.contains(&ObstacleEffect::Crash)
    }
}

/// One-shot effect triggered when the player crosses an obstacle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObstacleEffect {
    /// Instant game-over.
    Crash,
    /// Multiplicative speed change.  `-1.0` = full stop; `+0.5` = +50%.
    SpeedChange { affect: f32 },
    /// Player can't accelerate for `cooldown_ms` milliseconds.
    BlockGas { cooldown_ms: u16 },
    /// Player can't brake for `cooldown_ms` milliseconds.
    BlockBrakes { cooldown_ms: u16 },
    /// Player can't change lanes for `cooldown_ms` milliseconds.
    BlockWheels { cooldown_ms: u16 },
}

// ─── Scenery ────────────────────────────────────────────────────────────────

/// Left and right scenery slabs flanking the road.
///
/// "Left" / "right" are absolute screen sides.
#[derive(Debug, Clone, Copy)]
pub struct Sceneries {
    pub left: Scenery,
    pub right: Scenery,
}

/// One side's scenery: a band of fixed character width with a background and
/// randomly-placed objects on top.
#[derive(Debug, Clone, Copy)]
pub struct Scenery {
    pub width: u8,
    pub background: SceneryStyle,
    /// Objects to scatter.  Only spawn in cells not covered by a shoulder.
    pub objects: &'static [Object],
}

/// One scattered scenery object.
#[derive(Debug, Clone, Copy)]
pub struct Object {
    pub style: ObjectStyle,
    /// Relative weight when picking which object to render at a given cell.
    pub incidence: f32,
}

// ─── Shoulders ──────────────────────────────────────────────────────────────

/// Shoulder strips on each side of the road, drawn on top of scenery.
#[derive(Debug, Clone, Copy)]
pub struct Shoulders {
    /// Listed from the road edge outward.
    pub left: &'static [Shoulder],
    pub right: &'static [Shoulder],
}

/// One shoulder column.
#[derive(Debug, Clone, Copy)]
pub struct Shoulder {
    pub style: ShoulderStyle,
    /// Repetition period in rows: `0` = continuous; `n > 0` = every `n` rows.
    pub repeat: u8,
}

// ─── Helpers ────────────────────────────────────────────────────────────────

impl Track {
    /// Highest normalized finish score any track can award.
    pub const SCORE_MAX: i64 = 1000;

    pub fn total_distance_km(&self) -> f32 {
        self.stages.iter().map(|s| s.distance_km).sum()
    }

    /// Theoretical `(fastest, slowest)` completion times in seconds, derived
    /// purely from the track definition: per stage, driving at the highest
    /// (fastest) or lowest (slowest) lane speed the whole way. These bounds are
    /// what make every track's finish score comparable regardless of its
    /// distance or speed definition.
    pub fn theoretical_times_s(&self) -> (f32, f32) {
        let mut fastest = 0.0f32;
        let mut slowest = 0.0f32;
        for stage in self.stages {
            let dist_m = stage.distance_km * 1000.0 * self.distance_scale;
            let total = stage.road.lanes.total();
            let mut lane_max = f32::MIN;
            let mut lane_min = f32::MAX;
            for i in 0..total {
                if let Some(lane) = stage.road.lanes.get(i) {
                    lane_max = lane_max.max(lane.own_max_speed);
                    lane_min = lane_min.min(lane.own_min_speed);
                }
            }
            if !lane_max.is_finite() || lane_max <= 0.0 {
                continue;
            }
            if !lane_min.is_finite() || lane_min <= 0.0 {
                lane_min = lane_max;
            }
            let v_max = lane_max / 3.6 * self.speed_scale;
            let v_min = lane_min / 3.6 * self.speed_scale;
            fastest += dist_m / v_max;
            slowest += dist_m / v_min;
        }
        (fastest, slowest)
    }

    /// Grade a completion time to `0..=SCORE_MAX`. Finishing at the theoretical
    /// max speed scores `SCORE_MAX`; finishing at the min speed scores `0`.
    pub fn grade_time(&self, elapsed_s: f32) -> i64 {
        let (fastest, slowest) = self.theoretical_times_s();
        if slowest <= fastest {
            return Self::SCORE_MAX;
        }
        let frac = (slowest - elapsed_s) / (slowest - fastest);
        (frac.clamp(0.0, 1.0) * Self::SCORE_MAX as f32).round() as i64
    }
}

impl Lanes {
    pub fn total(&self) -> usize {
        self.incoming.len() + self.outgoing.len()
    }

    /// Look up a lane by flat index.
    ///
    /// Flat layout (left→right on screen):
    /// `incoming[last] … incoming[0] ║ outgoing[0] … outgoing[last]`
    ///
    /// Incoming lanes are indexed **inward-out** in the slice (index 0 = closest to
    /// center divider) but rendered outermost-first, so the slice order is reversed
    /// here. Outgoing lanes render left-to-right matching the slice order.
    pub fn get(&self, flat_idx: usize) -> Option<&Lane> {
        let in_n = self.incoming.len();
        if flat_idx < in_n {
            Some(&self.incoming[in_n - 1 - flat_idx])
        } else {
            self.outgoing.get(flat_idx - in_n)
        }
    }

    /// Flat index of the first outgoing lane (the player's default start lane).
    pub fn player_start_idx(&self) -> usize {
        self.incoming.len()
    }

    pub fn is_incoming(&self, flat_idx: usize) -> bool {
        flat_idx < self.incoming.len()
    }
}
