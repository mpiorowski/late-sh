use dartboard_core::{Canvas, Pos, RgbColor};
use late_core::models::chips::Difficulty;
use ratatui::style::Color;

use super::art::{
    ArtGrid, MIN_ART_TILE_GEOMETRY, PuzzleArt, TileGeometry, art_grid, tile_fragment,
};

fn piece(width: usize, height: usize, paint: impl FnOnce(&mut Canvas)) -> PuzzleArt {
    let mut canvas = Canvas::with_size(width, height);
    paint(&mut canvas);
    PuzzleArt {
        title: "sunset".to_string(),
        username: "painter".to_string(),
        canvas,
        width,
        height,
    }
}

fn text(grid: &ArtGrid) -> Vec<String> {
    grid.lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn a_small_piece_is_padded_to_the_minimum_tile_and_centred() {
    let art = piece(12, 4, |canvas| {
        canvas.set(Pos { x: 0, y: 0 }, '#');
        canvas.set_colored(Pos { x: 11, y: 3 }, '@', RgbColor { r: 1, g: 2, b: 3 });
    });
    let grid = art_grid(&art, Difficulty::Easy);

    assert_eq!(grid.geometry, MIN_ART_TILE_GEOMETRY);
    let rows = text(&grid);
    assert_eq!(rows.len(), 9);
    assert!(rows.iter().all(|row| row.chars().count() == 18), "{rows:?}");
    assert_eq!(rows[2], "   #              ");
    assert_eq!(rows[5], "              @   ");
    assert_eq!(grid.lines[5].spans[14].style.fg, Some(Color::Rgb(1, 2, 3)));

    // Tile 1 is the top-left cut and carries the `#`; the gap has no art.
    let first = tile_fragment(&grid, Difficulty::Easy, 1).expect("tile 1");
    assert_eq!(
        first
            .iter()
            .map(|line| line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>())
            .collect::<Vec<_>>(),
        vec!["      ", "      ", "   #  "]
    );
    assert!(tile_fragment(&grid, Difficulty::Easy, 0).is_none());
    assert!(tile_fragment(&grid, Difficulty::Easy, 9).is_none());
}

#[test]
fn a_large_piece_sets_the_tile_size_and_wide_glyphs_never_straddle_a_cut() {
    let art = piece(120, 40, |canvas| {
        // Origin on a tile's last column: torn in two by the cut.
        canvas.set(Pos { x: 23, y: 0 }, '🙂');
        // Origin inside a tile: kept, with its continuation cell.
        canvas.set(Pos { x: 0, y: 1 }, '🙂');
    });
    let grid = art_grid(&art, Difficulty::Hard);

    assert_eq!(
        grid.geometry,
        TileGeometry {
            width: 24,
            height: 8,
        }
    );
    assert_eq!(grid.lines.len(), 40);
    assert!(grid.lines.iter().all(|line| line.spans.len() == 120));
    let rows = text(&grid);
    assert_eq!(&rows[0][..25], "                         ");
    assert!(rows[1].starts_with("🙂 "), "{:?}", rows[1]);
    assert_eq!(grid.lines[1].spans[1].content.as_ref(), "");
}
