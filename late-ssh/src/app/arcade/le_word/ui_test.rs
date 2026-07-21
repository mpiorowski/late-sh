use super::*;

#[test]
fn result_panel_prefers_space_below_board() {
    let board_area = Rect::new(0, 0, 80, 40);
    let board_rect = Rect::new(28, 10, 24, 13);
    let keyboard_rect = Rect::new(20, 25, 39, 5);

    let area = result_panel_area(board_area, board_rect, Some(keyboard_rect));

    assert!(area.y > keyboard_rect.y + keyboard_rect.height);
    assert_eq!(area.width, 28);
    assert_eq!(area.height, 4);
}

#[test]
fn layout_places_keyboard_two_rows_below_board() {
    let layout = le_word_layout(Rect::new(0, 0, 80, 40));
    let keyboard = layout.keyboard.expect("keyboard fits");

    assert_eq!(
        keyboard.y,
        layout.board.y + layout.board.height + BOARD_KEYBOARD_GAP
    );
    assert_eq!(keyboard.width, KEYBOARD_WIDTH);
    assert_eq!(keyboard.height, KEYBOARD_HEIGHT);
}

#[test]
fn keyboard_hit_test_maps_clicks_to_keys() {
    let area = Rect::new(0, 0, 80, 40);

    assert_eq!(
        keyboard_hit_test(area, 20, 25),
        Some(KeyboardKey::Letter('q'))
    );
    assert_eq!(
        keyboard_hit_test(area, 22, 27),
        Some(KeyboardKey::Letter('a'))
    );
    assert_eq!(keyboard_hit_test(area, 20, 29), Some(KeyboardKey::Enter));
    assert_eq!(
        keyboard_hit_test(area, 54, 29),
        Some(KeyboardKey::Backspace)
    );
    assert_eq!(keyboard_hit_test(area, 0, 0), None);
}
