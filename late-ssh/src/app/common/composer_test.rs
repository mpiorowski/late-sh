use crate::app::common::composer::*;
use crate::app::common::theme;
use ratatui::style::Modifier;
use ratatui_textarea::WrapMode;

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

/// On the terminal palette both `TEXT()` and `BG_CANVAS()` resolve to
/// `Color::Reset`, so a cursor built as an explicit fg/bg pair of the two
/// paints nothing. The visible cursor inverts the cell instead and lets the
/// terminal supply the pair, which holds on every palette.
#[test]
fn themed_textarea_visible_cursor_inverts_the_cell() {
    theme::set_current_by_id("terminal");
    let textarea = new_themed_textarea("Type a message...", WrapMode::Word, true);
    let style = textarea.cursor_style();
    theme::set_current_by_id("contrast");

    assert!(
        style.add_modifier.contains(Modifier::REVERSED),
        "visible cursor does not invert the cell: {style:?}"
    );
    assert_eq!(style.bg, None);
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
    assert!(
        textarea
            .cursor_style()
            .add_modifier
            .contains(Modifier::REVERSED)
    );

    theme::set_current_by_id("late");
}
