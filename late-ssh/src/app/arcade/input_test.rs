use super::*;

#[test]
fn lobby_navigation_follows_rendered_order() {
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_2048),
        GAME_SELECTION_TETRIS
    );
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_TETRIS),
        GAME_SELECTION_SNAKE
    );
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_SNAKE),
        GAME_SELECTION_TRAFFIC
    );
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_TRAFFIC),
        GAME_SELECTION_LE_WORD
    );
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_LE_WORD),
        GAME_SELECTION_RUBIKS_CUBE
    );
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_RUBIKS_CUBE),
        GAME_SELECTION_SLIDING_PUZZLE
    );
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_SLIDING_PUZZLE),
        GAME_SELECTION_SUDOKU
    );
    assert_eq!(
        prev_lobby_selection(GAME_SELECTION_SUDOKU),
        GAME_SELECTION_SLIDING_PUZZLE
    );
}

#[test]
fn lobby_navigation_wraps_in_rendered_order() {
    assert_eq!(
        next_lobby_selection(GAME_SELECTION_SOLITAIRE),
        GAME_SELECTION_2048
    );
    assert_eq!(
        prev_lobby_selection(GAME_SELECTION_2048),
        GAME_SELECTION_SOLITAIRE
    );
}

#[tokio::test]
async fn sliding_puzzle_q_and_escape_return_to_arcade_lobby() {
    use crate::test_helpers::{make_app, new_test_db};
    use late_core::test_utils::create_test_user;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-exit-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "sliding-puzzle-exit-token");
    app.set_screen(Screen::Arcade);
    app.game_selection = GAME_SELECTION_SLIDING_PUZZLE;

    for key in [b'q', 0x1B] {
        app.is_playing_game = true;
        assert!(handle_key(&mut app, key));
        assert!(!app.is_playing_game);
        assert_eq!(app.screen, Screen::Arcade);
    }
}

#[tokio::test]
async fn sliding_puzzle_left_click_moves_an_adjacent_tile_into_the_gap() {
    use crate::{
        app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
        test_helpers::{make_app, new_test_db},
    };
    use late_core::{models::chips::Difficulty, test_utils::create_test_user};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-click-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "sliding-puzzle-click-token");
    app.set_screen(Screen::Arcade);
    app.game_selection = GAME_SELECTION_SLIDING_PUZZLE;
    app.is_playing_game = true;
    app.sliding_puzzle_state.set_board_for_test(
        Difficulty::Easy,
        vec![1, 2, 3, 4, 0, 5, 6, 7, 8],
        0,
    );

    let area = arcade_content_area(&app);
    let difficulty = app.sliding_puzzle_state.difficulty();
    let (x, y) = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .find(|&(x, y)| {
            crate::app::arcade::sliding_puzzle::ui::hit_test(
                area,
                difficulty,
                crate::app::arcade::sliding_puzzle::image::TileView::Numbered,
                x,
                y,
            ) == Some(7)
        })
        .expect("clickable tile");
    let event = ParsedInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down,
        button: Some(MouseButton::Left),
        x: x + 1,
        y: y + 1,
        modifiers: Default::default(),
    });

    assert!(handle_event(&mut app, &event));
    assert_eq!(
        app.sliding_puzzle_state.board(),
        &[1, 2, 3, 4, 7, 5, 6, 0, 8]
    );
    assert_eq!(app.sliding_puzzle_state.moves(), 1);
}
