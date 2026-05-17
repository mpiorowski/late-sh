use crate::app::{
    input::{ParsedInput, sanitize_paste_markers},
    state::App,
};

use super::state::BoothFocus;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    let snapshot = app.audio.queue_snapshot();
    let queue_len = snapshot.queue.len();
    app.booth_modal_state.clamp(queue_len);

    match event {
        ParsedInput::Byte(0x1B) => {
            app.booth_modal_state.close();
            return;
        }
        ParsedInput::Byte(b'\t') => {
            app.booth_modal_state.toggle_focus();
            return;
        }
        _ => {}
    }

    match app.booth_modal_state.focus() {
        BoothFocus::Submit => handle_submit_input(app, event),
        BoothFocus::Queue => handle_queue_input(app, event, queue_len),
    }

    let queue_len = app.audio.queue_snapshot().queue.len();
    app.booth_modal_state.clamp(queue_len);
}

fn handle_submit_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r') => {
            if !app.audio.booth_submit_enabled() {
                return;
            }
            let value = app.booth_modal_state.take_input();
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return;
            }
            app.audio.booth_submit_public(trimmed.to_string());
        }
        ParsedInput::Byte(0x7F) | ParsedInput::Byte(0x08) => {
            app.booth_modal_state.backspace();
        }
        ParsedInput::Arrow(b'B') => {
            app.booth_modal_state.set_focus(BoothFocus::Queue);
        }
        ParsedInput::Paste(bytes) => {
            let raw = String::from_utf8_lossy(&bytes);
            let cleaned = sanitize_paste_markers(&raw);
            for ch in cleaned.chars() {
                if !ch.is_control() {
                    app.booth_modal_state.push(ch);
                }
            }
        }
        ParsedInput::Char(ch) => app.booth_modal_state.push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.booth_modal_state.push(byte as char);
        }
        _ => {}
    }
}

fn handle_queue_input(app: &mut App, event: ParsedInput, queue_len: usize) {
    match event {
        ParsedInput::Arrow(b'A') => {
            if app.booth_modal_state.selected() == 0 {
                app.booth_modal_state.set_focus(BoothFocus::Submit);
            } else {
                app.booth_modal_state.move_selection(-1, queue_len);
            }
        }
        ParsedInput::Arrow(b'B') => {
            app.booth_modal_state.move_selection(1, queue_len);
        }
        ParsedInput::PageUp => app.booth_modal_state.move_selection(-8, queue_len),
        ParsedInput::PageDown => app.booth_modal_state.move_selection(8, queue_len),
        ParsedInput::Char('+') | ParsedInput::Char('=') => cast_selected_vote(app, 1),
        ParsedInput::Char('-') | ParsedInput::Char('_') => cast_selected_vote(app, -1),
        ParsedInput::Char('0') => clear_selected_vote(app),
        ParsedInput::Char('s') | ParsedInput::Char('S') => {
            app.audio.booth_skip_vote();
        }
        _ => {}
    }
}

fn cast_selected_vote(app: &mut App, value: i16) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app.booth_modal_state.selected_item_id(&snapshot.queue) else {
        return;
    };
    app.audio.booth_vote(item_id, value);
}

fn clear_selected_vote(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app.booth_modal_state.selected_item_id(&snapshot.queue) else {
        return;
    };
    app.audio.booth_clear_vote(item_id);
}
