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
        ParsedInput::Byte(0x7F | 0x08) => {
            ta.delete_char();
        }
        ParsedInput::Delete => {
            ta.delete_next_char();
        }
        ParsedInput::CtrlBackspace => {
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
        ParsedInput::Byte(0x7F | 0x08) => {
            ta.delete_char();
        }
        ParsedInput::Delete => {
            ta.delete_next_char();
        }
        ParsedInput::CtrlBackspace => {
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


