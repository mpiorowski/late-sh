use chrono::NaiveDate;
use dartboard_core::{Canvas, Pos};
use late_core::{
    db::{Db, DbConfig},
    models::chips::Difficulty,
};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    activity::event::ActivityEvent,
    arcade::sliding_puzzle::{
        art::{PuzzleArt, art_grid, tile_fragment},
        state::board_len,
        svc::{ArtLoad, SlidingPuzzleService},
    },
};

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
            let (grid, geometry) = board_layout(board_area, difficulty, art).expect("board fits");
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

    let (grid, geometry) = board_layout(board_area, Difficulty::Easy, Some(ART)).expect("art fits");
    assert_eq!(geometry, ART);
    assert_eq!(grid.width, 3 * ART.width);
}

fn state_with_art(load: ArtLoad) -> State {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    let mut state = State::new_for_date(
        Uuid::now_v7(),
        SlidingPuzzleService::new(db, activity),
        NaiveDate::from_ymd_opt(2026, 9, 21).expect("date"),
        Vec::new(),
    );
    state.set_art_for_test(load);
    state
}

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

fn rendered(state: &State) -> String {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_game(frame, frame.area(), state, SHOW_GAME_BOTTOM_BAR))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

/// The art-status tips explain the numbered fallback, but a pending
/// two-press confirm is the one line the player must see: without it a
/// second `r` wipes an in-progress board unannounced.
#[test]
fn a_pending_confirm_shows_over_the_art_status_tip() {
    for load in [ArtLoad::Empty, ArtLoad::Failed] {
        let mut state = state_with_art(load);
        assert!(!state.request_reset());
        let text = rendered(&state);
        assert!(
            text.contains("Press r or 0 again to restore today's scramble."),
            "{text}"
        );
    }
}

/// The number lands on a wide glyph or its continuation cell: the row keeps
/// its width instead of shifting the rest of the tile.
#[test]
fn the_tile_number_keeps_the_row_width_over_wide_glyphs() {
    // An 18x9 piece cuts into 6x3 tiles on the easy board; tile 1's
    // number sits on row 1 at column 2.
    for emoji_x in [1, 2] {
        let art = piece(18, 9, |canvas| {
            canvas.set(Pos { x: emoji_x, y: 1 }, '🙂');
        });
        let grid = art_grid(&art, Difficulty::Easy);
        let mut fragment = tile_fragment(&grid, Difficulty::Easy, 1).expect("tile 1");
        add_art_tile_number(&mut fragment, 1, grid.geometry);
        let widths: Vec<usize> = fragment
            .iter()
            .map(|line| line.spans.iter().map(|span| span.width()).sum())
            .collect();
        assert_eq!(widths, vec![6, 6, 6], "emoji at x={emoji_x}: {fragment:?}");
        let middle: String = fragment[1]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(middle.contains('1'), "emoji at x={emoji_x}: {middle:?}");
    }
}
