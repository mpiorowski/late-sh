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
            crate::app::arcade::sliding_puzzle::ui::hit_test(area, difficulty, x, y) == Some(7)
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

#[tokio::test]
async fn le_word_s_types_a_letter_until_the_round_ends_then_copies_the_card() {
    use crate::test_helpers::{make_app, new_test_db};
    use late_core::test_utils::create_test_user;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-share-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "le-word-share-token");
    app.set_screen(Screen::Arcade);
    app.game_selection = GAME_SELECTION_LE_WORD;
    app.is_playing_game = true;
    app.le_word_state.daily_word_loaded = true;
    app.le_word_state.answer = "shade".to_string();

    assert!(handle_key(&mut app, b's'));
    assert_eq!(app.le_word_state.current_guess, "s");
    assert!(app.pending_clipboard.is_none());

    app.le_word_state.current_guess.clear();
    app.le_word_state.guesses = vec!["adieu".to_string(), "shade".to_string()];
    app.le_word_state.is_game_over = true;
    app.le_word_state.won = true;

    assert!(handle_key(&mut app, b's'));
    let card = app.pending_clipboard.take().expect("card copied");
    assert!(card.starts_with("late.sh Le Word #"), "{card}");
    assert!(
        card.contains("· 2/6\n🟨🟨⬛🟨⬛\n🟩🟩🟩🟩🟩\nssh late.sh"),
        "{card}"
    );
    assert_eq!(app.le_word_state.current_guess, "");
    assert!(app.is_playing_game);
}

#[tokio::test]
async fn lobby_s_copies_the_day_card() {
    use crate::test_helpers::{make_app, new_test_db};
    use late_core::test_utils::create_test_user;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "day-card-share-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "day-card-share-token");
    app.set_screen(Screen::Arcade);
    app.is_playing_game = false;

    assert!(handle_key(&mut app, b's'));
    let card = app.pending_clipboard.take().expect("day card copied");
    assert!(card.starts_with("late.sh Daily #"), "{card}");
    assert!(
        card.contains("· 0/7\n⬛⬛⬛⬛⬛⬛⬛\nssh late.sh"),
        "{card}"
    );
}
