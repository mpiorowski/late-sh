use chrono::Utc;
use late_core::db::{Db, DbConfig};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::*;
use crate::app::{
    activity::event::ActivityEvent,
    arcade::quartets::{
        state::find_puzzle,
        svc::{Daily, QuartetsService, Today},
    },
};

/// A click lands on the tile the frame drew, counted from the first row
/// below the quartets found; a gap between tiles, a found quartet's bar
/// and the space past the wall are no tile.
#[test]
fn a_click_lands_on_the_tile_drawn() {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    let puzzle = find_puzzle(1).expect("puzzle 1 in the pack");
    let mut state = State::new(
        Uuid::now_v7(),
        QuartetsService::new(db, activity),
        Some(Today {
            daily: Daily {
                puzzle_date: Utc::now().date_naive(),
                puzzle,
            },
            saved: None,
        }),
    );
    let area = Rect::new(0, 0, 100, 40);
    let layout = layout(
        game_content_area(area, true, SHOW_GAME_BOTTOM_BAR),
        tile_width(puzzle),
    );
    let pitch = layout.tile_width + TILE_GAP;
    let at = |state: &State, column: u16, row: u16| {
        hit_test(area, state, layout.wall.x + column, layout.wall.y + row)
    };
    assert_eq!(
        [
            at(&state, 0, 0),
            at(&state, pitch - 1, 2),
            at(&state, pitch, 1),
            at(&state, 3 * pitch, 12),
            at(&state, 0, 3),
            at(&state, 4 * pitch, 0),
        ],
        [Some(0), None, Some(1), Some(15), None, None]
    );

    for word in [0, 1, 2, 3] {
        let position = state
            .wall()
            .iter()
            .position(|on_wall| *on_wall == word)
            .unwrap();
        state.click(position);
    }
    state.submit();
    assert_eq!(
        [
            at(&state, 0, 0),
            at(&state, 0, 4),
            at(&state, 3 * pitch, 12)
        ],
        [None, Some(0), Some(11)]
    );
}

/// The wall and its status line sit mid-board as one group.
#[test]
fn the_wall_sits_mid_board() {
    let board = Rect::new(0, 2, 100, 48);
    let layout = layout(board, MAX_TILE_WIDTH);
    let (above, below) = (
        layout.wall.y - board.y,
        board.bottom() - layout.status.bottom(),
    );
    assert!(above.abs_diff(below) <= 1, "{above} above, {below} below");
}
