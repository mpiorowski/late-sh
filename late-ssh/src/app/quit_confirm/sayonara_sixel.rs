//! Small pixel sayonara scene rendered in the quit-confirm modal on
//! capable terminals. A horizon sunset with the late.sh figure waving
//! goodbye — purely flavour, no business logic, no persistent state.
//! Non-capable terminals see only the existing "Clicked by mistake,
//! right?" prompt and the q / Esc footer.
//!
//! Drawn procedurally with `image::RgbaImage`, PNG-encoded once per
//! protocol, then routed through the shared
//! `terminal_image::terminal_image_from_bytes` so the exact same wipe
//! and dedupe machinery that backs every other terminal-image surface
//! also paints this.

use std::{
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Result;
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};

use crate::app::files::terminal_image::{
    TerminalImageData, TerminalImageProtocol, terminal_image_from_bytes,
};

/// Display footprint — the quit-confirm modal is 60×9 cells with one
/// prompt row + one footer row + a flexible spacer in between, so a
/// 40×4 scene anchors comfortably in the middle without crowding either
/// the question or the two key hints.
pub(crate) const SAYONARA_DISPLAY_COLS: u16 = 40;
pub(crate) const SAYONARA_DISPLAY_ROWS: u16 = 4;

const CANVAS_W: u32 = (SAYONARA_DISPLAY_COLS as u32) * 8;
const CANVAS_H: u32 = (SAYONARA_DISPLAY_ROWS as u32) * 16;

type SayonaraCache = Option<(TerminalImageProtocol, Arc<TerminalImageData>)>;

fn cache() -> &'static Mutex<SayonaraCache> {
    static CACHE: OnceLock<Mutex<SayonaraCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn sayonara_terminal_image(
    protocol: TerminalImageProtocol,
) -> Result<Arc<TerminalImageData>> {
    {
        let guard = cache().lock().expect("sayonara cache mutex poisoned");
        if let Some((cached_protocol, data)) = guard.as_ref()
            && *cached_protocol == protocol
        {
            return Ok(data.clone());
        }
    }
    let rgba = draw_sayonara_rgba();
    let png = png_encode_rgba(&rgba)?;
    let data = terminal_image_from_bytes(
        &png,
        u32::from(SAYONARA_DISPLAY_COLS),
        u32::from(SAYONARA_DISPLAY_ROWS),
        protocol,
    )?;
    let arc = Arc::new(data);
    *cache().lock().expect("sayonara cache mutex poisoned") = Some((protocol, arc.clone()));
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

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>) {
    for dy in 0..h {
        for dx in 0..w {
            put(img, x + dx, y + dy, c);
        }
    }
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

/// Horizon scene: dusk sky gradient, low sun, water band with shimmer,
/// and a small waving figure on the right. Deterministic — every render
/// produces the same bytes so the cache hashes consistently.
fn draw_sayonara_rgba() -> RgbaImage {
    let transparent = Rgba([0, 0, 0, 0]);
    let mut img = RgbaImage::from_pixel(CANVAS_W, CANVAS_H, transparent);

    let sky_top = Rgba([72, 56, 110, 255]);
    let sky_mid = Rgba([200, 110, 96, 255]);
    let sky_low = Rgba([240, 168, 110, 255]);
    let sun = Rgba([255, 220, 130, 255]);
    let sun_glow = Rgba([255, 190, 110, 255]);
    let water = Rgba([72, 96, 130, 255]);
    let water_shimmer = Rgba([232, 220, 200, 255]);
    let silhouette = Rgba([24, 28, 40, 255]);

    let canvas_w = CANVAS_W as i32;
    let canvas_h = CANVAS_H as i32;

    // Three-band sky gradient — top third deep dusk, middle warm rose,
    // lower third amber blending into the water band.
    let band_h = canvas_h / 3;
    fill_rect(&mut img, 0, 0, canvas_w, band_h, sky_top);
    fill_rect(&mut img, 0, band_h, canvas_w, band_h, sky_mid);
    fill_rect(&mut img, 0, band_h * 2, canvas_w, band_h, sky_low);

    // Low sun centered horizontally, sitting on the horizon line.
    let horizon_y = canvas_h * 3 / 4;
    let sun_cx = canvas_w / 2 - canvas_w / 10;
    let sun_cy = horizon_y - 2;
    fill_disc(&mut img, sun_cx, sun_cy, 10, sun_glow);
    fill_disc(&mut img, sun_cx, sun_cy, 7, sun);

    // Water band — last quarter of the canvas with a couple of shimmer
    // stripes for movement.
    fill_rect(
        &mut img,
        0,
        horizon_y,
        canvas_w,
        canvas_h - horizon_y,
        water,
    );
    for stripe in [horizon_y + 4, horizon_y + 10] {
        for x in (10..canvas_w - 10).step_by(6) {
            put(&mut img, x, stripe, water_shimmer);
            put(&mut img, x + 1, stripe, water_shimmer);
        }
    }

    // Sun reflection — a dim vertical column from the horizon down to
    // the bottom of the canvas, narrower than the sun itself.
    for y in horizon_y..canvas_h {
        for dx in -3..=3 {
            put(&mut img, sun_cx + dx, y, sun_glow);
        }
    }

    // Waving figure silhouette — head, body, two arms with the right
    // arm raised. Anchored on the right third so it doesn't cover the
    // sun.
    let figure_x = canvas_w - 18;
    let figure_top = horizon_y - 18;
    // Head.
    fill_disc(&mut img, figure_x, figure_top, 3, silhouette);
    // Body (slim trapezoid).
    fill_rect(&mut img, figure_x - 2, figure_top + 3, 5, 9, silhouette);
    // Left arm down.
    fill_rect(&mut img, figure_x - 4, figure_top + 4, 2, 6, silhouette);
    // Right arm raised — wave.
    fill_rect(&mut img, figure_x + 3, figure_top - 1, 2, 6, silhouette);
    fill_rect(&mut img, figure_x + 5, figure_top - 3, 2, 3, silhouette);

    img
}

#[cfg(test)]
#[path = "sayonara_sixel_test.rs"]
mod sayonara_sixel_test;
