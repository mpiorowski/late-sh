use std::{io::Cursor, sync::Arc};

use anyhow::{Context, Result, bail};
use image::GenericImageView;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use super::terminal_image::TerminalImageData;

const MAX_DECODED_IMAGE_PIXELS: u64 = 25_000_000;
const TERMINAL_IMAGE_CELL_PIXEL_WIDTH: u32 = 8;
const TERMINAL_IMAGE_CELL_PIXEL_HEIGHT: u32 = 16;
const TERMINAL_IMAGE_MAX_COLS: u32 = 120;
const TERMINAL_IMAGE_MAX_ROWS: u32 = 32;

pub async fn fetch_and_render_image(
    url: String,
    max_width: u32,
    max_height: u32,
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
        let (fallback_cols, fallback_rows) =
            display_cells_for_image(width, height, max_width, max_height);
        let fallback_width = u32::from(fallback_cols).max(1);
        let fallback_height = u32::from(fallback_rows).saturating_mul(2).max(1);

        let scale = f32::min(
            fallback_width as f32 / width as f32,
            fallback_height as f32 / height as f32,
        );
        let new_w = (width as f32 * scale).round().max(1.0) as u32;
        let new_h = (height as f32 * scale).round().max(1.0) as u32;

        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::CatmullRom);
        let rgba_img = resized.to_rgba8();
        let (w, h) = rgba_img.dimensions();

        let mut lines = Vec::new();
        for y in (0..h).step_by(2) {
            let mut spans = Vec::new();
            for x in 0..w {
                let top_pixel = rgba_img.get_pixel(x, y);
                let bottom_pixel = if y + 1 < h {
                    rgba_img.get_pixel(x, y + 1)
                } else {
                    &image::Rgba([0, 0, 0, 0])
                };

                let has_fg = top_pixel[3] > 0;
                let has_bg = bottom_pixel[3] > 0;
                if !has_fg && !has_bg {
                    spans.push(Span::raw(" "));
                    continue;
                }

                let mut style = Style::default();
                if has_fg {
                    style = style.fg(Color::Rgb(top_pixel[0], top_pixel[1], top_pixel[2]));
                }
                if has_bg {
                    style = style.bg(Color::Rgb(
                        bottom_pixel[0],
                        bottom_pixel[1],
                        bottom_pixel[2],
                    ));
                }
                spans.push(Span::styled("▀", style));
            }
            lines.push(Line::from(spans));
        }

        let (terminal_cols, terminal_rows) = display_cells_for_image(
            width,
            height,
            TERMINAL_IMAGE_MAX_COLS,
            TERMINAL_IMAGE_MAX_ROWS,
        );
        let terminal_width = u32::from(terminal_cols)
            .saturating_mul(TERMINAL_IMAGE_CELL_PIXEL_WIDTH)
            .max(1);
        let terminal_height = u32::from(terminal_rows)
            .saturating_mul(TERMINAL_IMAGE_CELL_PIXEL_HEIGHT)
            .max(1);
        let terminal_img = img.resize_exact(
            terminal_width,
            terminal_height,
            image::imageops::FilterType::Lanczos3,
        );
        let terminal_rgba = terminal_img.to_rgba8();
        let mut png = Vec::new();
        {
            let cursor = Cursor::new(&mut png);
            let encoder = PngEncoder::new(cursor);
            encoder
                .write_image(
                    terminal_rgba.as_raw(),
                    terminal_width,
                    terminal_height,
                    ExtendedColorType::Rgba8,
                )
                .context("failed to encode terminal image preview")?;
        }

        Ok::<InlineImagePreview, anyhow::Error>(InlineImagePreview {
            fallback_lines: lines,
            terminal: Some(TerminalImageData {
                png_bytes: Arc::new(png),
                pixel_width: terminal_width,
                pixel_height: terminal_height,
                display_cols: terminal_cols,
                display_rows: terminal_rows,
            }),
        })
    })
    .await?
}

#[derive(Clone, Debug)]
pub struct InlineImagePreview {
    pub fallback_lines: Vec<Line<'static>>,
    pub terminal: Option<TerminalImageData>,
}

impl InlineImagePreview {
    pub(crate) fn display_lines(&self) -> Vec<Line<'static>> {
        self.fallback_lines.clone()
    }
}

fn display_cells_for_image(width: u32, height: u32, max_cols: u32, max_rows: u32) -> (u16, u16) {
    if width == 0 || height == 0 || max_cols == 0 || max_rows == 0 {
        return (1, 1);
    }

    let max_cols = max_cols.max(1);
    let max_rows = max_rows.max(1);
    let mut cols = width.min(max_cols).max(1);
    let mut rows = ((cols as f32 * height as f32 / width as f32) / 2.0)
        .ceil()
        .max(1.0) as u32;

    if rows > max_rows {
        rows = max_rows;
        cols = ((rows as f32 * 2.0 * width as f32 / height as f32)
            .ceil()
            .max(1.0) as u32)
            .min(max_cols);
    }

    (
        cols.min(u32::from(u16::MAX)) as u16,
        rows.min(u32::from(u16::MAX)) as u16,
    )
}
