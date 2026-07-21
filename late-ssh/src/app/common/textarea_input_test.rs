use crate::app::common::textarea_input::*;
use crate::app::input::ParsedInput;
use ratatui_textarea::TextArea;

fn ta(text: &str) -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.insert_str(text);
    ta
}

fn text(ta: &TextArea<'static>) -> String {
    ta.lines().join("\n")
}

#[test]
fn single_line_submits_on_enter_and_cancels_on_escape() {
    let mut input = ta("abc");
    assert_eq!(
        handle_single_line_edit(&mut input, &ParsedInput::Byte(b'\r'), 10),
        EditOutcome::Submit
    );
    assert_eq!(
        handle_single_line_edit(&mut input, &ParsedInput::Byte(0x1B), 10),
        EditOutcome::Cancel
    );
    assert_eq!(text(&input), "abc", "submit/cancel must not mutate text");
}

#[test]
fn single_line_inserts_chars_up_to_the_limit() {
    let mut input = ta("");
    for ch in ['a', 'b', 'c', 'd'] {
        assert_eq!(
            handle_single_line_edit(&mut input, &ParsedInput::Char(ch), 3),
            EditOutcome::Handled
        );
    }
    assert_eq!(text(&input), "abc");
}

#[test]
fn single_line_accepts_raw_printable_bytes() {
    let mut input = ta("");
    handle_single_line_edit(&mut input, &ParsedInput::Byte(b'x'), 8);
    handle_single_line_edit(&mut input, &ParsedInput::Byte(b' '), 8);
    assert_eq!(text(&input), "x ");
}

#[test]
fn single_line_backspace_delete_and_home() {
    let mut input = ta("ab");
    handle_single_line_edit(&mut input, &ParsedInput::Byte(0x7F), 8);
    assert_eq!(text(&input), "a");
    handle_single_line_edit(&mut input, &ParsedInput::Byte(0x01), 8);
    handle_single_line_edit(&mut input, &ParsedInput::Delete, 8);
    assert_eq!(text(&input), "");
}

#[test]
fn single_line_paste_strips_newlines_and_clamps() {
    let mut input = ta("");
    let pasted = ParsedInput::Paste(b"he\nllo world".to_vec());
    assert_eq!(
        handle_single_line_edit(&mut input, &pasted, 5),
        EditOutcome::Handled
    );
    assert_eq!(text(&input), "hello");
}

#[test]
fn single_line_ctrl_u_clears() {
    let mut input = ta("abc");
    assert_eq!(
        handle_single_line_edit(&mut input, &ParsedInput::Byte(0x15), 8),
        EditOutcome::Handled
    );
    assert_eq!(text(&input), "");
}

#[test]
fn single_line_ignores_non_editing_keys() {
    let mut input = ta("abc");
    assert_eq!(
        handle_single_line_edit(&mut input, &ParsedInput::FocusGained, 8),
        EditOutcome::Ignored
    );
    assert_eq!(
        handle_single_line_edit(&mut input, &ParsedInput::PageUp, 8),
        EditOutcome::Ignored
    );
    assert_eq!(text(&input), "abc");
}

#[test]
fn multiline_alt_enter_inserts_newline_and_enter_submits() {
    let mut input = ta("ab");
    assert_eq!(
        handle_multiline_edit(&mut input, &ParsedInput::AltEnter, 10),
        EditOutcome::Handled
    );
    assert_eq!(
        handle_multiline_edit(&mut input, &ParsedInput::Char('c'), 10),
        EditOutcome::Handled
    );
    assert_eq!(text(&input), "ab\nc");
    assert_eq!(
        handle_multiline_edit(&mut input, &ParsedInput::Byte(b'\r'), 10),
        EditOutcome::Submit
    );
}

#[test]
fn multiline_paste_normalizes_and_keeps_newlines() {
    let mut input = ta("");
    handle_multiline_edit(&mut input, &ParsedInput::Paste(b"a\r\nb\rc".to_vec()), 16);
    assert_eq!(text(&input), "a\nb\nc");
}

#[test]
fn multiline_char_limit_counts_newlines() {
    // "a\nb" is 3 chars (newline included); the trailing "\nc" is dropped.
    let mut input = ta("");
    handle_multiline_edit(&mut input, &ParsedInput::Paste(b"a\nb\nc".to_vec()), 3);
    assert_eq!(text(&input), "a\nb");
}

#[test]
fn multiline_escape_reports_cancel() {
    let mut input = ta("abc");
    assert_eq!(
        handle_multiline_edit(&mut input, &ParsedInput::Byte(0x1B), 8),
        EditOutcome::Cancel
    );
}
