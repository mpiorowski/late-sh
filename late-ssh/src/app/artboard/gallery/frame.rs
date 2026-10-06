//! Framing: turning a rectangle of the live board into a piece.
//!
//! Pure. Given the board, its provenance, the frame, and who is hanging, it
//! crops the cells into a canvas of the frame's size, crops the provenance
//! with them, counts who painted what, and decides whether this is the
//! hanger's work at all. The rails it enforces are the ones only the hanger
//! can check (size, share); the daily cap and the duplicate refusal are
//! SQL's (`late_core::models::artboard_piece`).

use std::collections::HashMap;

use dartboard_core::{Canvas, Pos};
use dartboard_editor::Bounds;
use late_core::models::artboard_piece::{
    PIECE_MAX_HEIGHT, PIECE_MAX_WIDTH, PIECE_MIN_GLYPHS, PIECE_MIN_OWN_SHARE_PERCENT,
};
use sha2::{Digest, Sha256};

use crate::app::artboard::provenance::ArtboardProvenance;

/// A frame that passed every local rail and is ready to hang.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramedPiece {
    pub width: usize,
    pub height: usize,
    /// The crop, origin at the frame's top-left corner.
    pub canvas: Canvas,
    /// The crop's provenance, same origin, glyph origins only.
    pub provenance: ArtboardProvenance,
    pub glyph_count: usize,
    pub own_share_percent: u32,
    /// Everyone with a glyph in the frame, most glyphs first, the hanger
    /// included. Shown as the piece's credits.
    pub credits: Vec<Credit>,
    /// Hex SHA-256 over the glyphs and their relative positions, colours
    /// left out so a recolour of someone else's work hashes the same.
    pub content_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credit {
    pub username: String,
    pub glyphs: usize,
}

/// Why a frame cannot hang. Each carries what the notice needs to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Fewer non-blank glyphs than [`PIECE_MIN_GLYPHS`].
    TooSmall { glyphs: usize },
    /// Wider or taller than a piece may be.
    TooLarge { width: usize, height: usize },
    /// The hanger painted under [`PIECE_MIN_OWN_SHARE_PERCENT`] of the
    /// glyphs. `largest_other` names the biggest other hand, if any.
    OwnShareTooLow {
        percent: u32,
        largest_other: Option<Credit>,
    },
}

impl FrameError {
    /// The one-line notice for the framing bar.
    pub fn notice(&self) -> String {
        match self {
            Self::TooSmall { glyphs } => format!(
                "Too small to hang: {glyphs} glyphs in the frame, a piece needs {PIECE_MIN_GLYPHS}."
            ),
            Self::TooLarge { width, height } => format!(
                "Too big to hang: {width}x{height}, a piece is at most {PIECE_MAX_WIDTH}x{PIECE_MAX_HEIGHT}."
            ),
            Self::OwnShareTooLow {
                percent,
                largest_other: Some(other),
            } => format!(
                "Not your work: you painted {percent}% of the frame (@{} painted {} glyphs), a piece needs {PIECE_MIN_OWN_SHARE_PERCENT}%.",
                other.username, other.glyphs
            ),
            Self::OwnShareTooLow {
                percent,
                largest_other: None,
            } => format!(
                "Not your work: you painted {percent}% of the frame, a piece needs {PIECE_MIN_OWN_SHARE_PERCENT}%."
            ),
        }
    }
}

/// Crop `bounds` (inclusive, already normalized to the canvas) out of the
/// board and decide whether `username` may hang it.
pub fn frame_piece(
    canvas: &Canvas,
    provenance: &ArtboardProvenance,
    bounds: Bounds,
    username: &str,
) -> Result<FramedPiece, FrameError> {
    let width = bounds.width();
    let height = bounds.height();
    if width > PIECE_MAX_WIDTH || height > PIECE_MAX_HEIGHT {
        return Err(FrameError::TooLarge { width, height });
    }

    let mut cropped = Canvas::with_size(width, height);
    let mut cropped_provenance = ArtboardProvenance::default();
    let mut glyphs_by_user: HashMap<String, usize> = HashMap::new();
    let mut glyph_count = 0usize;
    let mut hashed: Vec<(usize, usize, char)> = Vec::new();

    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let pos = Pos { x, y };
            let Some(glyph) = canvas.glyph_at(pos) else {
                continue;
            };
            // A wide glyph reports itself from both of its cells; count it
            // once, at its origin.
            if glyph.pos != pos {
                continue;
            }
            // A wide glyph whose second half falls outside the frame is not
            // in the frame.
            if glyph.width == 2 && x + 1 > bounds.max_x {
                continue;
            }
            let local = Pos {
                x: x - bounds.min_x,
                y: y - bounds.min_y,
            };
            match glyph.fg {
                Some(fg) => {
                    cropped.put_glyph_colored(local, glyph.ch, fg);
                }
                None => {
                    cropped.put_glyph(local, glyph.ch);
                }
            }
            glyph_count += 1;
            hashed.push((local.x, local.y, glyph.ch));
            if let Some(owner) = provenance.username_at(canvas, pos) {
                cropped_provenance.set_username(local, owner);
                *glyphs_by_user.entry(owner.to_string()).or_default() += 1;
            }
        }
    }

    if glyph_count < PIECE_MIN_GLYPHS {
        return Err(FrameError::TooSmall {
            glyphs: glyph_count,
        });
    }

    let own_glyphs = glyphs_by_user.get(username).copied().unwrap_or(0);
    let own_share_percent = (own_glyphs * 100 / glyph_count) as u32;
    let mut credits: Vec<Credit> = glyphs_by_user
        .into_iter()
        .map(|(username, glyphs)| Credit { username, glyphs })
        .collect();
    credits.sort_by(|a, b| {
        b.glyphs
            .cmp(&a.glyphs)
            .then_with(|| a.username.cmp(&b.username))
    });

    if own_share_percent < PIECE_MIN_OWN_SHARE_PERCENT {
        let largest_other = credits
            .iter()
            .find(|credit| credit.username != username)
            .cloned();
        return Err(FrameError::OwnShareTooLow {
            percent: own_share_percent,
            largest_other,
        });
    }

    hashed.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(format!("{width}x{height}\n"));
    for (x, y, ch) in &hashed {
        hasher.update(format!("{x},{y},{}\n", u32::from(*ch)));
    }
    let content_hash = hex::encode(hasher.finalize());

    Ok(FramedPiece {
        width,
        height,
        canvas: cropped,
        provenance: cropped_provenance,
        glyph_count,
        own_share_percent,
        credits,
        content_hash,
    })
}

#[cfg(test)]
#[path = "frame_test.rs"]
mod frame_test;
