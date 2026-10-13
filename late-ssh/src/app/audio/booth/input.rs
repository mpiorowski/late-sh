use crate::app::{
    audio::youtube::watch_url,
    common::primitives::Banner,
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

    // Ctrl+Y copies the focused track's link from any focus, the filter
    // included: every printable key is already somebody's (submit text,
    // filter text, or an action on the selected row), so the copy rides a
    // control byte, the way room search's Ctrl+Y does.
    if let ParsedInput::Byte(0x19) = event {
        copy_focused_track(app);
        return;
    }

    // While the History `/` filter is capturing, it owns every other key
    // (including Esc and Tab, which cancel the filter rather than close the
    // booth).
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

/// Ctrl+Y: the watch link of the track the focus points at. The submit row
/// has no track of its own, so it copies the one playing; the lists copy
/// their selected row. Nothing to point at (the fallback stream, an empty
/// list) leaves the clipboard alone.
fn copy_focused_track(app: &mut App) {
    let snapshot = app.audio.queue_snapshot();
    let state = &app.booth_modal_state;
    let video_id = match state.focus() {
        BoothFocus::Submit => snapshot.current.as_ref().map(|item| &item.video_id),
        BoothFocus::Queue => state
            .selected_item(&snapshot.queue)
            .map(|item| &item.video_id),
        BoothFocus::History => state
            .selected_history_item(&snapshot.history)
            .map(|item| &item.video_id),
    };
    match video_id {
        Some(video_id) => {
            app.pending_clipboard = Some(watch_url(video_id));
            app.banner = Some(Banner::success("Track link copied to clipboard!"));
        }
        None => app.banner = Some(Banner::error("No track to copy")),
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

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
