//! A YouTube track's thumbnail, shrunk to the live strip's picture column
//! and painted there in half blocks (`booth/live.rs`). Pure: the fetch is
//! the service's (`svc.rs`).

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use image::{RgbaImage, imageops::FilterType};

use crate::app::live::ui::PICTURE_COLS;

/// Pixels across: one per column of the picture.
pub const THUMBNAIL_WIDTH: u32 = PICTURE_COLS as u32;
/// Pixels down, two per row: 16:9 at that width, so six rows.
pub const THUMBNAIL_HEIGHT: u32 = 12;
/// YouTube's 320x180 thumbnail is a few kilobytes; anything near this is
/// not one.
pub const THUMBNAIL_MAX_BYTES: usize = 512 * 1024;
const THUMBNAIL_MAX_PIXELS: u64 = 1920 * 1080;

pub type Thumbnail = Arc<RgbaImage>;

/// The 16:9 thumbnail YouTube serves for every video, without the black
/// bars the 4:3 sizes carry.
pub fn thumbnail_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
}

/// Decode a fetched thumbnail and shrink it to the picture column.
pub fn shrink(bytes: &[u8]) -> Result<RgbaImage> {
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .context("guessing thumbnail format")?
        .into_dimensions()
        .context("reading thumbnail dimensions")?;
    if u64::from(width) * u64::from(height) > THUMBNAIL_MAX_PIXELS {
        bail!("thumbnail is {width}x{height}, past the pixel limit");
    }
    let image = image::load_from_memory(bytes).context("decoding thumbnail")?;
    Ok(image
        .resize_exact(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT, FilterType::Triangle)
        .to_rgba8())
}

#[cfg(test)]
#[path = "thumbnail_test.rs"]
mod thumbnail_test;
