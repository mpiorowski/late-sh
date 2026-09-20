//! Sliding Puzzle's art: a gallery piece laid over the board and cut into
//! tiles. Pure; the day's piece comes from `svc.rs`, the tiles are drawn by
//! `ui.rs`.
//!
//! Text cannot be scaled, so the tile size follows the piece: each tile is
//! the piece divided by the board's dimension, never smaller than
//! [`MIN_ART_TILE_GEOMETRY`], and the piece sits centred in the grid with
//! blank cells around it. A grid wider than the terminal is the UI's
//! problem, not this module's.

use dartboard_core::{Canvas, Pos};
use late_core::models::chips::Difficulty;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use super::state::board_dimension;
use crate::app::common::theme;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TileView {
    Numbered,
    #[default]
    Art,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TileGeometry {
    pub width: u16,
    pub height: u16,
}

/// The smallest tile the art view draws: a piece narrower than the board's
/// dimension times this is padded out, not shrunk.
pub const MIN_ART_TILE_GEOMETRY: TileGeometry = TileGeometry {
    width: 6,
    height: 3,
};

/// The day's piece, decoded once per session.
#[derive(Clone, Debug)]
pub struct PuzzleArt {
    pub title: String,
    pub username: String,
    pub canvas: Canvas,
    pub width: usize,
    pub height: usize,
}

/// A piece laid over one board size: every cell of the grid is its own
/// span, so [`tile_fragment`] can cut it by index.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtGrid {
    pub geometry: TileGeometry,
    pub lines: Vec<Line<'static>>,
}

pub fn art_grid(art: &PuzzleArt, difficulty: Difficulty) -> ArtGrid {
    let dimension = board_dimension(difficulty);
    let tile_width = art.width.div_ceil(dimension).max(usize::from(MIN_ART_TILE_GEOMETRY.width));
    let tile_height = art
        .height
        .div_ceil(dimension)
        .max(usize::from(MIN_ART_TILE_GEOMETRY.height));
    let grid_width = tile_width * dimension;
    let grid_height = tile_height * dimension;
    let offset_x = (grid_width - art.width) / 2;
    let offset_y = (grid_height - art.height) / 2;

    let lines = (0..grid_height)
        .map(|y| {
            let mut spans = Vec::with_capacity(grid_width);
            let mut x = 0;
            while x < grid_width {
                let glyph = y
                    .checked_sub(offset_y)
                    .zip(x.checked_sub(offset_x))
                    .filter(|(py, px)| *py < art.height && *px < art.width)
                    .and_then(|(py, px)| art.canvas.glyph_at(Pos { x: px, y: py }));
                match glyph {
                    Some(glyph) if glyph.width > 1 => {
                        // A wide glyph on a tile's last column would straddle
                        // the cut; it becomes two blanks rather than a torn
                        // glyph.
                        let straddles = x % tile_width == tile_width - 1;
                        if straddles {
                            spans.push(blank());
                            spans.push(blank());
                        } else {
                            spans.push(Span::styled(glyph.ch.to_string(), glyph_style(glyph.fg)));
                            spans.push(Span::raw(""));
                        }
                        x += 2;
                    }
                    Some(glyph) => {
                        spans.push(Span::styled(glyph.ch.to_string(), glyph_style(glyph.fg)));
                        x += 1;
                    }
                    None => {
                        spans.push(blank());
                        x += 1;
                    }
                }
            }
            spans.truncate(grid_width);
            Line::from(spans)
        })
        .collect();

    ArtGrid {
        geometry: TileGeometry {
            width: tile_width as u16,
            height: tile_height as u16,
        },
        lines,
    }
}

fn blank() -> Span<'static> {
    Span::raw(" ")
}

fn glyph_style(fg: Option<dartboard_core::RgbColor>) -> Style {
    let color = match fg {
        Some(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        None => theme::TEXT(),
    };
    Style::default().fg(color)
}

/// The cells of `tile` in its solved position: `tile` 1 is the top-left
/// cut, the gap (0) has no art.
pub fn tile_fragment(grid: &ArtGrid, difficulty: Difficulty, tile: u8) -> Option<Vec<Line<'static>>> {
    let dimension = board_dimension(difficulty);
    if tile == 0 || usize::from(tile) >= dimension * dimension {
        return None;
    }
    let width = usize::from(grid.geometry.width);
    let height = usize::from(grid.geometry.height);
    let source = usize::from(tile - 1);
    let row_start = (source / dimension) * height;
    let column_start = (source % dimension) * width;
    let rows = grid.lines.get(row_start..row_start + height)?;
    rows.iter()
        .map(|line| {
            line.spans
                .get(column_start..column_start + width)
                .map(|spans| Line::from(spans.to_vec()))
        })
        .collect()
}
