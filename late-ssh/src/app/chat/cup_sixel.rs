//! Private pixel-cup celebration for the sender of `/coffee` and `/tea`.
//!
//! When the user runs one of the cup rituals their terminal gets a brief
//! sixel/Kitty/iTerm2 cup in the corner of the chat area; the chat ritual
//! itself still goes out as plain ASCII to every viewer. So others see what
//! was sent today and nothing else changes for them.
//!
//! The cup is drawn procedurally with `image::RgbaImage` (no bundled
//! assets), PNG-encoded once per `(kind, protocol)`, and then handed to
//! `crate::app::files::terminal_image::terminal_image_from_bytes` to get
//! back a `TerminalImageData` with the protocol-specific bytes the chat
//! render pipe already knows how to emit.

use std::{
    collections::HashMap,
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Result;
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};

use crate::app::chat::state::CupKind;
use crate::app::files::terminal_image::{
    TerminalImageData, TerminalImageProtocol, terminal_image_from_bytes,
};

/// Cells the cup occupies on screen. Width/height are small on purpose —
/// the cup is a corner treat, not a centerpiece.
pub(crate) const CUP_DISPLAY_COLS: u16 = 10;
pub(crate) const CUP_DISPLAY_ROWS: u16 = 6;

/// Pixel canvas the cup is drawn into before encoding. Mirrors the cell
/// aspect (8×16) used by `terminal_image::TERMINAL_IMAGE_CELL_PIXEL_*`, so
/// the encoded image scales to the requested cell grid cleanly.
const CANVAS_W: u32 = (CUP_DISPLAY_COLS as u32) * 8;
const CANVAS_H: u32 = (CUP_DISPLAY_ROWS as u32) * 16;

type CupCacheKey = (CupKind, TerminalImageProtocol);

/// Cache the encoded cup image per `(kind, protocol)` so repeated rituals
/// don't re-encode. Mutex guards a `HashMap` because the cache is shared
/// across SSH sessions and only ever holds a handful of entries.
fn cache() -> &'static Mutex<HashMap<CupCacheKey, Arc<TerminalImageData>>> {
    static CACHE: OnceLock<Mutex<HashMap<CupCacheKey, Arc<TerminalImageData>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn cup_terminal_image(
    kind: CupKind,
    protocol: TerminalImageProtocol,
) -> Result<Arc<TerminalImageData>> {
    if let Some(cached) = cache()
        .lock()
        .expect("cup cache mutex poisoned")
        .get(&(kind, protocol))
    {
        return Ok(cached.clone());
    }

    let rgba = draw_cup_rgba(kind);
    let png = png_encode_rgba(&rgba)?;
    let data = terminal_image_from_bytes(
        &png,
        u32::from(CUP_DISPLAY_COLS),
        u32::from(CUP_DISPLAY_ROWS),
        protocol,
    )?;
    let arc = Arc::new(data);
    cache()
        .lock()
        .expect("cup cache mutex poisoned")
        .insert((kind, protocol), arc.clone());
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

/// Solid-colour painter that ignores out-of-bounds pixels so the drawing
/// primitives can be sloppy without panicking near the canvas edges.
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

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>) {
    for dy in 0..h {
        for dx in 0..w {
            put(img, x + dx, y + dy, c);
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>) {
    for dx in 0..w {
        put(img, x + dx, y, c);
        put(img, x + dx, y + h - 1, c);
    }
    for dy in 0..h {
        put(img, x, y + dy, c);
        put(img, x + w - 1, y + dy, c);
    }
}

/// Draw a small mug or cup centered in the canvas. Steam squiggles ride
/// above; a saucer sits below; the body and handle are kind-specific.
fn draw_cup_rgba(kind: CupKind) -> RgbaImage {
    let transparent = Rgba([0, 0, 0, 0]);
    let mut img = RgbaImage::from_pixel(CANVAS_W, CANVAS_H, transparent);

    // The cup body sits roughly in the lower-middle third of the canvas
    // with room above it for steam and below it for the saucer.
    let cup_w = 32i32;
    let cup_h = 28i32;
    let cup_x = (CANVAS_W as i32 - cup_w) / 2;
    let cup_y = (CANVAS_H as i32 - cup_h) / 2 + 4;

    let (fill, rim) = match kind {
        CupKind::Coffee => (Rgba([99, 65, 41, 255]), Rgba([214, 173, 122, 255])),
        CupKind::Tea => (Rgba([194, 124, 79, 255]), Rgba([248, 220, 172, 255])),
    };
    let cup_outline = Rgba([223, 228, 238, 255]); // theme::TEXT — light frame
    let handle_color = cup_outline;
    let saucer_color = Rgba([138, 92, 53, 255]);
    let steam = Rgba([200, 208, 224, 230]);

    // Filled body (drink colour) inset by one pixel from the cup outline.
    fill_rect(&mut img, cup_x + 1, cup_y + 1, cup_w - 2, cup_h - 2, fill);
    // Rim band — top three rows in the lighter accent so the cup reads from
    // a distance even on a noisy chat background.
    fill_rect(&mut img, cup_x + 1, cup_y + 1, cup_w - 2, 3, rim);
    // Outline frame.
    stroke_rect(&mut img, cup_x, cup_y, cup_w, cup_h, cup_outline);

    match kind {
        CupKind::Coffee => {
            // Right-side handle: small oval, two pixels thick. The middle
            // rows of the handle bulge out further than the top/bottom so
            // it reads as a curve rather than a flat tab.
            let hx = cup_x + cup_w - 1;
            for dy in 0..18 {
                let y = cup_y + 5 + dy;
                let on_handle_curve = (4..=13).contains(&dy);
                let bulge = if on_handle_curve { 8 } else { 4 };
                for dx in 0..2 {
                    put(&mut img, hx + bulge + dx, y, handle_color);
                }
                put(&mut img, hx + 4, y, handle_color);
                if on_handle_curve {
                    put(&mut img, hx + 8, y, handle_color);
                }
            }
        }
        CupKind::Tea => {
            // No handle — tea is the handle-less cup in the ASCII art
            // (`\___/`). Instead add a saucer wider than the cup.
        }
    }

    // Saucer underneath both kinds.
    let saucer_y = cup_y + cup_h;
    fill_rect(&mut img, cup_x - 4, saucer_y, cup_w + 8, 3, saucer_color);

    // Steam squiggles above the cup — two columns offset.
    let steam_x_a = cup_x + cup_w / 3;
    let steam_x_b = cup_x + (cup_w * 2) / 3;
    for (sx, phase) in [(steam_x_a, 0i32), (steam_x_b, 3)] {
        for dy in 0..18 {
            let y = cup_y - 4 - dy;
            let wobble = (((dy + phase) % 6) - 3).abs();
            let x = sx + (wobble - 1);
            put(&mut img, x, y, steam);
        }
    }

    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cup_image_caches_per_kind_and_protocol() {
        let a = cup_terminal_image(CupKind::Coffee, TerminalImageProtocol::Kitty).unwrap();
        let b = cup_terminal_image(CupKind::Coffee, TerminalImageProtocol::Kitty).unwrap();
        // Same Arc — second lookup hits the cache.
        assert!(Arc::ptr_eq(&a, &b));

        let c = cup_terminal_image(CupKind::Tea, TerminalImageProtocol::Kitty).unwrap();
        assert!(!Arc::ptr_eq(&a, &c));
    }

    #[test]
    fn cup_image_has_expected_display_size() {
        let data = cup_terminal_image(CupKind::Coffee, TerminalImageProtocol::Kitty).unwrap();
        assert_eq!(data.display_cols, CUP_DISPLAY_COLS);
        assert_eq!(data.display_rows, CUP_DISPLAY_ROWS);
    }

    #[test]
    fn cup_image_sixel_only_for_sixel_protocol() {
        let kitty = cup_terminal_image(CupKind::Coffee, TerminalImageProtocol::Kitty).unwrap();
        assert!(kitty.sixel_bytes.is_none());

        let sixel = cup_terminal_image(CupKind::Tea, TerminalImageProtocol::Sixel).unwrap();
        assert!(sixel.sixel_bytes.is_some());
    }

    #[test]
    fn drawn_cup_has_non_transparent_pixels() {
        let img = draw_cup_rgba(CupKind::Coffee);
        let non_transparent = img.pixels().filter(|p| p.0[3] > 0).count();
        assert!(
            non_transparent > 100,
            "cup should have meaningful pixel coverage, got {non_transparent}"
        );
    }
}
