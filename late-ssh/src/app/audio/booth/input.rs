use crate::app::{
    input::{ParsedInput, sanitize_paste_markers},
    state::App,
};

use super::state::BoothFocus;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    let snapshot = app.audio.queue_snapshot();
    let queue_len = snapshot.queue.len();
    let history_len = app
        .booth_modal_state
        .filtered_history_len(&snapshot.history);
    app.booth_modal_state.clamp(queue_len, history_len);

    // While the History `/` filter is capturing, it owns every key (including
    // Esc and Tab, which cancel the filter rather than close the booth).
    if app.booth_modal_state.history_filter_active() {
        handle_history_filter_input(app, event);
        reclamp(app);
        return;
    }

    match event {
        ParsedInput::Byte(0x1B) => {
            app.booth_modal_state.close();
            return;
        }
        ParsedInput::Byte(b'\t') => {
            app.booth_modal_state
                .cycle_focus(app.audio.booth_submit_enabled());
            return;
        }
        _ => {}
    }

    match app.booth_modal_state.focus() {
        BoothFocus::Submit => handle_submit_input(app, event),
        BoothFocus::Queue => handle_queue_input(app, event, queue_len),
        BoothFocus::History => handle_history_input(app, event, history_len),
    }

    reclamp(app);
}

fn reclamp(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let queue_len = snapshot.queue.len();
    let history_len = app
        .booth_modal_state
        .filtered_history_len(&snapshot.history);
    app.booth_modal_state.clamp(queue_len, history_len);
}

fn handle_history_filter_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.booth_modal_state.apply_history_filter();
        }
        ParsedInput::Byte(0x1B) => {
            app.booth_modal_state.cancel_history_filter();
        }
        ParsedInput::Byte(0x7F) | ParsedInput::Byte(0x08) => {
            app.booth_modal_state.backspace_history_filter();
        }
        // Ctrl+W clears the whole query.
        ParsedInput::Byte(0x17) => {
            app.booth_modal_state.clear_history_filter_query();
        }
        ParsedInput::Paste(bytes) => {
            let raw = String::from_utf8_lossy(&bytes);
            let cleaned = sanitize_paste_markers(&raw);
            for ch in cleaned.chars() {
                app.booth_modal_state.push_history_filter(ch);
            }
        }
        ParsedInput::Char(ch) => app.booth_modal_state.push_history_filter(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.booth_modal_state.push_history_filter(byte as char);
        }
        _ => {}
    }
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
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
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
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => {
            if app.booth_modal_state.selected_queue() == 0 {
                app.booth_modal_state.set_focus(BoothFocus::Submit);
            } else {
                app.booth_modal_state.move_selection(-1, queue_len);
            }
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
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
        ParsedInput::Char('d') | ParsedInput::Char('D') => {
            delete_selected(app);
        }
        ParsedInput::Char('u') | ParsedInput::Char('U') => {
            toggle_unskippable_selected(app);
        }
        ParsedInput::Char(']') | ParsedInput::Char('[') => {
            app.booth_modal_state.set_focus(BoothFocus::History);
        }
        _ => {}
    }
}

fn handle_history_input(app: &mut App, event: ParsedInput, history_len: usize) {
    match event {
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => {
            app.booth_modal_state.move_selection(-1, history_len);
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
            app.booth_modal_state.move_selection(1, history_len);
        }
        ParsedInput::PageUp => app.booth_modal_state.move_selection(-8, history_len),
        ParsedInput::PageDown => app.booth_modal_state.move_selection(8, history_len),
        ParsedInput::Char('+') | ParsedInput::Char('=') => cast_selected_history_vote(app, 1),
        ParsedInput::Char('-') | ParsedInput::Char('_') => cast_selected_history_vote(app, -1),
        ParsedInput::Char('0') => clear_selected_history_vote(app),
        ParsedInput::Byte(b'\r') => requeue_selected_history(app),
        ParsedInput::Char('d') | ParsedInput::Char('D') => delete_selected_history(app),
        ParsedInput::Char('/') | ParsedInput::Char('?') => {
            app.booth_modal_state.enter_history_filter();
        }
        ParsedInput::Char(']') | ParsedInput::Char('[') => {
            app.booth_modal_state.set_focus(BoothFocus::Queue);
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

fn delete_selected(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app.booth_modal_state.selected_item_id(&snapshot.queue) else {
        return;
    };
    app.audio.booth_delete(item_id);
}

fn toggle_unskippable_selected(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app.booth_modal_state.selected_item_id(&snapshot.queue) else {
        return;
    };
    app.audio.booth_toggle_unskippable(item_id);
}

fn cast_selected_history_vote(app: &mut App, value: i16) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app
        .booth_modal_state
        .selected_history_item_id(&snapshot.history)
    else {
        return;
    };
    app.audio.booth_history_vote(item_id, value);
}

fn clear_selected_history_vote(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app
        .booth_modal_state
        .selected_history_item_id(&snapshot.history)
    else {
        return;
    };
    app.audio.booth_history_clear_vote(item_id);
}

fn requeue_selected_history(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app
        .booth_modal_state
        .selected_history_item_id(&snapshot.history)
    else {
        return;
    };
    app.audio.booth_history_requeue(item_id);
}

fn delete_selected_history(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let Some(item_id) = app
        .booth_modal_state
        .selected_history_item_id(&snapshot.history)
    else {
        return;
    };
    app.audio.booth_history_delete(item_id);
}
