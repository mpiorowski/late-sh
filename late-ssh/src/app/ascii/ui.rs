//! Drawing a piece into any rect: the Zen ascii tile, zoomed or not, and the
//! away screensaver over the whole terminal.

use late_core::models::user::{AsciiPiece, SceneStyle, TextPiece};
use ratatui::{Frame, layout::Rect, style::Color};

use super::piece::{Picture, Shade, ShadedFrame, TextFrame, picture, pixel};
use crate::app::common::theme;

/// Draw `piece` at frame edge `frame_index` over `area`.
pub(crate) fn draw_piece(frame: &mut Frame, area: Rect, piece: AsciiPiece, frame_index: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let picture = picture(
        piece,
        frame_index,
        area.width as usize,
        area.height as usize,
    );
    match (piece, picture) {
        (AsciiPiece::Scene(_, SceneStyle::Pixels), Picture::Shaded(scene)) => {
            draw_pixels(frame, area, &scene)
        }
        (AsciiPiece::Scene(_, SceneStyle::Dots), Picture::Shaded(scene)) => {
            draw_dots(frame, area, &scene)
        }
        (AsciiPiece::Text(text), Picture::Text(art) | Picture::Field(art)) => {
            draw_text(frame, area, &art, ink(text))
        }
        (AsciiPiece::Scene(..), Picture::Text(_) | Picture::Field(_))
        | (AsciiPiece::Text(_), Picture::Shaded(_)) => {
            unreachable!("piece::picture draws a scene shaded and a text piece as text")
        }
    }
}

/// The one colour a text piece is drawn in.
fn ink(piece: TextPiece) -> Color {
    match piece {
        TextPiece::Plasma => theme::TEXT_DIM(),
        TextPiece::LavaLamp => theme::AMBER_GLOW(),
        TextPiece::Donut => theme::AMBER(),
    }
}

/// Where each terminal cell of `area` samples a scene scaled to cover it
/// (the overflow cropped evenly from both sides): the scene column, and the
/// upper and lower scene rows, of a cell. A terminal cell is one scene cell
/// wide and two tall.
fn sampling(area: Rect, scene: &ShadedFrame) -> impl Fn(u16, u16) -> (usize, usize, usize) + '_ {
    let (w, h) = (f64::from(area.width), f64::from(area.height));
    let (cols, rows) = (scene.cols as f64, scene.rows as f64);
    // Scene cells per terminal column; a terminal row takes twice that.
    let per = (cols / w).min(rows / (2.0 * h));
    let x0 = (cols - w * per) / 2.0;
    let y0 = (rows - 2.0 * h * per) / 2.0;
    move |tx, ty| {
        let col = ((x0 + (f64::from(tx) + 0.5) * per).floor().max(0.0) as usize).min(scene.cols - 1);
        let row = |half: f64| {
            ((y0 + (2.0 * f64::from(ty) + half) * per).floor().max(0.0) as usize).min(scene.rows - 1)
        };
        (col, row(0.5), row(1.5))
    }
}

/// A scene as pixels: the two scene rows a terminal cell stands on are the
/// halves of a `▀`, the upper in its ink, the lower in its background, each
/// the cell's ink over the ground by its brightness (`piece::pixel`). At the
/// scene's native 200x50 that is its 200x100 grid, cell for cell.
fn draw_pixels(frame: &mut Frame, area: Rect, scene: &ShadedFrame) {
    let at = sampling(area, scene);
    let rgb = |[r, g, b]: [u8; 3]| Color::Rgb(r, g, b);
    let ground = rgb(scene.ground);
    let buffer = frame.buffer_mut();
    for ty in 0..area.height {
        for tx in 0..area.width {
            let (col, upper, lower) = at(tx, ty);
            let top = rgb(pixel(scene.cells[upper * scene.cols + col], scene.ground));
            let bottom = rgb(pixel(scene.cells[lower * scene.cols + col], scene.ground));
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

/// The 4x4 ordered-dither matrix the pieces halftone with, as a threshold
/// offset in steps (about -0.47..=0.47) by scene cell.
fn bayer(col: usize, row: usize) -> f64 {
    const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    f64::from(BAYER[(row & 3) * 4 + (col & 3)]) / 16.0 - 0.47
}

/// A scene as a halftone of braille dots: each scene row of a terminal cell
/// is a 2x2 of its dots, 0 to 4 of them lit by the row's brightness (ordered
/// dither at scene coordinates), the cell in the one ink its lit rows
/// share, on the ground.
fn draw_dots(frame: &mut Frame, area: Rect, scene: &ShadedFrame) {
    let at = sampling(area, scene);
    let [gr, gg, gb] = scene.ground;
    let ground = Color::Rgb(gr, gg, gb);
    // How many of a row's four dots are lit, and which: top-left first, then
    // bottom-right, top-right, bottom-left, so two lit dots sit diagonal.
    let lit = |shade: Shade, col: usize, row: usize| {
        ((shade.level * 4.0 + bayer(col, row)).round().clamp(0.0, 4.0)) as usize
    };
    // Braille dot bits for [top-left, bottom-right, top-right, bottom-left]
    // of the upper 2x2 (dots 1, 5, 4, 2) and the lower 2x2 (dots 3, 8, 6, 7).
    const UPPER: [u32; 4] = [0x01, 0x10, 0x08, 0x02];
    const LOWER: [u32; 4] = [0x04, 0x80, 0x20, 0x40];
    let buffer = frame.buffer_mut();
    for ty in 0..area.height {
        for tx in 0..area.width {
            let (col, upper, lower) = at(tx, ty);
            let top = scene.cells[upper * scene.cols + col];
            let bottom = scene.cells[lower * scene.cols + col];
            let (n_top, n_bottom) = (lit(top, col, upper), lit(bottom, col, lower));
            let Some(cell) = buffer.cell_mut((area.x + tx, area.y + ty)) else {
                continue;
            };
            if n_top + n_bottom == 0 {
                cell.set_char(' ').set_fg(ground).set_bg(ground);
                continue;
            }
            let bits = UPPER[..n_top].iter().chain(&LOWER[..n_bottom]).fold(0, |b, bit| b | bit);
            let glyph = char::from_u32(0x2800 + bits).expect("a braille pattern");
            let weight = (n_top + n_bottom) as f64;
            let mut ink = [0u8; 3];
            for c in 0..3 {
                let mixed = (top.ink[c] * n_top as f64 + bottom.ink[c] * n_bottom as f64) / weight;
                ink[c] = (mixed * 255.0).round() as u8;
            }
            cell.set_char(glyph)
                .set_fg(Color::Rgb(ink[0], ink[1], ink[2]))
                .set_bg(ground);
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
