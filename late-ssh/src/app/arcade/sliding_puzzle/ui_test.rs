use late_core::models::chips::Difficulty;
use ratatui::layout::Rect;

use crate::app::arcade::sliding_puzzle::state::board_len;

use super::*;

const ART: TileGeometry = TileGeometry {
    width: 10,
    height: 4,
};

#[test]
fn hit_test_maps_every_tile_rect_to_its_board_index() {
    let area = Rect::new(0, 0, 80, 40);

    for art in [None, Some(ART)] {
        for &difficulty in Difficulty::ALL {
            let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
            let (grid, geometry) =
                board_layout(board_area, difficulty, art).expect("board fits");
            assert_eq!(art.unwrap_or(NUMBERED_TILE_GEOMETRY), geometry);
            let dimension = board_dimension(difficulty);

            assert_eq!(grid.width, dimension as u16 * geometry.width);
            for index in 0..board_len(difficulty) {
                let row = index / dimension;
                let column = index % dimension;
                let x = grid.x + column as u16 * geometry.width + geometry.width / 2;
                let y = grid.y + row as u16 * geometry.height + geometry.height / 2;
                assert_eq!(hit_test(area, difficulty, art, x, y), Some(index));
            }
        }
    }
}

#[test]
fn hit_test_rejects_points_outside_the_board_and_undersized_areas() {
    let area = Rect::new(0, 0, 80, 40);
    assert_eq!(hit_test(area, Difficulty::Easy, None, 0, 0), None);

    let too_small = Rect::new(0, 0, 20, 8);
    assert_eq!(
        board_layout(
            game_content_area(too_small, true, SHOW_GAME_BOTTOM_BAR),
            Difficulty::Easy,
            None
        ),
        None
    );
    assert_eq!(hit_test(too_small, Difficulty::Easy, None, 10, 4), None);
}

/// Art too wide for the board falls back to the numbered grid rather than
/// cropping the piece, and the click map follows the fallback.
#[test]
fn art_that_does_not_fit_falls_back_to_the_numbered_grid() {
    let board_area = Rect::new(0, 0, 60, 20);
    let huge = TileGeometry {
        width: 40,
        height: 14,
    };
    let (grid, geometry) =
        board_layout(board_area, Difficulty::Easy, Some(huge)).expect("numbered fallback");
    assert_eq!(geometry, NUMBERED_TILE_GEOMETRY);
    assert_eq!(grid.width, 3 * NUMBERED_TILE_GEOMETRY.width);

    let (grid, geometry) =
        board_layout(board_area, Difficulty::Easy, Some(ART)).expect("art fits");
    assert_eq!(geometry, ART);
    assert_eq!(grid.width, 3 * ART.width);
}
