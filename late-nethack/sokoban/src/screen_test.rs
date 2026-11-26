use crate::screen::{CellSize, Grid, PpmError, cursor, parse_ppm};

const CELL: CellSize = CellSize { width: 13.0, height: 28.0 };

/// A dark 200x120 PPM with an orange 13x28 block at (20, 10), a blue
/// pile-highlight block at (60, 40), a white bar
/// eight cells wide at (40, 90) whiter than the cursor (the curses name
/// field), a white blob shaped like an emoji at (150, 12) and, if given, a
/// 13x28 cursor block with a dark glyph inside at `white`.
fn screenshot(white: Option<(usize, usize)>) -> Vec<u8> {
    let (w, h) = (200, 120);
    let mut px = vec![[20u8, 20, 30]; w * h];
    let mut rect = |x0: usize, y0: usize, rw: usize, rh: usize, c: [u8; 3]| {
        for y in y0..y0 + rh {
            for x in x0..x0 + rw {
                px[y * w + x] = c;
            }
        }
    };
    rect(20, 10, 13, 28, [255, 165, 0]);
    rect(60, 40, 13, 28, [137, 180, 250]);
    rect(40, 90, 13 * 8, 28, [240, 240, 240]);
    for y in 16..34 {
        let inset = (34 - y) / 3;
        rect(150 + inset, y, 15 - 2 * inset, 1, [250, 250, 250]);
    }
    if let Some((x, y)) = white {
        rect(x, y, 13, 28, [190, 195, 220]);
        rect(x + 3, y + 8, 7, 14, [31, 31, 40]);
    }
    let mut out = format!("P6\n# grim\n{w} {h}\n255\n").into_bytes();
    out.extend(px.iter().flatten());
    out
}

#[test]
fn finds_the_cursor_not_a_colored_block_a_pile_or_a_whiter_bar() {
    let img = parse_ppm(&screenshot(Some((104, 40)))).unwrap();
    let (x, y) = cursor(&img, CELL).unwrap();
    assert!((x - 110.5).abs() <= 1.5 && (y - 54.0).abs() <= 2.0, "{x} {y}");
}

#[test]
fn a_bar_alone_is_no_cursor() {
    let img = parse_ppm(&screenshot(None)).unwrap();
    assert_eq!(cursor(&img, CELL), None);
}

#[test]
fn a_calibrated_grid_maps_the_cursor_to_map_cells() {
    let grid = Grid::calibrate(CELL, (110.5, 80.0), (7, 2));
    assert_eq!(grid.cell_at((110.5, 80.0)), (7, 2));
    assert_eq!(grid.cell_at((110.5 + 13.0 * 3.0 + 1.0, 80.0 - 28.0)), (10, 1));
    assert_eq!(grid.cell_at((110.5 - 13.0 * 8.0, 80.0 - 28.0 * 3.0)), (-1, -1));
}

#[test]
fn rejects_what_grim_would_not_write() {
    assert_eq!(parse_ppm(b"P3\n1 1\n255\n0 0 0").err(), Some(PpmError::NotBinaryPpm));
    assert_eq!(parse_ppm(b"P6\n1 x\n255\n").err(), Some(PpmError::BadHeader));
    assert_eq!(parse_ppm(b"P6\n2 2\n255\n\0\0\0").err(), Some(PpmError::Truncated));
}
