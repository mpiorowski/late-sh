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
