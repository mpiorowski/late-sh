use anyhow::{Result, bail};
use image::GenericImageView;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const MAX_DECODED_IMAGE_PIXELS: u64 = 25_000_000;

pub async fn fetch_and_render_image(
    url: String,
    max_width: u32,
    max_height: u32,
) -> Result<Vec<Line<'static>>> {
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
        let target_width = width.min(max_width);
        let target_height = height.min(max_height * 2);

        let scale = f32::min(
            target_width as f32 / width as f32,
            target_height as f32 / height as f32,
        );
        let new_w = (width as f32 * scale).round() as u32;
        let new_h = (height as f32 * scale).round() as u32;

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

        Ok::<Vec<Line<'static>>, anyhow::Error>(lines)
    })
    .await?
}
