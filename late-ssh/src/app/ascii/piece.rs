//! What every piece hands the renderer, the one clock they all play on,
//! and the frame cache that lets every session share a frame.
//!
//! The pieces are ports of ascii.rest by @bas3line (MIT, see
//! `LICENSE-ascii-rest` beside this file). Each one is a pure function of
//! play time, so a frame is the same for every session that asks for it:
//! the cache computes it once per frame edge for the whole process.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use late_core::MutexRecover;
use late_core::models::user::{AsciiPiece, Scene, SceneStyle};

/// How a piece plays: how often it draws a new frame, and how fast its play
/// time runs. What a session pays for a piece on screen is the cells that
/// change per frame times the frames per second, so the cadence is the
/// piece's cost as much as its look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cadence {
    /// A new frame on every half-tier edge (`FRAME_MS`, ~7.5fps), play
    /// time at the wall clock's pace: alpine dawn in dots, the one piece
    /// whose frame is cheap enough for it under the wire budget
    /// (`ui_test.rs`, `every_piece_stays_under_the_wire_budget`).
    Half,
    /// A new frame on every quarter-tier edge (`QUARTER_FRAME_MS`,
    /// ~3.8fps), play time at the wall clock's pace: alpine dawn in
    /// pixels, whose frame is a true-colour cell for every scene cell that
    /// moved and so several times the bytes of the dots' (Cost in
    /// `CONTEXT.md`); half the frames keeps it under the budget.
    Quarter,
    /// A new frame every `frame_ms` (a whole number of the 1Hz edges the
    /// idle floor already takes, so the piece never wakes the render loop),
    /// play time at `rate` of the wall clock: a crawl that moves a few
    /// cells a second, so an away session under it costs about what an
    /// idle one did. The default screensaver, earthrise, and the misty
    /// forest; each pays its budget its own way (`cadence`). The aurora
    /// plays this way too, at a fifth of the wall clock: its whole sky
    /// moves every frame, so no faster rate fits the wire budget.
    Slow { frame_ms: u64, rate: f64 },
}

impl Cadence {
    pub(crate) fn frame_ms(self) -> u64 {
        match self {
            Self::Half => FRAME_MS,
            Self::Quarter => QUARTER_FRAME_MS,
            Self::Slow { frame_ms, .. } => frame_ms,
        }
    }

    /// Play seconds per wall second.
    fn rate(self) -> f64 {
        match self {
            Self::Half | Self::Quarter => 1.0,
            Self::Slow { rate, .. } => rate,
        }
    }
}

/// The pace a piece plays at: the fastest its bytes a second allow under
/// the wire budget (`ui_test.rs`, `every_piece_stays_under_the_wire_budget`,
/// `WIRE_BUDGET_KB_PER_S`). A 1Hz scene's cadence is the scene's, whatever
/// style draws it (the forest drifts a hair every second, the Earth turns
/// a visible notch every few seconds, `ui_test.rs` holds both to one cell
/// budget; the aurora drifts a fifth of a second of play every second).
/// Alpine dawn's is its style's: dots at the half tier, pixels at the
/// quarter, since a pixel frame is several times the bytes of a dots frame.
pub(crate) fn cadence(piece: AsciiPiece) -> Cadence {
    match (piece.scene, piece.style) {
        (Scene::Earthrise, _) => Cadence::Slow {
            frame_ms: EARTH_FRAME_MS,
            rate: EARTH_RATE,
        },
        (Scene::MistyForest, _) => Cadence::Slow {
            frame_ms: SLOW_FRAME_MS,
            rate: SLOW_RATE,
        },
        (Scene::AuroraFjord, _) => Cadence::Slow {
            frame_ms: SLOW_FRAME_MS,
            rate: AURORA_RATE,
        },
        (Scene::AlpineDawn, SceneStyle::Dots) => Cadence::Half,
        (Scene::AlpineDawn, SceneStyle::Pixels) => Cadence::Quarter,
    }
}

/// One frame edge of the lively pieces: the half tier the render loop wakes
/// on while one is up (`tick.rs`, `ANIM_HALF_TICK`). ~7.5fps keeps them
/// fluid, and a full-screen piece at this pace is what an away session
/// under one costs in bytes.
pub(crate) const FRAME_MS: u64 = 132;
/// One frame edge of the lively pieces in pixels: the quarter tier
/// (`tick.rs`, `ANIM_QUARTER_TICK`), every other half edge.
pub(crate) const QUARTER_FRAME_MS: u64 = 2 * FRAME_MS;
/// One frame edge of the slow pieces: the 1Hz edge the render loop already
/// takes while idle (`tick.rs`), so a slow piece never wakes it faster.
pub(crate) const SLOW_FRAME_MS: u64 = 1000;
/// How fast the misty forest's play time runs against the wall clock: a
/// frame a second at this rate drifts its fog a fraction of a cell, which
/// the halftone turns into a few dots moving, not a repaint; the forest
/// plays its beams faster than this on its own (`misty_forest::crawl`).
/// `ui_test.rs` holds the cell budget; the count grows linearly with this.
pub(crate) const SLOW_RATE: f64 = 0.01;
/// Earthrise's frame edge, every fourth 1Hz edge, and its play rate: the
/// only cells a frame moves are the Earth's disc, and any motion at all
/// flips the cells sitting on a tone step, so it plays fewer, bigger
/// steps, a visible notch of the globe every few seconds, for about a
/// turn in half an hour (`earthrise::frame`, the spin), at the same bytes
/// a second as the forest.
pub(crate) const EARTH_FRAME_MS: u64 = 4 * SLOW_FRAME_MS;
pub(crate) const EARTH_RATE: f64 = 0.08;
/// How fast the aurora's play time runs against the wall clock, a frame a
/// second: its sky and water move as a whole, so a frame flips most of
/// the screen at any rate, and only one frame a second at this fraction
/// of the clock fits the wire budget in pixels (Cost in `CONTEXT.md`).
pub(crate) const AURORA_RATE: f64 = 0.2;

/// One cell of a shaded scene: the brightness the original's halftone turns
/// into dot size, the colour it shades the cell with before any of that,
/// and the ink its largest dot would be drawn in (the piece's own rule,
/// clamped to 0..=1), which is what the terminal draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shade {
    pub level: f64,
    pub rgb: [f64; 3],
    pub ink: [f64; 3],
}

/// A shaded scene's frame on square cells, on its own ground colour: what
/// the renderer samples down to the terminal's grid (`ui.rs`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShadedFrame {
    pub cols: usize,
    pub rows: usize,
    pub ground: [u8; 3],
    pub cells: Vec<Shade>,
}

/// How far apart a pixel's colour steps are, per channel, counted from the
/// ground: a cell only changes when the scene moves it this much, so the
/// water's shimmer and the sky's drift stop flipping every cell they touch
/// by one level (the dots style rounds its ink the same way). The ground
/// itself is always exact, so bare ground still draws as a plain space.
pub(crate) const PIXEL_STEP: f64 = 8.0;

/// A cell as one solid pixel: its ink over the ground by its brightness,
/// held to `PIXEL_STEP` steps from the ground. The original's dot of that
/// ink covers that much of the cell, so from a step back this is the same
/// tone; drawn solid it fills the terminal cell, which a glyph cannot.
pub(crate) fn pixel(shade: Shade, ground: [u8; 3]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for c in 0..3 {
        let base = f64::from(ground[c]);
        let lift = (shade.ink[c] * 255.0 - base) * shade.level;
        let stepped = (lift / PIXEL_STEP).round() * PIXEL_STEP;
        out[c] = (base + stepped).clamp(0.0, 255.0) as u8;
    }
    out
}

/// Build what the scenes build once per process (alpine dawn's raymarched
/// range, about half a second of one core; earthrise's cratered ground and
/// its shadows; the aurora's and the misty forest's land). Called from
/// `main` on a blocking thread at startup, so the first session to draw a
/// scene never does it under the app lock on a runtime worker.
pub fn warm() {
    super::alpine_dawn::warm();
    super::earthrise::warm();
    super::aurora_fjord::warm();
    super::misty_forest::warm();
}

/// Milliseconds on the process-wide clock every session shares, so two
/// people away at once watch the same frame.
pub(crate) fn clock_now() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_millis() as u64
}

/// The frame edge a piece is on at `clock_ms`, by its cadence.
pub(crate) fn frame_index(piece: AsciiPiece, clock_ms: u64) -> u64 {
    clock_ms / cadence(piece).frame_ms()
}

/// A piece's play time in seconds at one of its frame edges.
pub(crate) fn seconds(piece: AsciiPiece, frame: u64) -> f64 {
    let cadence = cadence(piece);
    frame as f64 * cadence.frame_ms() as f64 / 1000.0 * cadence.rate()
}

/// The piece's frame at `frame` (one of its own edges, `frame_index`), from
/// the shared cache. A scene's frame is the same in either style, so both
/// share one entry.
pub(crate) fn picture(piece: AsciiPiece, frame: u64) -> Arc<ShadedFrame> {
    let t = seconds(piece, frame);
    cached(piece.scene, frame, || {
        Arc::new(match piece.scene {
            Scene::Earthrise => super::earthrise::frame(t),
            Scene::MistyForest => super::misty_forest::crawl(t),
            Scene::AuroraFjord => super::aurora_fjord::frame(t),
            Scene::AlpineDawn => super::alpine_dawn::frame(t),
        })
    })
}

struct Cached {
    scene: Scene,
    frame: u64,
    picture: Arc<ShadedFrame>,
}

/// The latest frame of each scene. Computed outside the lock, so a session
/// drawing the aurora never holds up another drawing the forest; two
/// sessions racing on one edge both compute it and the second write wins,
/// which costs a frame of CPU and nothing else.
fn cached(scene: Scene, frame: u64, make: impl FnOnce() -> Arc<ShadedFrame>) -> Arc<ShadedFrame> {
    static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());
    if let Some(hit) = CACHE
        .lock_recover()
        .iter()
        .find(|entry| entry.scene == scene && entry.frame == frame)
    {
        return hit.picture.clone();
    }
    let picture = make();
    let mut cache = CACHE.lock_recover();
    match cache.iter_mut().find(|entry| entry.scene == scene) {
        Some(entry) => {
            entry.frame = frame;
            entry.picture = picture.clone();
        }
        None => cache.push(Cached {
            scene,
            frame,
            picture: picture.clone(),
        }),
    }
    picture
}

/// The originals' `6.28`, a full turn rounded to two places. Their star and
/// wisp phases are written with it, and the golden fixtures were recorded
/// from them, so a port keeps it rather than `TAU`.
#[allow(clippy::approx_constant)]
pub(crate) const ROUGH_TAU: f64 = 6.28;

/// JavaScript's `x | 0`: truncate toward zero, wrap into an i32.
pub(crate) fn js_i32(value: f64) -> i32 {
    value as i64 as i32
}

/// JavaScript's `Math.round`: halves round up, toward positive infinity.
pub(crate) fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

#[cfg(test)]
#[path = "piece_test.rs"]
mod piece_test;
