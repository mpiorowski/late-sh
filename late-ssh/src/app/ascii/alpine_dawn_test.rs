//! Golden frame from ascii.rest's own `alpine-dawn.ts` (node, `frame(1)`),
//! halftoned at its native 200x100 the way the original draws it, recorded
//! with its nearest-colour cache removed (`aurora_fjord_test.rs` says why).

use super::{COLS, PALETTE, ROWS, dot, frame, nearest};

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
    assert_eq!(text, include_str!("fixtures/alpine_dawn_t1.txt"));
    assert_eq!(
        colors.as_slice(),
        include_bytes!("fixtures/alpine_dawn_t1.colors").as_slice()
    );
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
