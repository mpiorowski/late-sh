//! Golden frame from ascii.rest's own `aurora-fjord.ts` (node, `frame(1)`),
//! halftoned at its native 200x100 the way the original draws it. One edit
//! to the original before recording: its nearest-colour lookup caches by
//! 5-bit colour bucket and keeps whichever colour asked first, so its
//! colours depend on draw order; the fixture was recorded with that cache
//! removed, which is the exact lookup this port does. The dots are the same
//! with or without the cache.

use super::{COLS, GROUND, PALETTE, ROWS, dot, frame, nearest, pixel};
use crate::app::ascii::piece::Shade;

#[test]
fn frame_matches_ascii_rest_dot_for_dot_and_colour_for_colour() {
    let scene = frame(1.0);
    assert_eq!((scene.cols, scene.rows), (COLS, ROWS));
    let mut text = String::new();
    let mut colors = Vec::with_capacity(COLS * ROWS);
    for row in 0..ROWS {
        for col in 0..COLS {
            let (glyph, color) = dot(scene.cells[row * COLS + col], col, row);
            text.push(glyph);
            colors.push(color);
        }
        text.push('\n');
    }
    assert_eq!(text, include_str!("fixtures/aurora_fjord_t1.txt"));
    assert_eq!(
        colors.as_slice(),
        include_bytes!("fixtures/aurora_fjord_t1.colors").as_slice()
    );
}

/// A pixel is the dot's colour spread over its cell: no dot is bare ground,
/// the largest dot is the full palette colour, whatever the dither says.
#[test]
fn a_pixel_spreads_the_dot_over_its_cell() {
    let dark = Shade {
        level: 0.0,
        rgb: [0.2, 0.5, 0.3],
    };
    let white = Shade {
        level: 1.0,
        rgb: [1.0, 1.0, 1.0],
    };
    for (x, y) in [(0, 0), (1, 2), (3, 3), (2, 1)] {
        assert_eq!(pixel(dark, x, y), GROUND, "no dot at {x},{y}");
        let (_, index) = dot(white, x, y);
        assert_eq!(pixel(white, x, y), PALETTE[index as usize], "full dot at {x},{y}");
    }
}

#[test]
fn every_palette_colour_is_its_own_nearest() {
    for (index, [r, g, b]) in PALETTE.iter().enumerate() {
        let channel = |c: u8| f64::from(c) / 255.0;
        assert_eq!(
            nearest(channel(*r), channel(*g), channel(*b)) as usize,
            index
        );
    }
}
