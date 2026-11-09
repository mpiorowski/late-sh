//! In-memory Canvas rasterizer for gallery lounge cards.
//!
//! Renders a `dartboard_core::Canvas` into an `RgbaImage`, which feeds directly
//! into the chat inline-image thumbnail generator (`chafa-syms-rs`) or full-size
//! Sixel/Kitty/iTerm2 terminal image pipeline.
//!
//! Uses the same font stack and glyph metrics as `late-web/src/pages/shared/board_png.rs`.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use anyhow::bail;
use dartboard_core::{Canvas, CellValue, RgbColor};
use image::{Rgba, RgbaImage};

use crate::app::files::{
    inline_image::{InlineImagePreview, InlineImageRenderSettings, render_rgba_image_preview},
    terminal_image::{TerminalImageData, TerminalImageProtocol, terminal_image_from_dynamic},
};

pub(crate) const DEFAULT_INK: [u8; 3] = [0xd1, 0xd5, 0xdb];
pub(crate) const MODAL_BACKGROUND: [u8; 4] = [0x11, 0x18, 0x27, 0xff];
pub(crate) const TARGET_WIDTH_PX: usize = 1536;
pub(crate) const MIN_CELL_WIDTH_PX: usize = 2;
pub(crate) const MAX_CELL_WIDTH_PX: usize = 48;

const MONO_TTF: &[u8] = include_bytes!("../../../../../late-web/assets/fonts/DejaVuSansMono.ttf");
const NERD_TTF: &[u8] = include_bytes!("../../../../../late-web/static/fonts/SymbolsNerdFont-Regular.ttf");
const SYMBOLS2_TTF: &[u8] =
    include_bytes!("../../../../../late-web/static/fonts/NotoSansSymbols2-Regular.ttf");

fn fonts() -> &'static [FontRef<'static>; 3] {
    static FONTS: OnceLock<[FontRef<'static>; 3]> = OnceLock::new();
    FONTS.get_or_init(|| {
        [
            FontRef::try_from_slice(MONO_TTF).expect("DejaVu Sans Mono parses"),
            FontRef::try_from_slice(NERD_TTF).expect("Symbols Nerd Font parses"),
            FontRef::try_from_slice(SYMBOLS2_TTF).expect("Noto Sans Symbols 2 parses"),
        ]
    })
}

pub(crate) fn cell_size(width: usize) -> (usize, usize) {
    let cell_w = (TARGET_WIDTH_PX / width.max(1)).clamp(MIN_CELL_WIDTH_PX, MAX_CELL_WIDTH_PX);
    (cell_w, cell_w * 2)
}

/// Renders `canvas` to an RGBA bitmap with `background` [r, g, b, a].
/// When `background[3] == 0` (transparent), glyphs are drawn with alpha coverage.
/// When `background[3] == 255`, RGB channels blend toward `ink` by coverage with full alpha.
pub(crate) fn render_canvas_rgba(canvas: &Canvas, background: [u8; 4]) -> anyhow::Result<RgbaImage> {
    if canvas.width == 0 || canvas.height == 0 {
        bail!("cannot render an empty canvas");
    }
    let (cell_w, cell_h) = cell_size(canvas.width);
    let img_w = (canvas.width * cell_w) as u32;
    let img_h = (canvas.height * cell_h) as u32;
    let mut image = RgbaImage::from_pixel(img_w, img_h, Rgba(background));
    let scale = PxScale::from(cell_h as f32 * 14.0 / 16.0);

    for (pos, cell) in canvas.iter() {
        let ch = match cell {
            CellValue::Narrow(ch) | CellValue::Wide(ch) => *ch,
            CellValue::WideCont => continue,
        };
        if ch.is_whitespace() {
            continue;
        }
        let ink = canvas
            .fg(*pos)
            .map_or(DEFAULT_INK, |RgbColor { r, g, b }| [r, g, b]);
        let Some((font, id)) = fonts().iter().find_map(|font| {
            let id = font.glyph_id(ch);
            (id.0 != 0).then_some((font, id))
        }) else {
            continue;
        };
        let top_left = point(
            (pos.x * cell_w) as f32,
            (pos.y * cell_h) as f32 + font.as_scaled(scale).ascent(),
        );
        let Some(outlined) = font.outline_glyph(id.with_scale_and_position(scale, top_left)) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let cov = coverage.clamp(0.0, 1.0);
            if cov <= 0.0 {
                return;
            }
            let px = bounds.min.x as i64 + i64::from(gx);
            let py = bounds.min.y as i64 + i64::from(gy);
            if px < 0 || py < 0 || px >= i64::from(img_w) || py >= i64::from(img_h) {
                return;
            }
            let px = px as u32;
            let py = py as u32;
            let current = image.get_pixel_mut(px, py);

            if background[3] == 0 {
                let alpha = (cov * 255.0).round() as u8;
                if alpha > current.0[3] {
                    *current = Rgba([ink[0], ink[1], ink[2], alpha]);
                }
            } else {
                let inv = 1.0 - cov;
                let r = (f32::from(current.0[0]) * inv + f32::from(ink[0]) * cov).round() as u8;
                let g = (f32::from(current.0[1]) * inv + f32::from(ink[1]) * cov).round() as u8;
                let b = (f32::from(current.0[2]) * inv + f32::from(ink[2]) * cov).round() as u8;
                *current = Rgba([r, g, b, 255]);
            }
        });
    }

    Ok(image)
}

/// Renders a transparent in-memory canvas thumbnail for inline chat preview using `chafa-syms-rs`.
pub(crate) fn render_canvas_inline_preview(
    canvas: &Canvas,
    max_width: u32,
    max_height: u32,
    settings: InlineImageRenderSettings,
) -> anyhow::Result<InlineImagePreview> {
    let rgba = render_canvas_rgba(canvas, [0, 0, 0, 0])?;
    render_rgba_image_preview(&rgba, max_width, max_height, settings)
}

/// Renders an opaque `#111827` canvas image for the full-size Sixel/Kitty/iTerm2 Image Preview modal.
pub(crate) fn render_canvas_terminal_image(
    canvas: &Canvas,
    max_cols: u32,
    max_rows: u32,
    protocol: TerminalImageProtocol,
) -> anyhow::Result<TerminalImageData> {
    let rgba = render_canvas_rgba(canvas, MODAL_BACKGROUND)?;
    terminal_image_from_dynamic(image::DynamicImage::ImageRgba8(rgba), max_cols, max_rows, protocol)
}
