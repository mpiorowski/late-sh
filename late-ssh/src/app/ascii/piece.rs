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
use late_core::models::user::{AsciiPiece, Scene, TextPiece};

/// How a piece plays: how often it draws a new frame, and how fast its play
/// time runs. What a session pays for a piece on screen is the cells that
/// change per frame times the frames per second, so the cadence is the
/// piece's cost as much as its look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cadence {
    /// A new frame on every half-tier edge (`FRAME_MS`, ~7.5fps), play
    /// time at the wall clock's pace: the lively pieces.
    Half,
    /// A new frame every `SLOW_FRAME_MS`, play time at `SLOW_RATE` of the
    /// wall clock: a crawl that moves a few cells a frame, so an away
    /// session under it costs about what an idle one did. The default
    /// screensaver.
    Slow,
}

impl Cadence {
    pub(crate) fn frame_ms(self) -> u64 {
        match self {
            Self::Half => FRAME_MS,
            Self::Slow => SLOW_FRAME_MS,
        }
    }

    /// Play seconds per wall second.
    fn rate(self) -> f64 {
        match self {
            Self::Half => 1.0,
            Self::Slow => SLOW_RATE,
        }
    }
}

/// The pace a piece plays at. A scene's cadence is the scene's, whatever
/// style draws it.
pub(crate) fn cadence(piece: AsciiPiece) -> Cadence {
    match piece {
        AsciiPiece::Scene(Scene::MistyForest, _) => Cadence::Slow,
        AsciiPiece::Scene(Scene::AuroraFjord | Scene::AlpineDawn, _) | AsciiPiece::Text(_) => {
            Cadence::Half
        }
    }
}

/// One frame edge of the lively pieces: the half tier the render loop wakes
/// on while one is up (`tick.rs`, `ANIM_HALF_TICK`). ~7.5fps keeps them
/// fluid, and a full-screen piece at this pace is what an away session
/// under one costs in bytes.
pub(crate) const FRAME_MS: u64 = 132;
/// One frame edge of the slow pieces: the 1Hz edge the render loop already
/// takes while idle (`tick.rs`), so a slow piece never wakes it faster.
pub(crate) const SLOW_FRAME_MS: u64 = 1000;
/// How fast a slow piece's play time runs against the wall clock: a frame a
/// second at this rate drifts the misty forest's fog a fraction of a cell,
/// which the halftone turns into a few dots moving, not a repaint; the
/// forest plays its beams faster than this on its own (`misty_forest::crawl`).
/// `ui_test.rs` holds the cell budget; the count grows linearly with this.
pub(crate) const SLOW_RATE: f64 = 0.01;

/// A text piece's frame: `rows` lines of `cols` glyphs, drawn in one ink.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextFrame {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<char>,
}

impl TextFrame {
    pub(crate) fn blank(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![' '; cols * rows],
        }
    }

    pub(crate) fn at(&self, col: usize, row: usize) -> char {
        self.cells[row * self.cols + col]
    }

    /// The frame as ascii.rest prints it: lines joined by `\n`. What the
    /// golden frames are compared against.
    #[cfg(test)]
    pub(crate) fn to_text(&self) -> String {
        self.cells
            .chunks(self.cols)
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

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

/// A cell as one solid pixel: its ink over the ground by its brightness. The
/// original's dot of that ink covers that much of the cell, so from a step
/// back this is the same tone; drawn solid it fills the terminal cell,
/// which a glyph cannot.
pub(crate) fn pixel(shade: Shade, ground: [u8; 3]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for c in 0..3 {
        let base = f64::from(ground[c]);
        out[c] = (base + (shade.ink[c] * 255.0 - base) * shade.level).round() as u8;
    }
    out
}

/// A frame, ready to draw.
#[derive(Clone, Debug)]
pub(crate) enum Picture {
    /// A scene shaded per cell, scaled to cover the area it is drawn in.
    Shaded(Arc<ShadedFrame>),
    /// Fixed-size text art, centred in the area.
    Text(Arc<TextFrame>),
    /// A field drawn at the area's own size (plasma).
    Field(Arc<TextFrame>),
}

/// Build what the scenes build once per process (alpine dawn's raymarched
/// range, about half a second of one core; the aurora's and the misty
/// forest's land). Called from `main` on a blocking thread at startup, so
/// the first session to draw a scene never does it under the app lock on a
/// runtime worker.
pub fn warm() {
    super::alpine_dawn::warm();
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

/// The piece's frame at `frame` (one of its own edges, `frame_index`), for
/// an area of `cols` x `rows` cells. The fixed-size pieces come from the
/// shared cache; the field is drawn to the area, so it is computed per call
/// (it is the cheapest piece). A scene's frame is the same in either style,
/// so both share one entry.
pub(crate) fn picture(piece: AsciiPiece, frame: u64, cols: usize, rows: usize) -> Picture {
    let t = seconds(piece, frame);
    match piece {
        AsciiPiece::Scene(scene, _) => cached(Fixed::Scene(scene), frame, || {
            Picture::Shaded(Arc::new(match scene {
                Scene::MistyForest => super::misty_forest::crawl(t),
                Scene::AuroraFjord => super::aurora_fjord::frame(t),
                Scene::AlpineDawn => super::alpine_dawn::frame(t),
            }))
        }),
        AsciiPiece::Text(TextPiece::LavaLamp) => {
            cached(Fixed::Text(TextPiece::LavaLamp), frame, || {
                Picture::Text(Arc::new(super::lava_lamp::frame(t)))
            })
        }
        AsciiPiece::Text(TextPiece::Donut) => cached(Fixed::Text(TextPiece::Donut), frame, || {
            Picture::Text(Arc::new(super::donut::frame(t)))
        }),
        AsciiPiece::Text(TextPiece::Plasma) => {
            Picture::Field(Arc::new(super::plasma::frame(t, cols, rows)))
        }
    }
}

/// What the cache holds a frame of: a scene (whatever style draws it) or a
/// fixed-size text piece.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fixed {
    Scene(Scene),
    Text(TextPiece),
}

struct Cached {
    piece: Fixed,
    frame: u64,
    picture: Picture,
}

/// The latest frame of each fixed-size piece. Computed outside the lock, so
/// a session drawing the aurora never holds up another drawing the donut;
/// two sessions racing on one edge both compute it and the second write
/// wins, which costs a frame of CPU and nothing else.
fn cached(piece: Fixed, frame: u64, make: impl FnOnce() -> Picture) -> Picture {
    static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());
    if let Some(hit) = CACHE
        .lock_recover()
        .iter()
        .find(|entry| entry.piece == piece && entry.frame == frame)
    {
        return hit.picture.clone();
    }
    let picture = make();
    let mut cache = CACHE.lock_recover();
    match cache.iter_mut().find(|entry| entry.piece == piece) {
        Some(entry) => {
            entry.frame = frame;
            entry.picture = picture.clone();
        }
        None => cache.push(Cached {
            piece,
            frame,
            picture: picture.clone(),
        }),
    }
    picture
}

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
