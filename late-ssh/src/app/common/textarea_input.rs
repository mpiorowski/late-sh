//! Shared keystroke handling for `ratatui_textarea::TextArea` edit fields.
//!
//! Modals used to carry a near-identical `ParsedInput` match per editable
//! field (see `settings_modal/input.rs`). These helpers centralize the
//! translation from parsed terminal input to `TextArea` edits; callers only
//! interpret the returned [`EditOutcome`] (commit or revert the edit).

use ratatui_textarea::{CursorMove, TextArea};

use crate::app::input::{ParsedInput, sanitize_paste_markers};

/// What the caller should do after a keystroke was offered to an edit field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditOutcome {
    /// The key was consumed and applied to the textarea.
    Handled,
    /// Enter: commit the edit.
    Submit,
    /// Esc: leave edit mode; the caller decides whether to revert.
    Cancel,
    /// Not an editing key; the caller may handle it itself.
    Ignored,
}

/// Keystroke handling for a single-line field (newlines stripped, `max_chars` cap).
pub fn handle_single_line_edit(
    ta: &mut TextArea<'static>,
    event: &ParsedInput,
    max_chars: usize,
) -> EditOutcome {
    match event {
        ParsedInput::Byte(0x1B) => return EditOutcome::Cancel,
        ParsedInput::Byte(b'\r') => return EditOutcome::Submit,
        ParsedInput::Byte(0x15) => clear(ta),
        ParsedInput::Byte(0x01) | ParsedInput::Home => ta.move_cursor(CursorMove::Head),
        ParsedInput::Byte(0x05) | ParsedInput::End => ta.move_cursor(CursorMove::End),
        ParsedInput::Byte(0x19) => {
            let yank = ta.yank_text();
            insert_single_line_limited(ta, &yank, max_chars);
        }
        ParsedInput::Byte(0x1F) => {
            ta.undo();
        }
        ParsedInput::Byte(0x7F) => {
            ta.delete_char();
        }
        ParsedInput::Delete => {
            ta.delete_next_char();
        }
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => {
            ta.delete_word();
        }
        ParsedInput::CtrlDelete => {
            ta.delete_next_word();
        }
        ParsedInput::Arrow(b'C') => ta.move_cursor(CursorMove::Forward),
        ParsedInput::Arrow(b'D') => ta.move_cursor(CursorMove::Back),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            ta.move_cursor(CursorMove::WordForward)
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            ta.move_cursor(CursorMove::WordBack)
        }
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(pasted));
            insert_single_line_limited(ta, &cleaned, max_chars);
        }
        ParsedInput::Char(ch) if !ch.is_control() => push_char_limited(ta, *ch, max_chars),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || *byte == b' ' => {
            push_char_limited(ta, *byte as char, max_chars)
        }
        _ => return EditOutcome::Ignored,
    }
    EditOutcome::Handled
}

/// Keystroke handling for a multiline field (bio convention: Enter submits,
/// Alt+Enter inserts a newline, Esc returns `Cancel` and the caller decides).
pub fn handle_multiline_edit(
    ta: &mut TextArea<'static>,
    event: &ParsedInput,
    max_chars: usize,
) -> EditOutcome {
    match event {
        ParsedInput::Byte(0x1B) => return EditOutcome::Cancel,
        ParsedInput::Byte(b'\r') => return EditOutcome::Submit,
        ParsedInput::AltEnter | ParsedInput::Byte(b'\n') => push_char_limited(ta, '\n', max_chars),
        ParsedInput::Byte(0x15) => clear(ta),
        ParsedInput::Byte(0x19) => {
            let yank = ta.yank_text();
            insert_multiline_limited(ta, &yank, max_chars);
        }
        ParsedInput::Byte(0x1F) => {
            ta.undo();
        }
        ParsedInput::Byte(0x17) => {
            ta.delete_word();
        }
        ParsedInput::Byte(0x7F) => {
            ta.delete_char();
        }
        ParsedInput::Delete => {
            ta.delete_next_char();
        }
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => {
            ta.delete_word();
        }
        ParsedInput::CtrlDelete => {
            ta.delete_next_word();
        }
        ParsedInput::Arrow(b'A') => ta.move_cursor(CursorMove::Up),
        ParsedInput::Arrow(b'B') => ta.move_cursor(CursorMove::Down),
        ParsedInput::Arrow(b'C') => ta.move_cursor(CursorMove::Forward),
        ParsedInput::Arrow(b'D') => ta.move_cursor(CursorMove::Back),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            ta.move_cursor(CursorMove::WordForward)
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            ta.move_cursor(CursorMove::WordBack)
        }
        ParsedInput::Home => ta.move_cursor(CursorMove::Head),
        ParsedInput::End => ta.move_cursor(CursorMove::End),
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(pasted));
            insert_multiline_limited(ta, &cleaned, max_chars);
        }
        ParsedInput::Char(ch) if !ch.is_control() => push_char_limited(ta, *ch, max_chars),
        _ => return EditOutcome::Ignored,
    }
    EditOutcome::Handled
}

/// Total character count, counting newlines between rows. This is the
/// accounting the `max_chars` limits use; callers (e.g. char counters in
/// modal UIs) can share it to stay consistent.
pub(crate) fn char_count(ta: &TextArea<'static>) -> usize {
    ta.lines().iter().map(|l| l.chars().count()).sum::<usize>() + ta.lines().len().saturating_sub(1)
}

fn push_char_limited(ta: &mut TextArea<'static>, ch: char, max_chars: usize) {
    if char_count(ta) < max_chars {
        ta.insert_char(ch);
    }
}

/// Insert `text` with newlines and control chars stripped, up to `max_chars`.
fn insert_single_line_limited(ta: &mut TextArea<'static>, text: &str, max_chars: usize) {
    for ch in text.chars() {
        if char_count(ta) >= max_chars {
            break;
        }
        if !ch.is_control() && ch != '\n' && ch != '\r' {
            ta.insert_char(ch);
        }
    }
}

/// Insert `text` with line endings normalized to `\n` and kept, up to `max_chars`.
fn insert_multiline_limited(ta: &mut TextArea<'static>, text: &str, max_chars: usize) {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for ch in normalized.chars() {
        if char_count(ta) >= max_chars {
            break;
        }
        if ch == '\n' || (!ch.is_control() && ch != '\u{7f}') {
            ta.insert_char(ch);
        }
    }
}

fn clear(ta: &mut TextArea<'static>) {
    ta.select_all();
    ta.cut();
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
