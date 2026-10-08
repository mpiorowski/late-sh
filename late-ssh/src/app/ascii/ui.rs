//! Drawing a piece into any rect: the Zen ascii tile, zoomed or not, and the
//! away screensaver over the whole terminal.

use late_core::models::user::AsciiPiece;
use ratatui::{Frame, layout::Rect, style::Color};

use super::{
    aurora_fjord,
    piece::{Picture, ShadedFrame, TextFrame, picture},
};
use crate::app::common::theme;

/// Draw `piece` at frame edge `frame_index` over `area`.
pub(crate) fn draw_piece(frame: &mut Frame, area: Rect, piece: AsciiPiece, frame_index: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let ink = ink(piece);
    match picture(
        piece,
        frame_index,
        area.width as usize,
        area.height as usize,
    ) {
        Picture::Shaded(scene) => draw_shaded(frame, area, &scene),
        Picture::Text(art) | Picture::Field(art) => draw_text(frame, area, &art, ink),
    }
}

/// The one colour a text piece is drawn in.
fn ink(piece: AsciiPiece) -> Color {
    match piece {
        // The aurora brings its own palette.
        AsciiPiece::AuroraFjord => theme::TEXT(),
        AsciiPiece::Plasma => theme::TEXT_DIM(),
        AsciiPiece::LavaLamp => theme::AMBER_GLOW(),
        AsciiPiece::Donut => theme::AMBER(),
    }
}

/// A scene on square cells, scaled to cover the area (the overflow is
/// cropped evenly from both sides) and drawn as pixels: a terminal cell is
/// one scene cell wide and two tall, so it shows the two scene rows it
/// stands on as the halves of a `▀`, the upper in its ink, the lower in its
/// background. Each half is the scene's halftone (`aurora_fjord::pixel`)
/// dithered at scene coordinates, so at 200x50 the picture is the original's
/// own dot grid, cell for cell. Only the aurora is shaded, so this draws it.
fn draw_shaded(frame: &mut Frame, area: Rect, scene: &ShadedFrame) {
    let (w, h) = (f64::from(area.width), f64::from(area.height));
    let (cols, rows) = (scene.cols as f64, scene.rows as f64);
    // Scene cells per terminal column; a terminal row takes twice that.
    let per = (cols / w).min(rows / (2.0 * h));
    let x0 = (cols - w * per) / 2.0;
    let y0 = (rows - 2.0 * h * per) / 2.0;
    let sample = |x: f64, y: f64| {
        let col = (x.floor().max(0.0) as usize).min(scene.cols - 1);
        let row = (y.floor().max(0.0) as usize).min(scene.rows - 1);
        let [r, g, b] = aurora_fjord::pixel(scene.cells[row * scene.cols + col], col, row);
        Color::Rgb(r, g, b)
    };
    let [gr, gg, gb] = aurora_fjord::GROUND;
    let ground = Color::Rgb(gr, gg, gb);
    let buffer = frame.buffer_mut();
    for ty in 0..area.height {
        for tx in 0..area.width {
            let x = x0 + (f64::from(tx) + 0.5) * per;
            let top = sample(x, y0 + (2.0 * f64::from(ty) + 0.5) * per);
            let bottom = sample(x, y0 + (2.0 * f64::from(ty) + 1.5) * per);
            let Some(cell) = buffer.cell_mut((area.x + tx, area.y + ty)) else {
                continue;
            };
            // Bare ground is a plain space: no glyph, no ink, fewer bytes.
            match top == ground && bottom == ground {
                true => cell.set_char(' ').set_fg(ground).set_bg(ground),
                false => cell.set_char('▀').set_fg(top).set_bg(bottom),
            };
        }
    }
}

/// Text art centred in the area; art larger than the area is cropped evenly
/// from both sides, so the middle of the piece stays in view.
fn draw_text(frame: &mut Frame, area: Rect, art: &TextFrame, ink: Color) {
    let (w, h) = (area.width as usize, area.height as usize);
    let (pad_x, crop_x) = match art.cols <= w {
        true => ((w - art.cols) / 2, 0),
        false => (0, (art.cols - w) / 2),
    };
    let (pad_y, crop_y) = match art.rows <= h {
        true => ((h - art.rows) / 2, 0),
        false => (0, (art.rows - h) / 2),
    };
    let buffer = frame.buffer_mut();
    for row in 0..art.rows.min(h) {
        for col in 0..art.cols.min(w) {
            let glyph = art.at(col + crop_x, row + crop_y);
            let x = area.x + (pad_x + col) as u16;
            let y = area.y + (pad_y + row) as u16;
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(glyph).set_fg(ink);
            }
        }
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
