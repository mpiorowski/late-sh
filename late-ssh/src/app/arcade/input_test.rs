use super::*;

use late_core::test_utils::create_test_user;

use crate::test_helpers::{make_app, new_test_db};

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
        GAME_SELECTION_SUDOKU
    );
    assert_eq!(
        prev_lobby_selection(GAME_SELECTION_SUDOKU),
        GAME_SELECTION_RUBIKS_CUBE
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
async fn leaving_le_word_cancels_replay_reset_confirmation() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-reset-navigation").await;
    let mut app = make_app(test_db.db, user.id, "test-session");
    app.game_selection = GAME_SELECTION_LE_WORD;
    app.is_playing_game = true;
    app.le_word_state.reset_pending = true;
    app.le_word_state.message = "Press 0 again for a new random word.".to_string();

    assert!(handle_key(&mut app, 0x1B));

    assert!(!app.is_playing_game);
    assert!(!app.le_word_state.reset_pending);
    assert!(app.le_word_state.message.is_empty());
}

#[tokio::test]
async fn opening_help_cancels_le_word_replay_reset_confirmation() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-reset-help").await;
    let mut app = make_app(test_db.db, user.id, "test-session");
    app.game_selection = GAME_SELECTION_LE_WORD;
    app.is_playing_game = true;
    app.le_word_state.reset_pending = true;
    app.le_word_state.message = "Press 0 again for a new random word.".to_string();

    assert!(handle_key(&mut app, b'?'));

    assert!(app.show_help);
    assert!(!app.le_word_state.reset_pending);
    assert!(app.le_word_state.message.is_empty());
}
