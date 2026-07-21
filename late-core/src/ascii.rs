use anyhow::{Context, Result};
use image::imageops::FilterType;

/// 10 levels of ASCII brightness, from darkest (space) to lightest (@).
const ASCII_CHARS: &[u8] = b" .:-=+*#%@";

/// Decodes image bytes, resizes them to the target dimensions, and converts to grayscale ASCII art.
pub fn bytes_to_ascii(bytes: &[u8], target_width: u32, target_height: u32) -> Result<String> {
    let img = image::load_from_memory(bytes).context("failed to decode image bytes")?;

    // Resize the image. FilterType::Triangle is fast and suitable for downscaling to ASCII.
    // We resize exactly to the target width and height without preserving aspect ratio,
    // assuming the caller has already factored in the terminal font aspect ratio (~1:2).
    let resized = img.resize_exact(target_width, target_height, FilterType::Triangle);

    // Convert the resized image to 8-bit grayscale
    let luma = resized.into_luma8();

    let mut ascii_art =
        String::with_capacity((target_width * target_height + target_height) as usize);

    for y in 0..target_height {
        for x in 0..target_width {
            let pixel = luma.get_pixel(x, y);
            // pixel[0] is brightness from 0 to 255
            // Map 0..=255 to 0..=9 (index in ASCII_CHARS)
            let index = (pixel[0] as usize * (ASCII_CHARS.len() - 1)) / 255;
            ascii_art.push(ASCII_CHARS[index] as char);
        }
        if y < target_height - 1 {
            ascii_art.push('\n');
        }
    }

    Ok(ascii_art)
}
