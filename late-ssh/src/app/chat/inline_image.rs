use anyhow::{bail, Result};
use image::GenericImageView;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub async fn fetch_and_render_image(url: String, max_width: u32, max_height: u32) -> Result<Vec<Line<'static>>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("late-sh/1.0")
        .build()?;
    fetch_and_render_image_with_client(client, url, max_width, max_height).await
}

pub async fn fetch_and_render_image_with_client(client: reqwest::Client, url: String, max_width: u32, max_height: u32) -> Result<Vec<Line<'static>>> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let cache_key = engine.encode(&url);
    let cache_dir = std::path::Path::new("late-ssh/cache/images");
    let cache_path = cache_dir.join(format!("{}_{}x{}.json", cache_key, max_width, max_height));
    
    // 1. Try to load from disk cache first
    if cache_path.exists() {
        if let Ok(data) = tokio::fs::read_to_string(&cache_path).await {
            if let Ok(lines_data) = serde_json::from_str::<Vec<Vec<(u8, u8, u8, u8, u8, u8, bool, bool)>>>(&data) {
                let mut lines = Vec::new();
                for row in lines_data {
                    let mut spans = Vec::new();
                    for (r1, g1, b1, r2, g2, b2, has_fg, has_bg) in row {
                        if !has_fg && !has_bg {
                            spans.push(Span::raw(" "));
                            continue;
                        }
                        let mut style = Style::default();
                        if has_fg {
                            style = style.fg(Color::Rgb(r1, g1, b1));
                        }
                        if has_bg {
                            style = style.bg(Color::Rgb(r2, g2, b2));
                        }
                        spans.push(Span::styled("▀", style));
                    }
                    lines.push(Line::from(spans));
                }
                tracing::info!("Loaded image from disk cache: {}", url);
                return Ok(lines);
            }
        }
    }

    tracing::info!("Attempting to render inline image: {}", url);
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        tracing::error!("HTTP error fetching image ({}): {}", url, resp.status());
        bail!("HTTP {}", resp.status());
    }
    
    let bytes = resp.bytes().await?;
    tracing::info!("Image downloaded: {} bytes", bytes.len());
    
    let lines_to_cache = tokio::task::spawn_blocking(move || {
        tracing::info!("Decoding image...");
        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(e) => {
                tracing::error!("Image decoding failed: {}", e);
                return Err(e.into());
            }
        };
        tracing::info!("Image decoded: {}x{}", img.width(), img.height());
        
        let (width, height) = img.dimensions();
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
        
        let mut lines_data = Vec::new();
        
        for y in (0..h).step_by(2) {
            let mut row = Vec::new();
            for x in 0..w {
                let top_pixel = rgba_img.get_pixel(x, y);
                let bottom_pixel = if y + 1 < h {
                    rgba_img.get_pixel(x, y + 1)
                } else {
                    &image::Rgba([0, 0, 0, 0])
                };
                
                let has_fg = top_pixel[3] > 0;
                let has_bg = bottom_pixel[3] > 0;
                
                row.push((
                    top_pixel[0], top_pixel[1], top_pixel[2],
                    bottom_pixel[0], bottom_pixel[1], bottom_pixel[2],
                    has_fg, has_bg
                ));
            }
            lines_data.push(row);
        }
        
        Ok::<Vec<Vec<(u8, u8, u8, u8, u8, u8, bool, bool)>>, anyhow::Error>(lines_data)
    }).await??;

    // Save to disk cache
    if let Ok(json) = serde_json::to_string(&lines_to_cache) {
        let _ = tokio::fs::write(&cache_path, json).await;
    }

    // Convert to ratatui lines for return
    let mut lines = Vec::new();
    for row in lines_to_cache {
        let mut spans = Vec::new();
        for (r1, g1, b1, r2, g2, b2, has_fg, has_bg) in row {
            if !has_fg && !has_bg {
                spans.push(Span::raw(" "));
                continue;
            }
            let mut style = Style::default();
            if has_fg {
                style = style.fg(Color::Rgb(r1, g1, b1));
            }
            if has_bg {
                style = style.bg(Color::Rgb(r2, g2, b2));
            }
            spans.push(Span::styled("▀", style));
        }
        lines.push(Line::from(spans));
    }

    Ok(lines)
}
