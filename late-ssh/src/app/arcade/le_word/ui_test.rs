use super::*;
use crate::app::arcade::le_word::svc::LeWordService;
use late_core::db::{Db, DbConfig};
use late_core::models::le_word::DailyWord;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

fn todays_state(answer: &str) -> State {
    let (activity_feed, _) = tokio::sync::broadcast::channel(1);
    let svc = LeWordService::new(
        Db::new(&DbConfig::default()).expect("test db pool"),
        activity_feed,
    );
    let now = chrono::Utc::now();
    let word = DailyWord {
        id: Uuid::now_v7(),
        created: now,
        updated: now,
        puzzle_date: svc.today(),
        answer_word: answer.to_string(),
    };
    State::new(Uuid::now_v7(), svc, Some(word), None)
}

/// The terminal owns the page's background: "Sync terminal background"
/// paints it to the theme's canvas through OSC 11, and with the setting off
/// it stays the user's own. The board, the keyboard, and the rules must not
/// lay the canvas down themselves.
#[test]
fn the_board_leaves_the_background_to_the_terminal() {
    theme::set_current_by_id("late");
    let canvas = theme::BG_CANVAS();
    let mut state = todays_state("crane");
    state.guesses.push("trace".to_string());
    for show_rules in [false, true] {
        state.show_rules = show_rules;
        let mut terminal = Terminal::new(TestBackend::new(80, 40)).expect("terminal");
        terminal
            .draw(|frame| draw_game(frame, frame.area(), &state, true))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let painted: Vec<(u16, u16)> = buffer
            .area()
            .positions()
            .filter(|position| buffer[*position].bg == canvas)
            .map(|position| (position.x, position.y))
            .collect();
        assert_eq!(
            painted,
            Vec::new(),
            "rules open: {show_rules}: cells painted with the canvas"
        );
    }
}

/// A tile's score must read without its color: several themes give
/// `SUCCESS`, `AMBER`, and `TEXT_FAINT` one value, so two scores that differ
/// only in color look the same there. Text and modifiers are what every
/// palette keeps.
#[test]
fn every_letter_score_has_its_own_tile_shape() {
    let shape = |score| {
        let tile = score_tile('R', score);
        (tile.content.to_string(), tile.style.add_modifier)
    };
    let scores = [
        LetterScore::Correct,
        LetterScore::Present,
        LetterScore::Absent,
    ];
    for (i, a) in scores.iter().enumerate() {
        for b in &scores[i + 1..] {
            assert_ne!(shape(*a), shape(*b), "{a:?} and {b:?} share a shape");
        }
    }
}

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
