//! Drawing a piece into any rect: the Zen ascii tile, zoomed or not, and the
//! away screensaver over the whole terminal.

use late_core::models::user::{AsciiPiece, SceneStyle};
use ratatui::{Frame, layout::Rect, style::Color};

use super::piece::{ShadedFrame, frame_index, picture, pixel};

/// Draw `piece` over `area` as it stands at `clock_ms` on the shared clock
/// (`piece::clock_now`): the frame edge is the piece's own, by its cadence.
pub(crate) fn draw_piece(frame: &mut Frame, area: Rect, piece: AsciiPiece, clock_ms: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let scene = picture(piece, frame_index(piece, clock_ms));
    match piece.style {
        SceneStyle::Pixels => draw_pixels(frame, area, &scene),
        SceneStyle::Dots => draw_dots(frame, area, &scene),
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
        let col =
            ((x0 + (f64::from(tx) + 0.5) * per).floor().max(0.0) as usize).min(scene.cols - 1);
        let row = |half: f64| {
            ((y0 + (2.0 * f64::from(ty) + half) * per).floor().max(0.0) as usize)
                .min(scene.rows - 1)
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

/// A scene as a halftone on a square grid of dots, the original's look:
/// each scene cell a dot (two scene rows to a terminal cell, so the grid
/// is the scene's own 200x100 at 200x50), sized and lit by the cell's
/// brightness. Braille draws the three sizes: one dot, a diagonal pair,
/// a 2x2 cluster, each at the same place in its quarter of the glyph, so
/// the grid stays regular and only the dots grow. The sizes take plain
/// thresholds, no dither: a dither between sizes is what turned the grid
/// into a texture. The tone does the rest: the ink over the ground on a
/// steep curve, so the dim sky falls away to dark and the fog, the sun and
/// its beams stand out. Both read the brightness after a local contrast
/// boost (`sharpened`), which keeps the trees: they are only a little
/// darker than the fog around them, and without it the few steps of a
/// halftone flatten the two into one dim field. The tone is held to
/// `TONES` steps and the ink to `INK_STEP` a channel, so a cell only
/// changes when the scene moves it a visible step: a slow piece's frame
/// then changes a few dozen cells, not every cell whose colour wobbled.

/// Each scene cell's brightness with its local contrast raised (unsharp
/// mask): the cell's difference from a box blur of `radius` cells around
/// it, scaled by `amount`, is added back. A dark tree beside lit fog goes
/// darker and the fog beside it brighter, so an edge the halftone's few
/// steps would flatten survives them.
fn sharpened(scene: &ShadedFrame, radius: usize, amount: f64) -> Vec<f64> {
    let (w, h) = (scene.cols, scene.rows);
    let level: Vec<f64> = scene.cells.iter().map(|c| c.level).collect();
    let blur_line = |get: &dyn Fn(usize) -> f64, n: usize| -> Vec<f64> {
        (0..n)
            .map(|i| {
                let (a, b) = (i.saturating_sub(radius), (i + radius).min(n - 1));
                (a..=b).map(get).sum::<f64>() / (b - a + 1) as f64
            })
            .collect()
    };
    let mut across = vec![0.0; w * h];
    for r in 0..h {
        let line = blur_line(&|x| level[r * w + x], w);
        across[r * w..(r + 1) * w].copy_from_slice(&line);
    }
    let mut out = level.clone();
    for x in 0..w {
        let line = blur_line(&|r| across[r * w + x], h);
        for r in 0..h {
            let k = r * w + x;
            out[k] = (level[k] + amount * (level[k] - line[r])).clamp(0.0, 1.0);
        }
    }
    out
}

fn draw_dots(frame: &mut Frame, area: Rect, scene: &ShadedFrame) {
    let at = sampling(area, scene);
    let [gr, gg, gb] = scene.ground;
    let ground = Color::Rgb(gr, gg, gb);
    // Below `FLOOR` brightness a cell is dark, from `CEILING` it is at full
    // ink; the curve spreads what lies between.
    const FLOOR: f64 = 0.12;
    const CEILING: f64 = 0.92;
    const CURVE: f64 = 1.2;
    const TONES: f64 = 12.0;
    // The local contrast: how far around a cell its surround reaches, in
    // scene cells, and how strongly its difference from it is pushed.
    const SURROUND: usize = 4;
    const SHARPEN: f64 = 1.0;
    const INK_STEP: f64 = 8.0;
    // The size thresholds: a diagonal pair, then a cluster.
    const PAIR: f64 = 0.55;
    const CLUSTER: f64 = 0.72;
    // Braille bits by size in the upper quarter (dot 1; dots 1, 5; dots 1,
    // 2, 4, 5) and the lower (dot 3; dots 3, 8; dots 3, 6, 7, 8).
    const UPPER: [u32; 4] = [0, 0x01, 0x11, 0x1b];
    const LOWER: [u32; 4] = [0, 0x04, 0x84, 0xe4];
    let levels = sharpened(scene, SURROUND, SHARPEN);
    let tone = |level: f64| {
        let t = ((level - FLOOR) / (CEILING - FLOOR))
            .clamp(0.0, 1.0)
            .powf(CURVE);
        (t * TONES).round() / TONES
    };
    let size = |level: f64, tone: f64| match (tone > 0.0, level) {
        (false, _) => 0,
        (true, level) if level >= CLUSTER => 3,
        (true, level) if level >= PAIR => 2,
        (true, _) => 1,
    };
    let buffer = frame.buffer_mut();
    for ty in 0..area.height {
        for tx in 0..area.width {
            let (col, upper, lower) = at(tx, ty);
            let top = scene.cells[upper * scene.cols + col];
            let bottom = scene.cells[lower * scene.cols + col];
            let (l_top, l_bottom) = (
                levels[upper * scene.cols + col],
                levels[lower * scene.cols + col],
            );
            let Some(cell) = buffer.cell_mut((area.x + tx, area.y + ty)) else {
                continue;
            };
            let (t_top, t_bottom) = (tone(l_top), tone(l_bottom));
            let bits = UPPER[size(l_top, t_top)] | LOWER[size(l_bottom, t_bottom)];
            if bits == 0 {
                cell.set_char(' ').set_fg(ground).set_bg(ground);
                continue;
            }
            let glyph = char::from_u32(0x2800 + bits).expect("a braille pattern");
            // One ink for the cell: the rows' inks, the brighter lending
            // more, at the brighter row's tone.
            let weight = t_top + t_bottom;
            let lit = t_top.max(t_bottom);
            let mut ink = [0u8; 3];
            for c in 0..3 {
                let mixed = (top.ink[c] * t_top + bottom.ink[c] * t_bottom) / weight;
                let base = f64::from(scene.ground[c]);
                let toned = base + (mixed * 255.0 - base) * lit;
                ink[c] = ((toned / INK_STEP).round() * INK_STEP).min(255.0) as u8;
            }
            cell.set_char(glyph)
                .set_fg(Color::Rgb(ink[0], ink[1], ink[2]))
                .set_bg(ground);
        }
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
