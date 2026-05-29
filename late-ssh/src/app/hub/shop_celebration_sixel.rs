//! Pixel celebration sprite shown briefly over the Hub Shop body after a
//! successful purchase. Same fallback shape as the trophy: capable
//! terminals see a small "burst" rendered through the existing
//! TerminalImagePlacement pipeline, everyone else sees nothing extra
//! (the success banner alone communicates the purchase).

use std::{
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Result;
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};

use crate::app::files::terminal_image::{
    TerminalImageData, TerminalImageProtocol, terminal_image_from_bytes,
};

/// Display footprint — chosen so the burst feels punchy without
/// blanketing the shop's item list. ~12×4 cells is comfortable inside
/// the modal at all standard window sizes.
pub(crate) const CELEBRATION_DISPLAY_COLS: u16 = 12;
pub(crate) const CELEBRATION_DISPLAY_ROWS: u16 = 4;

const CANVAS_W: u32 = (CELEBRATION_DISPLAY_COLS as u32) * 8;
const CANVAS_H: u32 = (CELEBRATION_DISPLAY_ROWS as u32) * 16;

type CelebrationCache = Option<(TerminalImageProtocol, Arc<TerminalImageData>)>;

fn cache() -> &'static Mutex<CelebrationCache> {
    static CACHE: OnceLock<Mutex<CelebrationCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn celebration_terminal_image(
    protocol: TerminalImageProtocol,
) -> Result<Arc<TerminalImageData>> {
    {
        let guard = cache().lock().expect("celebration cache mutex poisoned");
        if let Some((cached_protocol, data)) = guard.as_ref()
            && *cached_protocol == protocol
        {
            return Ok(data.clone());
        }
    }
    let rgba = draw_celebration_rgba();
    let png = png_encode_rgba(&rgba)?;
    let data = terminal_image_from_bytes(
        &png,
        u32::from(CELEBRATION_DISPLAY_COLS),
        u32::from(CELEBRATION_DISPLAY_ROWS),
        protocol,
    )?;
    let arc = Arc::new(data);
    *cache().lock().expect("celebration cache mutex poisoned") = Some((protocol, arc.clone()));
    Ok(arc)
}

fn png_encode_rgba(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    PngEncoder::new(Cursor::new(&mut bytes)).write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        ExtendedColorType::Rgba8,
    )?;
    Ok(bytes)
}

fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let (xu, yu) = (x as u32, y as u32);
    if xu >= img.width() || yu >= img.height() {
        return;
    }
    img.put_pixel(xu, yu, c);
}

fn fill_disc(img: &mut RgbaImage, cx: i32, cy: i32, radius: i32, c: Rgba<u8>) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                put(img, cx + dx, cy + dy, c);
            }
        }
    }
}

/// Procedural confetti burst: a handful of coloured discs scattered
/// across the canvas with a central highlight. Deterministic positions
/// so the image is identical across renders (the cache hashes on raw
/// bytes; randomising would defeat the cache).
fn draw_celebration_rgba() -> RgbaImage {
    let transparent = Rgba([0, 0, 0, 0]);
    let mut img = RgbaImage::from_pixel(CANVAS_W, CANVAS_H, transparent);

    let gold = Rgba([255, 220, 120, 255]);
    let amber = Rgba([255, 158, 70, 255]);
    let pink = Rgba([255, 132, 188, 255]);
    let teal = Rgba([90, 220, 220, 255]);
    let lilac = Rgba([180, 158, 255, 255]);

    // Confetti specks — (x, y, radius, colour).
    let specks: &[(i32, i32, i32, Rgba<u8>)] = &[
        (10, 8, 2, gold),
        (24, 14, 3, amber),
        (40, 6, 2, pink),
        (56, 18, 3, teal),
        (72, 10, 2, lilac),
        (16, 30, 2, pink),
        (32, 38, 3, gold),
        (48, 30, 2, lilac),
        (64, 38, 3, amber),
        (80, 26, 2, teal),
        (86, 44, 2, pink),
        (8, 52, 3, gold),
    ];
    for (x, y, r, c) in specks {
        fill_disc(&mut img, *x, *y, *r, *c);
    }

    // Central star — four arms made from short fills so it reads as a
    // sparkle rather than a blob.
    let cx = CANVAS_W as i32 / 2;
    let cy = CANVAS_H as i32 / 2;
    let arm = 6i32;
    for k in -arm..=arm {
        put(&mut img, cx + k, cy, gold);
        put(&mut img, cx, cy + k, gold);
    }
    fill_disc(&mut img, cx, cy, 2, Rgba([255, 245, 200, 255]));

    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celebration_image_caches() {
        let a = celebration_terminal_image(TerminalImageProtocol::Kitty).unwrap();
        let b = celebration_terminal_image(TerminalImageProtocol::Kitty).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn celebration_image_has_expected_display_size() {
        let data = celebration_terminal_image(TerminalImageProtocol::Kitty).unwrap();
        assert_eq!(data.display_cols, CELEBRATION_DISPLAY_COLS);
        assert_eq!(data.display_rows, CELEBRATION_DISPLAY_ROWS);
    }

    #[test]
    fn celebration_image_sixel_only_for_sixel_protocol() {
        let kitty = celebration_terminal_image(TerminalImageProtocol::Kitty).unwrap();
        assert!(kitty.sixel_bytes.is_none());

        let sixel = celebration_terminal_image(TerminalImageProtocol::Sixel).unwrap();
        assert!(sixel.sixel_bytes.is_some());
    }
}
