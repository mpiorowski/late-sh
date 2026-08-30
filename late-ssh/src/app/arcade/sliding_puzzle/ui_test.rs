use late_core::models::chips::Difficulty;
use ratatui::layout::Rect;

use crate::app::arcade::sliding_puzzle::state::board_len;

use super::*;

#[test]
fn hit_test_maps_every_tile_rect_to_its_board_index() {
    let area = Rect::new(0, 0, 80, 40);

    for &difficulty in Difficulty::ALL {
        let grid = hit_area(area, difficulty).expect("board fits");
        let dimension = board_dimension(difficulty);

        for index in 0..board_len(difficulty) {
            let row = index / dimension;
            let column = index % dimension;
            let x = grid.x + column as u16 * TILE_WIDTH + TILE_WIDTH / 2;
            let y = grid.y + row as u16 * TILE_HEIGHT + TILE_HEIGHT / 2;
            assert_eq!(hit_test(area, difficulty, x, y), Some(index));
        }
    }
}

#[test]
fn hit_test_rejects_points_outside_the_board_and_undersized_areas() {
    let area = Rect::new(0, 0, 80, 40);
    assert_eq!(hit_test(area, Difficulty::Easy, 0, 0), None);

    let too_small = Rect::new(0, 0, 20, 8);
    assert_eq!(hit_area(too_small, Difficulty::Easy), None);
    assert_eq!(hit_test(too_small, Difficulty::Easy, 10, 4), None);
}
