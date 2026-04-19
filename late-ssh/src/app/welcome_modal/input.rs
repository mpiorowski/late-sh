use crate::app::input::{ParsedInput, sanitize_paste_markers};
use crate::app::state::App;

use super::state::{PickerKind, Row};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if app.welcome_modal_state.picker_open() {
        handle_picker_input(app, event);
        return;
    }

    if app.welcome_modal_state.editing_username() {
        handle_username_input(app, event);
        return;
    }

    if app.welcome_modal_state.editing_bio() {
        handle_bio_input(app, event);
        return;
    }

    match event {
        ParsedInput::Byte(0x1B) => app.show_welcome = false,
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.welcome_modal_state.move_row(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.welcome_modal_state.move_row(-1),
        ParsedInput::Arrow(b'C') => app.welcome_modal_state.cycle_setting(true),
        ParsedInput::Arrow(b'D') => app.welcome_modal_state.cycle_setting(false),
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

fn activate_selected_row(app: &mut App) {
    match app.welcome_modal_state.selected_row() {
        Row::Username => app.welcome_modal_state.start_username_edit(),
        Row::Bio => app.welcome_modal_state.start_bio_edit(),
        Row::Theme
        | Row::BackgroundColor
        | Row::DirectMessages
        | Row::Mentions
        | Row::GameEvents
        | Row::Bell
        | Row::Cooldown => app.welcome_modal_state.cycle_setting(true),
        Row::Country => app.welcome_modal_state.open_picker(PickerKind::Country),
        Row::Timezone => app.welcome_modal_state.open_picker(PickerKind::Timezone),
        Row::Save => {
            app.welcome_modal_state.save();
            app.show_welcome = false;
        }
    }
}

fn handle_username_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.welcome_modal_state.cancel_username_edit(),
        ParsedInput::Byte(b'\r') => app.welcome_modal_state.submit_username(),
        ParsedInput::Byte(0x15) => app.welcome_modal_state.clear_username(),
        ParsedInput::Byte(0x7F) => app.welcome_modal_state.username_backspace(),
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            for ch in cleaned.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    app.welcome_modal_state.username_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => app.welcome_modal_state.username_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.welcome_modal_state.username_push(byte as char)
        }
        _ => {}
    }
}

fn handle_bio_input(app: &mut App, event: ParsedInput) {
    let composer = app.welcome_modal_state.bio_input_mut();
    match event {
        ParsedInput::Byte(0x1B) => app.welcome_modal_state.stop_bio_edit(),
        ParsedInput::Byte(b'\r') => app.welcome_modal_state.stop_bio_edit(),
        ParsedInput::AltEnter | ParsedInput::Byte(b'\n') => app.welcome_modal_state.bio_push('\n'),
        ParsedInput::Byte(0x15) => composer.clear(),
        ParsedInput::Byte(0x17) => composer.delete_word_left(),
        ParsedInput::Byte(0x7F) => composer.backspace(),
        ParsedInput::Delete => composer.delete_right(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => composer.delete_word_left(),
        ParsedInput::CtrlDelete => composer.delete_word_right(),
        ParsedInput::Arrow(b'A') => composer.cursor_up(),
        ParsedInput::Arrow(b'B') => composer.cursor_down(),
        ParsedInput::Arrow(b'C') => composer.cursor_right(),
        ParsedInput::Arrow(b'D') => composer.cursor_left(),
        ParsedInput::CtrlArrow(b'C') => composer.cursor_word_right(),
        ParsedInput::CtrlArrow(b'D') => composer.cursor_word_left(),
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            let normalized = cleaned.replace("\r\n", "\n").replace('\r', "\n");
            for ch in normalized.chars() {
                if ch == '\n' || (!ch.is_control() && ch != '\u{7f}') {
                    app.welcome_modal_state.bio_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => app.welcome_modal_state.bio_push(ch),
        _ => {}
    }
}

fn handle_picker_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.welcome_modal_state.close_picker(),
        ParsedInput::Byte(b'\r') => app.welcome_modal_state.apply_picker_selection(),
        ParsedInput::Byte(0x7F) => app.welcome_modal_state.picker_backspace(),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.welcome_modal_state.picker_move(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.welcome_modal_state.picker_move(-1),
        ParsedInput::PageDown => {
            let page = app.welcome_modal_state.picker().visible_height.get().max(1) as isize;
            app.welcome_modal_state.picker_move(page);
        }
        ParsedInput::PageUp => {
            let page = app.welcome_modal_state.picker().visible_height.get().max(1) as isize;
            app.welcome_modal_state.picker_move(-page);
        }
        ParsedInput::Scroll(delta) => app.welcome_modal_state.picker_move(-delta * 3),
        ParsedInput::Char(ch) if !ch.is_control() => app.welcome_modal_state.picker_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.welcome_modal_state.picker_push(byte as char)
        }
        _ => {}
    }
}
