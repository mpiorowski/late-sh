use anyhow::{Context, Result, bail};
use chafa_syms_rs::{
    Canvas, CanvasConfig, PixelType, SymbolMap, SymbolTags,
    select::{CanvasMode, CellOut},
};
use image::{GenericImageView, RgbaImage};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

const ALPHA_THRESHOLD: u8 = 128;
const MAX_DECODED_IMAGE_PIXELS: u64 = 25_000_000;

pub type InlineImagePreview = Vec<Line<'static>>;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct InlineImageRenderSettings {
    pub symbol_mode: InlineImageSymbolMode,
    pub background_rgb: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum InlineImageSymbolMode {
    #[default]
    Default,
    Octant,
    Sextant,
}

impl InlineImageSymbolMode {
    pub(crate) fn from_identity(value: &str) -> Self {
        let value = value.trim().to_ascii_lowercase();
        if ["kitty", "wezterm", "ghostty", "mtermux"]
            .iter()
            .any(|identity| value.contains(identity))
        {
            Self::Octant
        } else if ["iterm", "alacritty"]
            .iter()
            .any(|identity| value.contains(identity))
        {
            Self::Sextant
        } else {
            Self::Default
        }
    }

    pub(crate) fn from_env_hint(name: &str, value: &str) -> Self {
        if value.trim().is_empty() {
            return Self::Default;
        }
        match name.trim() {
            "TERM_PROGRAM" | "LC_TERMINAL" => Self::from_identity(value),
            "KITTY_WINDOW_ID" | "KITTY_PID" | "KITTY_PUBLIC_KEY" => Self::Octant,
            "WEZTERM_PANE" | "WEZTERM_EXECUTABLE" => Self::Octant,
            "GHOSTTY_RESOURCES_DIR" | "GHOSTTY_BIN_DIR" => Self::Octant,
            _ => Self::Default,
        }
    }

    fn symbol_map(self) -> SymbolMap {
        let mut map = SymbolMap::chafa_default();
        match self {
            Self::Default => {}
            Self::Octant => map.add_by_tags(SymbolTags::OCTANT),
            Self::Sextant => map.add_by_tags(SymbolTags::SEXTANT),
        }
        map
    }
}

pub(crate) async fn fetch_and_render_image(
    url: String,
    max_width: u32,
    max_height: u32,
    settings: InlineImageRenderSettings,
) -> Result<InlineImagePreview> {
    tracing::trace!("attempting to render inline image: {}", url);
    let bytes = crate::app::files::image_upload::download_url_bytes(
        &url,
        std::time::Duration::from_secs(15),
        crate::app::files::image_upload::max_upload_bytes(),
    )
    .await?;
    tracing::trace!("image downloaded: {} bytes", bytes.len());

    tokio::task::spawn_blocking(move || {
        tracing::trace!("decoding image...");
        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(e) => {
                tracing::trace!("image decoding failed: {}", e);
                return Err(e.into());
            }
        };
        tracing::trace!("image decoded: {}x{}", img.width(), img.height());

        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            bail!("image has invalid dimensions");
        }
        let pixel_count = u64::from(width) * u64::from(height);
        if pixel_count > MAX_DECODED_IMAGE_PIXELS {
            bail!("image dimensions are too large");
        }
        let (cols, rows) = preview_geometry(width, height, max_width, max_height);
        let rgba_img = img.to_rgba8();
        render_rgba_preview(&rgba_img, cols, rows, settings)
    })
    .await?
}

pub(crate) fn render_rgba_preview(
    img: &RgbaImage,
    cols: u32,
    rows: u32,
    settings: InlineImageRenderSettings,
) -> Result<InlineImagePreview> {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let mut config = CanvasConfig::new(cols as usize, rows as usize)
        .mode(CanvasMode::Truecolor)
        .symbol_map(settings.symbol_mode.symbol_map());
    if let Some(background_rgb) = settings.background_rgb {
        config = config.bg_color(background_rgb);
    }
    let mut canvas = Canvas::new(config);
    canvas.draw_all_pixels(
        PixelType::Rgba8,
        img.as_raw(),
        img.width() as usize,
        img.height() as usize,
        img.width() as usize * 4,
    );

    cells_to_lines(canvas.cells(), cols as usize, rows as usize)
}

fn preview_geometry(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let max_width = max_width.max(1);
    let max_height = max_height.max(1);
    let image_aspect = width as f32 / height as f32;
    let mut cols = max_width as f32;
    let mut rows = cols * 0.5 / image_aspect;
    if rows > max_height as f32 {
        rows = max_height as f32;
        cols = rows * image_aspect / 0.5;
    }
    (
        (cols.round() as u32).clamp(1, max_width),
        (rows.round() as u32).clamp(1, max_height),
    )
}

fn cells_to_lines(cells: &[CellOut], cols: usize, rows: usize) -> Result<InlineImagePreview> {
    let expected_len = cols.saturating_mul(rows);
    if cells.len() != expected_len {
        bail!(
            "chafa produced {} cells for {cols}x{rows} preview",
            cells.len()
        );
    }
    let mut lines = Vec::with_capacity(rows);
    for row in cells.chunks(cols) {
        let mut spans = Vec::with_capacity(cols);
        for cell in row {
            spans.push(cell_span(cell)?);
        }
        lines.push(Line::from(spans));
    }
    Ok(lines)
}

fn cell_span(cell: &CellOut) -> Result<Span<'static>> {
    if cell.c == 0 {
        return Ok(Span::raw(" "));
    }

    let fg = packed_truecolor(cell.fg);
    let bg = packed_truecolor(cell.bg);
    if fg.is_none() && bg.is_none() {
        return Ok(Span::raw(" "));
    }

    let ch = char::from_u32(cell.c)
        .with_context(|| format!("invalid chafa cell codepoint {}", cell.c))?;
    let mut style = Style::default();
    match (fg, bg) {
        (Some(fg), Some(bg)) => {
            style = style.fg(fg).bg(bg);
        }
        (Some(fg), None) => {
            style = style.fg(fg);
        }
        (None, Some(bg)) => {
            style = style.fg(bg).add_modifier(Modifier::REVERSED);
        }
        (None, None) => {}
    }
    Ok(Span::styled(ch.to_string(), style))
}

fn packed_truecolor(color: u32) -> Option<Color> {
    let alpha = (color >> 24) as u8;
    if alpha < ALPHA_THRESHOLD {
        return None;
    }
    Some(Color::Rgb(
        ((color >> 16) & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        (color & 0xff) as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_mode_from_identity_routes_requested_terminals() {
        assert_eq!(
            InlineImageSymbolMode::from_identity("xterm-kitty"),
            InlineImageSymbolMode::Octant
        );
        assert_eq!(
            InlineImageSymbolMode::from_identity("WezTerm 20240203"),
            InlineImageSymbolMode::Octant
        );
        assert_eq!(
            InlineImageSymbolMode::from_identity("ghostty"),
            InlineImageSymbolMode::Octant
        );
        assert_eq!(
            InlineImageSymbolMode::from_identity("mtermux"),
            InlineImageSymbolMode::Octant
        );
        assert_eq!(
            InlineImageSymbolMode::from_identity("iTerm2"),
            InlineImageSymbolMode::Sextant
        );
        assert_eq!(
            InlineImageSymbolMode::from_identity("alacritty"),
            InlineImageSymbolMode::Sextant
        );
        assert_eq!(
            InlineImageSymbolMode::from_identity("xterm-256color"),
            InlineImageSymbolMode::Default
        );
    }

    #[test]
    fn symbol_modes_extend_chafa_default() {
        let mut octant = InlineImageSymbolMode::Octant.symbol_map();
        octant.prepare();
        assert!(octant.has_symbol('\u{1cd00}'));

        let mut sextant = InlineImageSymbolMode::Sextant.symbol_map();
        sextant.prepare();
        assert!(sextant.has_symbol('\u{1fb00}'));
    }

    #[test]
    fn transparent_chafa_cell_renders_as_space() {
        let span = cell_span(&CellOut {
            c: '┈' as u32,
            fg: 0x01ff_ffff,
            bg: 0x0000_0000,
        })
        .expect("cell converts");

        assert_eq!(span.content.as_ref(), " ");
        assert_eq!(span.style.fg, None);
        assert_eq!(span.style.bg, None);
    }

    #[test]
    fn alpha_at_chafa_threshold_edge_renders_as_transparent() {
        let span = cell_span(&CellOut {
            c: '┈' as u32,
            fg: 0x7fff_ffff,
            bg: 0x0000_0000,
        })
        .expect("cell converts");

        assert_eq!(span.content.as_ref(), " ");
        assert_eq!(span.style.fg, None);
        assert_eq!(span.style.bg, None);
    }

    #[test]
    fn transparent_foreground_with_background_uses_reversed_video() {
        let span = cell_span(&CellOut {
            c: '┈' as u32,
            fg: 0x0000_0000,
            bg: 0xff12_3456,
        })
        .expect("cell converts");

        assert_eq!(span.content.as_ref(), "┈");
        assert_eq!(span.style.fg, Some(Color::Rgb(0x12, 0x34, 0x56)));
        assert!(span.style.add_modifier.contains(Modifier::REVERSED));
    }
}
