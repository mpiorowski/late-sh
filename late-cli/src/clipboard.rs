#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::io::Cursor;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const IMAGE_MAX_PIXELS: usize = 25_000_000;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const IMAGE_MAX_RGBA_BYTES: usize = 64 * 1024 * 1024;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const IMAGE_MAX_PNG_BYTES: usize = 10 * 1024 * 1024;
/// A copied file is read whole before decoding; past this it is refused.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const IMAGE_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// The clipboard's image as PNG bytes. A screenshot or a copied picture is
/// pixel data; a file copied in Finder, Explorer or a Linux file manager is
/// a list of paths instead, so when there are no pixels the first copied
/// file is read as an image.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn image_png_bytes() -> Result<Vec<u8>> {
    let mut clipboard = arboard::Clipboard::new().context("failed to access system clipboard")?;
    match clipboard.get_image() {
        Ok(image) => png_from_rgba(image.width, image.height, image.bytes.into_owned()),
        Err(image_err) => match clipboard.get().file_list() {
            Ok(paths) => match paths.first() {
                Some(path) => png_from_file(path),
                None => Err(no_image(image_err)),
            },
            Err(_) => Err(no_image(image_err)),
        },
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn no_image(image_err: arboard::Error) -> anyhow::Error {
    anyhow::Error::new(image_err).context(
        "clipboard does not contain an image or an image file; on Wayland, `wl-paste -l` should list an image MIME type like image/png",
    )
}

/// A copied image file, decoded and re-encoded as PNG under the same caps as
/// clipboard pixels. The file size is checked before anything is decoded.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn png_from_file(path: &Path) -> Result<Vec<u8>> {
    let size = std::fs::metadata(path)
        .with_context(|| format!("reading copied file {}", path.display()))?
        .len();
    if size > IMAGE_MAX_FILE_BYTES {
        anyhow::bail!("copied image file is too large");
    }
    let image = image::ImageReader::open(path)
        .with_context(|| format!("opening copied file {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("reading copied file {}", path.display()))?
        .decode()
        .with_context(|| format!("copied file {} is not an image", path.display()))?
        .into_rgba8();
    let (width, height) = image.dimensions();
    png_from_rgba(width as usize, height as usize, image.into_raw())
}

/// RGBA8 pixels as PNG, refusing images past the size caps.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn png_from_rgba(width: usize, height: usize, rgba: Vec<u8>) -> Result<Vec<u8>> {
    let pixel_count = width
        .checked_mul(height)
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
    if rgba.len() != expected_len {
        anyhow::bail!("clipboard image data had unexpected length");
    }
    if rgba.len() > IMAGE_MAX_RGBA_BYTES {
        anyhow::bail!("clipboard image dimensions are too large");
    }

    let mut png = Vec::new();
    {
        let cursor = Cursor::new(&mut png);
        let encoder = PngEncoder::new(cursor);
        encoder
            .write_image(&rgba, width as u32, height as u32, ExtendedColorType::Rgba8)
            .context("failed to encode clipboard image as PNG")?;
    }
    if png.len() > IMAGE_MAX_PNG_BYTES {
        anyhow::bail!("clipboard image is too large");
    }
    Ok(png)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn image_png_bytes() -> Result<Vec<u8>> {
    anyhow::bail!("clipboard image upload is not supported on this platform");
}

#[cfg(test)]
#[path = "clipboard_test.rs"]
mod clipboard_test;
