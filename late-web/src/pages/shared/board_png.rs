//! A cell canvas as a PNG: the server-side twin of `static/board.js`, for
//! the preview card a profile or gallery link unfurls into wherever it is
//! pasted (Mastodon, Slack, a chat that fetches `og:image`). Same
//! background, same default ink, same 14-on-16 glyph proportion drawn from
//! the cell's top edge. The fonts are the monospace face the browser's
//! stack names but ships no file for (DejaVu Sans Mono, `assets/fonts`)
//! and the two symbol faces the browser draws with too (`static/fonts`).
//! A glyph no face carries draws nothing, as a missing system font would.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use anyhow::{Context, bail};
use dartboard_core::{Canvas, CellValue, RgbColor};

/// The shell's background, `#111827`, as `board.js` fills it.
pub(crate) const BACKGROUND: [u8; 3] = [0x11, 0x18, 0x27];
/// The ink of an uncoloured cell, `#d1d5db`, as `board.js` draws it.
const DEFAULT_INK: [u8; 3] = [0xd1, 0xd5, 0xdb];
/// The width a rendered board aims for, so a 384-cell Artboard and a
/// 32-cell poster both come out at preview-card size.
const TARGET_WIDTH_PX: usize = 1536;
/// A cell never goes under two pixels wide (a wall-sized board stays a
/// picture) or over 48 (a small poster does not become a billboard).
const MIN_CELL_WIDTH_PX: usize = 2;
const MAX_CELL_WIDTH_PX: usize = 48;

const MONO_TTF: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSansMono.ttf");
const NERD_TTF: &[u8] = include_bytes!("../../../static/fonts/SymbolsNerdFont-Regular.ttf");
const SYMBOLS2_TTF: &[u8] = include_bytes!("../../../static/fonts/NotoSansSymbols2-Regular.ttf");

/// The font stack, parsed once: the monospace face first, then the symbol
/// faces, the order `board.js` names them in.
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

/// The pixel size of one cell for a board `width` cells wide: twice as
/// tall as wide, like the browser's 8 by 16.
pub(crate) fn cell_size(width: usize) -> (usize, usize) {
    let cell_w = (TARGET_WIDTH_PX / width.max(1)).clamp(MIN_CELL_WIDTH_PX, MAX_CELL_WIDTH_PX);
    (cell_w, cell_w * 2)
}

/// `canvas` as an RGB PNG.
pub(crate) fn render(canvas: &Canvas) -> anyhow::Result<Vec<u8>> {
    if canvas.width == 0 || canvas.height == 0 {
        bail!("cannot render an empty canvas");
    }
    let (cell_w, cell_h) = cell_size(canvas.width);
    let mut image = Image::new(canvas.width * cell_w, canvas.height * cell_h);
    // board.js: a 14px face on a 16px cell, drawn from the cell's top edge.
    let scale = PxScale::from(cell_h as f32 * 14.0 / 16.0);
    for (pos, cell) in canvas.iter() {
        let ch = match cell {
            CellValue::Narrow(ch) | CellValue::Wide(ch) => *ch,
            CellValue::WideCont => continue,
        };
        if ch.is_whitespace() {
            continue;
        }
        let Some(glyph) = canvas.glyph_at(*pos) else {
            continue;
        };
        let ink = glyph
            .fg
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
        outlined.draw(|x, y, coverage| {
            image.blend(
                bounds.min.x as i64 + i64::from(x),
                bounds.min.y as i64 + i64::from(y),
                ink,
                coverage,
            );
        });
    }
    image.encode()
}

/// `png` as the response an `og:image` fetch gets: ten minutes of cache,
/// so a link pasted into a busy room renders once, not once per server
/// that unfurls it.
pub(crate) fn response(png: Vec<u8>) -> axum::response::Response {
    use axum::{
        http::header::{CACHE_CONTROL, CONTENT_TYPE},
        response::IntoResponse,
    };
    (
        [
            (CONTENT_TYPE, "image/png"),
            (CACHE_CONTROL, "public, max-age=600"),
        ],
        png,
    )
        .into_response()
}

/// An RGB raster on the shell's background.
struct Image {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl Image {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgb: BACKGROUND.repeat(width * height),
        }
    }

    /// Lay `coverage` (0 to 1) of `ink` over the pixel at `x`, `y`; a
    /// pixel off the image (a glyph's overhang) is dropped.
    fn blend(&mut self, x: i64, y: i64, ink: [u8; 3], coverage: f32) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        let coverage = coverage.clamp(0.0, 1.0);
        let at = (y * self.width + x) * 3;
        for (channel, ink) in self.rgb[at..at + 3].iter_mut().zip(ink) {
            let base = f32::from(*channel);
            *channel = (base + (f32::from(ink) - base) * coverage).round() as u8;
        }
    }

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("writing the png header")?;
        writer
            .write_image_data(&self.rgb)
            .context("writing the png rows")?;
        writer.finish().context("finishing the png")?;
        Ok(out)
    }
}
