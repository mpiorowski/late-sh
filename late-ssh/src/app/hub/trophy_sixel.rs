//! Pixel trophies for the top three rows of each ranked leaderboard panel.
//!
//! On capable terminals (Sixel / Kitty / iTerm2) a tiny gold / silver /
//! bronze trophy is overlaid on the leading prefix cells of ranks 1–3 in
//! the chips and arcade panels. Anywhere else the rows render exactly as
//! they do today — the trophy slot just stays as the existing prefix
//! ("   " / " › ") with no extra glyphs and no errors.
//!
//! Drawn procedurally with `image::RgbaImage`, PNG-encoded once per
//! `(rank, protocol)`, then routed through the shared
//! `terminal_image::terminal_image_from_bytes` so the exact same wipe
//! and dedupe machinery that backs every other terminal-image surface
//! also paints these.

use std::{
    collections::HashMap,
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Result;
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};

use crate::app::files::terminal_image::{
    TerminalImageData, TerminalImageProtocol, terminal_image_from_bytes,
};

/// Trophy display footprint. 3 columns wide is enough to read as a cup
/// silhouette while still fitting inside the existing prefix gutter
/// (" › " is 3 cells). 1 row tall keeps it from spilling into the next
/// leaderboard row.
pub(crate) const TROPHY_DISPLAY_COLS: u16 = 3;
pub(crate) const TROPHY_DISPLAY_ROWS: u16 = 1;

const CANVAS_W: u32 = (TROPHY_DISPLAY_COLS as u32) * 8;
const CANVAS_H: u32 = (TROPHY_DISPLAY_ROWS as u32) * 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TrophyTier {
    Gold,
    Silver,
    Bronze,
}

impl TrophyTier {
    pub(crate) fn from_rank(rank: i64) -> Option<Self> {
        match rank {
            1 => Some(TrophyTier::Gold),
            2 => Some(TrophyTier::Silver),
            3 => Some(TrophyTier::Bronze),
            _ => None,
        }
    }
}

type TrophyCacheKey = (TrophyTier, TerminalImageProtocol);

fn cache() -> &'static Mutex<HashMap<TrophyCacheKey, Arc<TerminalImageData>>> {
    static CACHE: OnceLock<Mutex<HashMap<TrophyCacheKey, Arc<TerminalImageData>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn trophy_terminal_image(
    tier: TrophyTier,
    protocol: TerminalImageProtocol,
) -> Result<Arc<TerminalImageData>> {
    if let Some(cached) = cache()
        .lock()
        .expect("trophy cache mutex poisoned")
        .get(&(tier, protocol))
    {
        return Ok(cached.clone());
    }
    let rgba = draw_trophy_rgba(tier);
    let png = png_encode_rgba(&rgba)?;
    let data = terminal_image_from_bytes(
        &png,
        u32::from(TROPHY_DISPLAY_COLS),
        u32::from(TROPHY_DISPLAY_ROWS),
        protocol,
    )?;
    let arc = Arc::new(data);
    cache()
        .lock()
        .expect("trophy cache mutex poisoned")
        .insert((tier, protocol), arc.clone());
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

/// Cup + handles + base + pedestal silhouette. The colour palette varies
/// by tier; the geometry does not.
fn draw_trophy_rgba(tier: TrophyTier) -> RgbaImage {
    let transparent = Rgba([0, 0, 0, 0]);
    let mut img = RgbaImage::from_pixel(CANVAS_W, CANVAS_H, transparent);

    let (body, rim, shadow) = palette(tier);

    // Bowl of the cup — rectangle centered in the canvas, slightly above
    // the vertical midline so there's room for the base below.
    let bowl_w = 12i32;
    let bowl_h = 6i32;
    let bowl_x = (CANVAS_W as i32 - bowl_w) / 2;
    let bowl_y = 1;
    fill_rect(&mut img, bowl_x, bowl_y, bowl_w, bowl_h, body);
    // Rim band — top two rows in the lighter accent.
    fill_rect(&mut img, bowl_x, bowl_y, bowl_w, 2, rim);

    // Side handles — two small flares left and right of the bowl.
    for dy in 1..5 {
        put(&mut img, bowl_x - 1, bowl_y + dy, body);
        put(&mut img, bowl_x + bowl_w, bowl_y + dy, body);
    }
    for dy in 1..3 {
        put(&mut img, bowl_x - 2, bowl_y + dy, body);
        put(&mut img, bowl_x + bowl_w + 1, bowl_y + dy, body);
    }

    // Stem connecting bowl to base.
    let stem_x = (CANVAS_W as i32 - 2) / 2;
    fill_rect(&mut img, stem_x, bowl_y + bowl_h, 2, 3, shadow);

    // Base — a wider plate.
    let base_y = bowl_y + bowl_h + 3;
    let base_w = bowl_w - 2;
    let base_x = (CANVAS_W as i32 - base_w) / 2;
    fill_rect(&mut img, base_x, base_y, base_w, 2, body);
    // Base shadow row just below.
    fill_rect(&mut img, base_x + 1, base_y + 2, base_w - 2, 1, shadow);

    img
}

fn palette(tier: TrophyTier) -> (Rgba<u8>, Rgba<u8>, Rgba<u8>) {
    match tier {
        TrophyTier::Gold => (
            Rgba([212, 175, 55, 255]),
            Rgba([255, 230, 130, 255]),
            Rgba([138, 100, 18, 255]),
        ),
        TrophyTier::Silver => (
            Rgba([180, 188, 200, 255]),
            Rgba([232, 236, 244, 255]),
            Rgba([110, 118, 128, 255]),
        ),
        TrophyTier::Bronze => (
            Rgba([176, 110, 67, 255]),
            Rgba([224, 168, 110, 255]),
            Rgba([102, 60, 30, 255]),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_from_rank_covers_top_three_only() {
        assert_eq!(TrophyTier::from_rank(1), Some(TrophyTier::Gold));
        assert_eq!(TrophyTier::from_rank(2), Some(TrophyTier::Silver));
        assert_eq!(TrophyTier::from_rank(3), Some(TrophyTier::Bronze));
        assert_eq!(TrophyTier::from_rank(0), None);
        assert_eq!(TrophyTier::from_rank(4), None);
        assert_eq!(TrophyTier::from_rank(99), None);
    }

    #[test]
    fn trophy_image_caches_per_tier_and_protocol() {
        let a = trophy_terminal_image(TrophyTier::Gold, TerminalImageProtocol::Kitty).unwrap();
        let b = trophy_terminal_image(TrophyTier::Gold, TerminalImageProtocol::Kitty).unwrap();
        assert!(Arc::ptr_eq(&a, &b));

        let c = trophy_terminal_image(TrophyTier::Silver, TerminalImageProtocol::Kitty).unwrap();
        assert!(!Arc::ptr_eq(&a, &c));
    }

    #[test]
    fn trophy_image_has_expected_display_size() {
        let data = trophy_terminal_image(TrophyTier::Gold, TerminalImageProtocol::Kitty).unwrap();
        assert_eq!(data.display_cols, TROPHY_DISPLAY_COLS);
        assert_eq!(data.display_rows, TROPHY_DISPLAY_ROWS);
    }

    #[test]
    fn trophy_image_sixel_only_for_sixel_protocol() {
        let kitty = trophy_terminal_image(TrophyTier::Gold, TerminalImageProtocol::Kitty).unwrap();
        assert!(kitty.sixel_bytes.is_none());

        let sixel =
            trophy_terminal_image(TrophyTier::Bronze, TerminalImageProtocol::Sixel).unwrap();
        assert!(sixel.sixel_bytes.is_some());
    }

    #[test]
    fn drawn_trophy_has_non_transparent_pixels() {
        let img = draw_trophy_rgba(TrophyTier::Gold);
        let non_transparent = img.pixels().filter(|p| p.0[3] > 0).count();
        assert!(
            non_transparent > 20,
            "trophy should have meaningful pixel coverage, got {non_transparent}"
        );
    }

    #[test]
    fn palettes_differ_between_tiers() {
        let g = palette(TrophyTier::Gold);
        let s = palette(TrophyTier::Silver);
        let b = palette(TrophyTier::Bronze);
        assert_ne!(g.0, s.0);
        assert_ne!(s.0, b.0);
        assert_ne!(g.0, b.0);
    }
}
