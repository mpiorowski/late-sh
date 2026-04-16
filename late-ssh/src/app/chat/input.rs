use crate::app::state::App;
use uuid::Uuid;

fn is_next_room_key(byte: u8) -> bool {
    matches!(byte, b'l' | b'L' | 0x0E)
}

fn is_prev_room_key(byte: u8) -> bool {
    matches!(byte, b'h' | b'H' | 0x10)
}

pub fn handle_compose_input(app: &mut App, byte: u8) {
    if app.chat.is_autocomplete_active() {
        match byte {
            0x1B => {
                app.chat.ac_dismiss();
                return;
            }
            b'\t' | b'\r' | b'\n' => {
                app.chat.ac_confirm();
                return;
            }
            _ => {} // fall through to normal handling
        }
    }

    match byte {
        0x1B => {
            app.chat.reset_composer();
        }
        b'\r' | b'\n' => {
            if let Some(b) = app.chat.submit_composer() {
                app.banner = Some(b);
            }
        }
        0x15 => {
            // Ctrl-U: clear composer
            app.chat.composer_clear();
            app.chat.update_autocomplete();
        }
        0x7F => {
            app.chat.composer_backspace();
            app.chat.update_autocomplete();
        }
        b if (32..127).contains(&b) => {
            app.chat.composer_push(b as char);
            app.chat.update_autocomplete();
        }
        _ => {}
    }
}

pub fn handle_autocomplete_arrow(app: &mut App, key: u8) {
    match key {
        b'A' => app.chat.ac_move_selection(-1),
        b'B' => app.chat.ac_move_selection(1),
        _ => {}
    }
}

pub fn handle_scroll(app: &mut App, delta: isize) {
    let Some(room_id) = app.chat.selected_room_id else {
        return;
    };
    app.chat.select_message_in_room(room_id, delta);
}

pub fn handle_scroll_in_room(app: &mut App, room_id: Uuid, delta: isize) {
    app.chat.select_message_in_room(room_id, delta);
}

fn switch_room(app: &mut App, delta: isize) {
    if app.chat.move_selection(delta) {
        app.chat.reset_composer();
        app.chat.mark_selected_room_read();
        app.chat.request_list();
    }
}

/// Shared message-list navigation and actions. Consumed by both the chat page
/// and the dashboard card so that d/r/e/j/k/etc. behave identically on both
/// screens and new message actions only need to be wired here.
///
/// Returns true if the key was handled.
pub fn handle_message_action(app: &mut App, byte: u8) -> bool {
    let Some(room_id) = app.chat.selected_room_id else {
        return false;
    };
    handle_message_action_in_room(app, room_id, byte)
}

pub fn handle_message_action_in_room(app: &mut App, room_id: Uuid, byte: u8) -> bool {
    // `d` deletes and keeps the cursor on the adjacent message so you can
    // reap a run of your own messages with repeated presses.
    // `r` enters reply mode and drops the selection.
    // `e` enters edit mode and drops the selection.
    match byte {
        b'd' | b'D' => {
            if let Some(b) = app.chat.delete_selected_message_in_room(room_id) {
                app.banner = Some(b);
            }
            return true;
        }
        b'r' | b'R' => {
            app.chat.begin_reply_to_selected_in_room(room_id);
            app.chat.clear_message_selection();
            return true;
        }
        b'e' | b'E' => {
            if let Some(b) = app.chat.begin_edit_selected_in_room(room_id) {
                app.banner = Some(b);
            } else {
                app.chat.clear_message_selection();
            }
            return true;
        }
        _ => {}
    }

    if !matches!(byte, b'j' | b'J' | b'k' | b'K' | 0x04 | 0x15) {
        app.chat.clear_message_selection();
    }

    match byte {
        b'j' | b'J' => {
            app.chat.select_message_in_room(room_id, -1);
            true
        }
        b'k' | b'K' => {
            app.chat.select_message_in_room(room_id, 1);
            true
        }
        0x04 => {
            // Ctrl-D: half-page down. `select_message_in_room` delta is in
            // MESSAGES, not rows, and chat messages wrap to ~3 rows each,
            // so divide terminal height by 6 to feel like half a visible page.
            let step = (app.size.1 / 6).max(1) as isize;
            app.chat.select_message_in_room(room_id, -step);
            true
        }
        0x15 => {
            // Ctrl-U: half-page up. Same rationale as Ctrl-D above.
            let step = (app.size.1 / 6).max(1) as isize;
            app.chat.select_message_in_room(room_id, step);
            true
        }
        b'g' | b'G' => {
            app.chat.clear_message_selection();
            true
        }
        b'i' | b'I' => {
            app.chat.start_composing_in_room(room_id);
            true
        }
        _ => false,
    }
}

/// Arrow-key message navigation shared between screens.
pub fn handle_message_arrow(app: &mut App, key: u8) -> bool {
    let Some(room_id) = app.chat.selected_room_id else {
        return false;
    };
    handle_message_arrow_in_room(app, room_id, key)
}

pub fn handle_message_arrow_in_room(app: &mut App, room_id: Uuid, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.select_message_in_room(room_id, 1);
            true
        }
        b'B' => {
            app.chat.select_message_in_room(room_id, -1);
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.chat.room_jump_active {
        app.chat.cancel_room_jump();
        return true;
    }
    if app.chat.notifications_selected {
        return super::notifications::input::handle_arrow(app, key);
    }
    if app.chat.news_selected {
        return super::news::input::handle_arrow(app, key);
    }
    handle_message_arrow(app, key)
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    if app.chat.room_jump_active {
        match byte {
            b' ' => {
                app.chat.cancel_room_jump();
                return true;
            }
            _ => {
                let changed = app.chat.handle_room_jump_key(byte);
                if changed {
                    app.chat.reset_composer();
                    app.chat.mark_selected_room_read();
                    app.chat.request_list();
                }
                return true;
            }
        }
    }

    if byte == b' ' {
        app.chat.activate_room_jump();
        return true;
    }

    if app.chat.notifications_selected {
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        return super::notifications::input::handle_byte(app, byte);
    }

    if app.chat.news_selected {
        // Room-switch keys still work when a virtual room is selected.
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        return super::news::input::handle_byte(app, byte);
    }

    if handle_message_action(app, byte) {
        return true;
    }

    match byte {
        b if is_next_room_key(b) => {
            switch_room(app, 1);
            true
        }
        b if is_prev_room_key(b) => {
            switch_room(app, -1);
            true
        }
        b'\r' | b'\n' => {
            app.chat.start_composing();
            true
        }
        b'c' | b'C' => {
            if let Some(ref registry) = app.web_chat_registry {
                let username = app.profile_state.profile().username.clone();
                let base_url = app
                    .connect_url
                    .rsplit_once('/')
                    .map_or(&*app.connect_url, |p| p.0);
                let token = registry.create_link(app.user_id, username);
                let url = format!("{}/chat/{}", base_url, token);
                app.pending_clipboard = Some(url.clone());
                app.web_chat_qr_url = Some(url);
                app.show_web_chat_qr = true;
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_next_room_key, is_prev_room_key};

    #[test]
    fn next_room_keys_include_ctrl_n() {
        assert!(is_next_room_key(b'l'));
        assert!(is_next_room_key(b'L'));
        assert!(is_next_room_key(0x0E));
        assert!(!is_next_room_key(b'h'));
    }

    #[test]
    fn prev_room_keys_include_ctrl_p() {
        assert!(is_prev_room_key(b'h'));
        assert!(is_prev_room_key(b'H'));
        assert!(is_prev_room_key(0x10));
        assert!(!is_prev_room_key(b'l'));
    }
}
