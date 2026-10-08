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
use late_core::models::user::AsciiPiece;

/// One frame edge: the half tier the render loop wakes on while a piece is
/// up (`tick.rs`, `ANIM_HALF_TICK`). ~7.5fps keeps the slow pieces fluid,
/// and a full-screen piece at this pace is what an away session costs in
/// bytes.
pub(crate) const FRAME_MS: u64 = 132;

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

/// One cell of a shaded scene: its colour before it is quantised, and the
/// brightness the halftone turns into dot size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shade {
    pub level: f64,
    pub rgb: [f64; 3],
}

/// A shaded scene's frame on square cells: what the renderer samples down to
/// the terminal's grid and dithers into dots there (`ui.rs`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShadedFrame {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Shade>,
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

/// The frame edge `now` falls in, on the process-wide clock every session
/// shares, so two people away at once watch the same frame.
pub(crate) fn frame_index_now() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_millis() as u64 / FRAME_MS
}

/// Play time in seconds at a frame edge.
pub(crate) fn seconds(frame: u64) -> f64 {
    frame as f64 * FRAME_MS as f64 / 1000.0
}

/// The piece's frame at `frame`, for an area of `cols` x `rows` cells. The
/// fixed-size pieces come from the shared cache; the field is drawn to the
/// area, so it is computed per call (it is the cheapest piece).
pub(crate) fn picture(piece: AsciiPiece, frame: u64, cols: usize, rows: usize) -> Picture {
    let t = seconds(frame);
    match piece {
        AsciiPiece::AuroraFjord => cached(piece, frame, || {
            Picture::Shaded(Arc::new(super::aurora_fjord::frame(t)))
        }),
        AsciiPiece::LavaLamp => cached(piece, frame, || {
            Picture::Text(Arc::new(super::lava_lamp::frame(t)))
        }),
        AsciiPiece::Donut => cached(piece, frame, || {
            Picture::Text(Arc::new(super::donut::frame(t)))
        }),
        AsciiPiece::Plasma => Picture::Field(Arc::new(super::plasma::frame(t, cols, rows))),
    }
}

struct Cached {
    piece: AsciiPiece,
    frame: u64,
    picture: Picture,
}

/// The latest frame of each fixed-size piece. Computed outside the lock, so
/// a session drawing the aurora never holds up another drawing the donut;
/// two sessions racing on one edge both compute it and the second write
/// wins, which costs a frame of CPU and nothing else.
fn cached(piece: AsciiPiece, frame: u64, make: impl FnOnce() -> Picture) -> Picture {
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
