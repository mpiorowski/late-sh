use late_core::models::chips::Difficulty;
use ratatui::layout::Rect;

use crate::app::arcade::sliding_puzzle::state::board_len;

use super::*;

#[test]
fn hit_test_maps_every_tile_rect_to_its_board_index() {
    let area = Rect::new(0, 0, 80, 40);

    for view in [TileView::Numbered, TileView::Image] {
        for &difficulty in Difficulty::ALL {
            let (grid, geometry) = hit_layout(area, difficulty, view).expect("board fits");
            let dimension = board_dimension(difficulty);

            assert_eq!(grid.width, dimension as u16 * geometry.width);
            for index in 0..board_len(difficulty) {
                let row = index / dimension;
                let column = index % dimension;
                let x = grid.x + column as u16 * geometry.width + geometry.width / 2;
                let y = grid.y + row as u16 * geometry.height + geometry.height / 2;
                assert_eq!(hit_test(area, difficulty, view, x, y), Some(index));
            }
        }
    }
}

#[test]
fn hit_test_rejects_points_outside_the_board_and_undersized_areas() {
    let area = Rect::new(0, 0, 80, 40);
    assert_eq!(
        hit_test(area, Difficulty::Easy, TileView::Numbered, 0, 0),
        None
    );

    let too_small = Rect::new(0, 0, 20, 8);
    assert_eq!(
        hit_layout(too_small, Difficulty::Easy, TileView::Numbered),
        None
    );
    assert_eq!(
        hit_test(too_small, Difficulty::Easy, TileView::Numbered, 10, 4),
        None
    );
}

#[test]
fn native_tiles_must_match_the_current_grid_footprint() {
    assert!(native_tiles_fit_grid(
        ImageTileGeometry {
            width: 12,
            height: 6,
        },
        Rect::new(10, 3, 48, 24),
        4,
    ));
    assert!(!native_tiles_fit_grid(
        ImageTileGeometry {
            width: 12,
            height: 6,
        },
        Rect::new(10, 3, 36, 18),
        4,
    ));
}

#[test]
fn native_grid_placement_tracks_available_content_width() {
    let without_sidebar = grid_layout(
        game_content_area(Rect::new(1, 1, 118, 28), true, true),
        Difficulty::Medium,
        TileView::Image,
    )
    .expect("wide native placement")
    .0;
    let with_sidebar = grid_layout(
        game_content_area(Rect::new(1, 1, 94, 28), true, true),
        Difficulty::Medium,
        TileView::Image,
    )
    .expect("sidebar native placement")
    .0;

    assert_eq!(without_sidebar.width, with_sidebar.width);
    assert_ne!(without_sidebar.x, with_sidebar.x);
}
