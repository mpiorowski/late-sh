use ratatui_textarea::{TextArea, WrapMode};
use crate::app::common::composer::*;
use crate::app::common::theme;

#[test]
fn composer_rows_soft_wrap_words() {
    let rows = build_composer_rows("hello wide world", 8);
    let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(texts, vec!["hello", "wide", "world"]);
}

#[test]
fn themed_textarea_uses_theme_text_color() {
    let textarea = new_themed_textarea("Type a message...", WrapMode::Word, false);
    assert_eq!(textarea.style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().bg, None);
}

#[test]
fn themed_textarea_visible_cursor_uses_explicit_theme_colors() {
    let textarea = new_themed_textarea("Type a message...", WrapMode::Word, true);
    assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
    assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));
}

#[test]
fn apply_themed_textarea_style_refreshes_existing_textarea_colors() {
    theme::set_current_by_id("late");
    let mut textarea = new_themed_textarea("Type a message...", WrapMode::Word, false);
    let late_text = textarea.style().fg;

    theme::set_current_by_id("contrast");
    apply_themed_textarea_style(&mut textarea, true);

    assert_ne!(textarea.style().fg, late_text);
    assert_eq!(textarea.style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
    assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));

    theme::set_current_by_id("late");
}
