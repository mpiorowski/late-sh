use crate::app::common::readline::ctrl_byte_to_input;
use crate::app::input::{ParsedInput, sanitize_paste_markers};
use crate::app::state::App;

use super::state::{PickerKind, Row};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if app.settings_modal_state.picker_open() {
        handle_picker_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_username() {
        handle_username_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_bio() {
        handle_bio_input(app, event);
        return;
    }

    if is_close_event(&event) {
        app.show_settings = false;
        return;
    }

    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.settings_modal_state.move_row(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.settings_modal_state.move_row(-1),
        ParsedInput::Arrow(b'C') => app.settings_modal_state.cycle_setting(true),
        ParsedInput::Arrow(b'D') => app.settings_modal_state.cycle_setting(false),
        ParsedInput::Byte(b' ') | ParsedInput::Byte(b'\r') => activate_selected_row(app),
        ParsedInput::Char('e') | ParsedInput::Char('E') => activate_selected_row(app),
        _ => {}
    }
}

fn open_help(app: &mut App) {
    app.help_modal_state
        .open(crate::app::help_modal::data::HelpTopic::Overview);
    app.show_help = true;
}

pub fn handle_escape(app: &mut App) {
    handle_input(app, ParsedInput::Byte(0x1B));
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}

fn activate_selected_row(app: &mut App) {
    match app.settings_modal_state.selected_row() {
        Row::Username => app.settings_modal_state.start_username_edit(),
        Row::Bio => app.settings_modal_state.start_bio_edit(),
        Row::Theme
        | Row::BackgroundColor
        | Row::DirectMessages
        | Row::Mentions
        | Row::GameEvents
        | Row::Bell
        | Row::Cooldown => app.settings_modal_state.cycle_setting(true),
        Row::Country => app.settings_modal_state.open_picker(PickerKind::Country),
        Row::Timezone => app.settings_modal_state.open_picker(PickerKind::Timezone),
        Row::Save => {
            app.settings_modal_state.save();
            app.show_settings = false;
        }
    }
}

fn handle_username_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(0x1B) => state.cancel_username_edit(),
        ParsedInput::Byte(b'\r') => state.submit_username(),
        // Readline ^U: kill from cursor to start of line. (Single-line field,
        // so equivalent to "clear everything before the cursor".)
        ParsedInput::Byte(0x15) => state.username_kill_to_head(),
        // ^Y stays explicit so the capped `username_paste` path runs; handing
        // it to ratatui-textarea's built-in `paste()` would bypass
        // `USERNAME_MAX_LEN`.
        ParsedInput::Byte(0x19) => state.username_paste(),
        ParsedInput::Byte(0x1F) => state.username_undo(),
        ParsedInput::Byte(0x7F) => state.username_backspace(),
        ParsedInput::Delete => state.username_delete_right(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => state.username_delete_word_left(),
        ParsedInput::CtrlDelete => state.username_delete_word_right(),
        ParsedInput::Arrow(b'C') => state.username_cursor_right(),
        ParsedInput::Arrow(b'D') => state.username_cursor_left(),
        ParsedInput::CtrlArrow(b'C') => state.username_cursor_word_right(),
        ParsedInput::CtrlArrow(b'D') => state.username_cursor_word_left(),
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            for ch in cleaned.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    state.username_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => state.username_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            state.username_push(byte as char)
        }
        ParsedInput::Byte(byte) => {
            // Fallthrough: forward remaining Ctrl+<letter> chords (^A/^E/^K/
            // ^F/^B/...) to ratatui-textarea's emacs keymap. None of them
            // insert text, so the length cap isn't affected.
            if let Some(input) = ctrl_byte_to_input(byte) {
                state.username_input(input);
            }
        }
        _ => {}
    }
}

fn handle_bio_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(0x1B) => state.stop_bio_edit(),
        ParsedInput::Byte(b'\r') => state.stop_bio_edit(),
        ParsedInput::AltEnter | ParsedInput::Byte(b'\n') => state.bio_push('\n'),
        // Readline ^U: kill cursor-to-head of the current line only (not the
        // whole buffer).
        ParsedInput::Byte(0x15) => state.bio_kill_to_head(),
        // ^Y stays explicit so the capped `bio_paste` path runs; handing it
        // to ratatui-textarea's built-in `paste()` would bypass `BIO_MAX_LEN`.
        ParsedInput::Byte(0x19) => state.bio_paste(),
        ParsedInput::Byte(0x1F) => state.bio_undo(),
        ParsedInput::Byte(0x17) => state.bio_delete_word_left(),
        ParsedInput::Byte(0x7F) => state.bio_backspace(),
        ParsedInput::Delete => state.bio_delete_right(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => state.bio_delete_word_left(),
        ParsedInput::CtrlDelete => state.bio_delete_word_right(),
        ParsedInput::Arrow(b'A') => state.bio_cursor_up(),
        ParsedInput::Arrow(b'B') => state.bio_cursor_down(),
        ParsedInput::Arrow(b'C') => state.bio_cursor_right(),
        ParsedInput::Arrow(b'D') => state.bio_cursor_left(),
        ParsedInput::CtrlArrow(b'C') => state.bio_cursor_word_right(),
        ParsedInput::CtrlArrow(b'D') => state.bio_cursor_word_left(),
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            let normalized = cleaned.replace("\r\n", "\n").replace('\r', "\n");
            for ch in normalized.chars() {
                if ch == '\n' || (!ch.is_control() && ch != '\u{7f}') {
                    state.bio_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => state.bio_push(ch),
        ParsedInput::Byte(byte) => {
            // Fallthrough: forward remaining Ctrl+<letter> chords (^A/^E/^K/
            // ^F/^B/^N/^P/...) to ratatui-textarea's emacs keymap. None of
            // them insert text, so `BIO_MAX_LEN` isn't affected.
            if let Some(input) = ctrl_byte_to_input(byte) {
                state.bio_input(input);
            }
        }
        _ => {}
    }
}

fn handle_picker_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.settings_modal_state.close_picker(),
        ParsedInput::Byte(b'\r') => app.settings_modal_state.apply_picker_selection(),
        ParsedInput::Byte(0x7F) => app.settings_modal_state.picker_backspace(),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.settings_modal_state.picker_move(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.settings_modal_state.picker_move(-1),
        ParsedInput::PageDown => {
            let page = app
                .settings_modal_state
                .picker()
                .visible_height
                .get()
                .max(1) as isize;
            app.settings_modal_state.picker_move(page);
        }
        ParsedInput::PageUp => {
            let page = app
                .settings_modal_state
                .picker()
                .visible_height
                .get()
                .max(1) as isize;
            app.settings_modal_state.picker_move(-page);
        }
        ParsedInput::Scroll(delta) => app.settings_modal_state.picker_move(-delta * 3),
        ParsedInput::Char(ch) if !ch.is_control() => app.settings_modal_state.picker_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.settings_modal_state.picker_push(byte as char)
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_keys_include_esc_and_q() {
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Byte(b'q')));
        assert!(is_close_event(&ParsedInput::Byte(b'Q')));
        assert!(!is_close_event(&ParsedInput::Char('?')));
    }
}
