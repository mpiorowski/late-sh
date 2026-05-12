use std::io::Cursor;

use anyhow::{Context, Result};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

const IMAGE_MAX_PIXELS: usize = 25_000_000;
const IMAGE_MAX_RGBA_BYTES: usize = 64 * 1024 * 1024;
const IMAGE_MAX_PNG_BYTES: usize = 10 * 1024 * 1024;

pub(crate) fn image_png_bytes() -> Result<Vec<u8>> {
    let mut clipboard = arboard::Clipboard::new().context("failed to access system clipboard")?;
    let image = clipboard
        .get_image()
        .context("clipboard does not contain an image; on Wayland, `wl-paste -l` should list an image MIME type like image/png")?;
    let pixel_count = image
        .width
        .checked_mul(image.height)
        .context("clipboard image dimensions overflowed")?;
    if pixel_count == 0 {
        anyhow::bail!("clipboard image has invalid dimensions");
    }
    if pixel_count > IMAGE_MAX_PIXELS {
        anyhow::bail!("clipboard image dimensions are too large");
    }

    let expected_len = pixel_count
        .checked_mul(4)
        .context("clipboard image byte length overflowed")?;
    let rgba_len = image.bytes.len();
    if rgba_len != expected_len {
        anyhow::bail!("clipboard image data had unexpected length");
    }
    if rgba_len > IMAGE_MAX_RGBA_BYTES {
        anyhow::bail!("clipboard image dimensions are too large");
    }

    let rgba = image.bytes.into_owned();
    let mut png = Vec::new();
    {
        let cursor = Cursor::new(&mut png);
        let encoder = PngEncoder::new(cursor);
        encoder
            .write_image(
                &rgba,
                image.width as u32,
                image.height as u32,
                ExtendedColorType::Rgba8,
            )
            .context("failed to encode clipboard image as PNG")?;
    }
    if png.len() > IMAGE_MAX_PNG_BYTES {
        anyhow::bail!("clipboard image is too large");
    }
    Ok(png)
}
