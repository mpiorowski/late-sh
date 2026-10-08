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
/// cropped evenly from both sides) and dithered into dots on the terminal's
/// own grid, so the halftone stays crisp at any size. A terminal cell is
/// one scene cell wide and two tall, so it averages the two scene rows it
/// stands on. Only the aurora is shaded, so its halftone draws it.
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
        scene.cells[row * scene.cols + col]
    };
    let [gr, gg, gb] = aurora_fjord::GROUND;
    let ground = Color::Rgb(gr, gg, gb);
    let buffer = frame.buffer_mut();
    for ty in 0..area.height {
        for tx in 0..area.width {
            let x = x0 + (f64::from(tx) + 0.5) * per;
            let top = sample(x, y0 + (2.0 * f64::from(ty) + 0.5) * per);
            let bottom = sample(x, y0 + (2.0 * f64::from(ty) + 1.5) * per);
            let shade = super::piece::Shade {
                level: (top.level + bottom.level) / 2.0,
                rgb: [
                    (top.rgb[0] + bottom.rgb[0]) / 2.0,
                    (top.rgb[1] + bottom.rgb[1]) / 2.0,
                    (top.rgb[2] + bottom.rgb[2]) / 2.0,
                ],
            };
            let (glyph, index) = aurora_fjord::dot(shade, tx as usize, ty as usize);
            let [r, g, b] = aurora_fjord::PALETTE[index as usize];
            if let Some(cell) = buffer.cell_mut((area.x + tx, area.y + ty)) {
                cell.set_char(glyph)
                    .set_fg(Color::Rgb(r, g, b))
                    .set_bg(ground);
            }
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
