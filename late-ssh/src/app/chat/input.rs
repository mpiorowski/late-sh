use crate::app::common::primitives::Banner;
use crate::app::common::readline::ctrl_byte_to_input;
use crate::app::help_modal::data::HelpTopic;
use crate::app::state::App;
use uuid::Uuid;

fn is_next_room_key(byte: u8) -> bool {
    matches!(byte, b'l' | b'L' | 0x0E)
}

fn is_prev_room_key(byte: u8) -> bool {
    matches!(byte, b'h' | b'H' | 0x10)
}

fn leader_reaction_kind(byte: u8) -> Option<i16> {
    match byte {
        b'1' => Some(1),
        b'2' => Some(2),
        b'3' => Some(3),
        b'4' => Some(4),
        b'5' => Some(5),
        b'6' => Some(6),
        b'7' => Some(7),
        b'8' => Some(8),
        _ => None,
    }
}

pub fn handle_compose_input(
    app: &mut App,
    byte: u8,
    allow_room_switch: bool,
    from_dashboard: bool,
) {
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
            _ => {}
        }
    }

    match byte {
        0x1B => app.chat.reset_composer(),
        b'\r' | b'\n' => {
            if let Some(b) = app.chat.submit_composer(false, from_dashboard) {
                app.banner = Some(b);
            }
            handle_post_submit_requests(app);
        }
        0x15 => {
            // Readline ^U: kill from cursor to start of current line.
            app.chat.composer_kill_to_head();
            app.chat.update_autocomplete();
        }
        0x1F => {
            // Ctrl-/ (same byte as Ctrl-_): undo
            app.chat.composer_undo();
            app.chat.update_autocomplete();
        }
        0x7F => {
            app.chat.composer_backspace();
            app.chat.update_autocomplete();
        }
        // ^N / ^P switch to the next/previous room without losing the
        // in-progress draft — useful when you start typing and realize you
        // meant to be in another room. Reply/edit targets are dropped on
        // the jump (they point at a message in the prior room); composer
        // text and cursor survive. Shadows ratatui-textarea's cursor-
        // down/up, which is rarely useful in a chat composer.
        0x0E if allow_room_switch => {
            if app.chat.switch_room_preserving_draft(1) {
                app.sync_visible_chat_room();
            }
            app.chat.update_autocomplete();
        }
        0x10 if allow_room_switch => {
            if app.chat.switch_room_preserving_draft(-1) {
                app.sync_visible_chat_room();
            }
            app.chat.update_autocomplete();
        }
        b => {
            // Hand remaining Ctrl+<letter> chords to ratatui-textarea so its
            // built-in emacs keymap owns ^A/^E/^K/^Y/^F/^B/etc. ^W and ^H
            // are intercepted earlier in app::input for delete-word-left
            // and don't reach this point.
            if let Some(input) = ctrl_byte_to_input(b) {
                app.chat.composer_input(input);
                app.chat.update_autocomplete();
            }
        }
    }
}

fn open_help_modal(app: &mut App, topic: HelpTopic) {
    app.help_modal_state.open(topic);
    app.show_help = true;
}

fn open_settings_modal(app: &mut App) {
    app.settings_modal_state.open_from_profile(
        app.profile_state.profile(),
        app.chat.favorite_room_options(),
        crate::app::settings_modal::ui::MODAL_WIDTH,
    );
    app.show_settings = true;
}

pub(crate) fn handle_post_submit_requests(app: &mut App) {
    if app.chat.take_requested_quit() {
        crate::app::input::trigger_global_quit(app);
    }
    if let Some(topic) = app.chat.take_requested_help_topic() {
        open_help_modal(app, topic);
    }
    if app.chat.take_requested_settings_modal() {
        open_settings_modal(app);
    }
}

pub fn handle_compose_char(app: &mut App, ch: char) {
    app.chat.composer_push(ch);
    app.chat.update_autocomplete();
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
        app.sync_visible_chat_room();
        app.chat.request_list();
    }
}

/// Shared message-list navigation and actions. Consumed by both the chat page
/// and the dashboard card so that d/r/e/p/j/k/etc. behave identically on both
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
    if app.chat.is_reaction_leader_active() {
        if let Some(kind) = leader_reaction_kind(byte) {
            if let Some(banner) = app.chat.react_to_selected_message_in_room(room_id, kind) {
                app.banner = Some(banner);
            }
            return true;
        }
        app.chat.cancel_reaction_leader();
        return true;
    }

    // `d` deletes and keeps the cursor on the adjacent message so you can
    // reap a run of your own messages with repeated presses.
    // `r` enters reply mode and drops the selection.
    // `e` enters edit mode and drops the selection.
    // `p` opens a read-only profile modal for the selected author.
    match byte {
        b'f' | b'F' if app.chat.begin_reaction_leader() => return true,
        b'd' | b'D' => {
            if let Some(b) = app.chat.delete_selected_message_in_room(room_id) {
                app.banner = Some(b);
            }
            return true;
        }
        b'r' | b'R' => {
            if let Some(b) = app.chat.begin_reply_to_selected_in_room(room_id) {
                app.banner = Some(b);
            } else {
                app.chat.clear_message_selection();
            }
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
        b'p' => {
            if let Some((user_id, username)) = app.chat.selected_message_author_in_room(room_id) {
                app.profile_modal_state.open(user_id, username);
                app.show_profile_modal = true;
                return true;
            }
        }
        b'c' => {
            if let Some(body) = app.chat.selected_message_body_in_room(room_id) {
                app.pending_clipboard = Some(body);
                app.banner = Some(Banner::success("Message copied to clipboard!"));
                app.chat.clear_message_selection();
                return true;
            }
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
    // Left/Right switch rooms (mirrors h/l). Up/Down stay room-local for
    // message selection.
    match key {
        b'C' => {
            switch_room(app, 1);
            return true;
        }
        b'D' => {
            switch_room(app, -1);
            return true;
        }
        _ => {}
    }
    if app.chat.notifications_selected {
        return super::notifications::input::handle_arrow(app, key);
    }
    if app.chat.discover_selected {
        return super::discover::input::handle_arrow(app, key);
    }
    if app.chat.news_selected {
        return super::news::input::handle_arrow(app, key);
    }
    if app.chat.showcase_selected {
        return super::showcase::input::handle_arrow(app, key);
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
                    app.sync_visible_chat_room();
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

    if app.chat.discover_selected {
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        return super::discover::input::handle_byte(app, byte);
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

    if app.chat.showcase_selected {
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        return super::showcase::input::handle_byte(app, byte);
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
        b'C' => {
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
    use super::{is_next_room_key, is_prev_room_key, leader_reaction_kind};

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

    #[test]
    fn leader_reaction_keys_are_plain_digits() {
        assert_eq!(leader_reaction_kind(b'1'), Some(1));
        assert_eq!(leader_reaction_kind(b'5'), Some(5));
        assert_eq!(leader_reaction_kind(b'6'), Some(6));
        assert_eq!(leader_reaction_kind(b'7'), Some(7));
        assert_eq!(leader_reaction_kind(b'8'), Some(8));
        assert_eq!(leader_reaction_kind(b'!'), None);
    }
}
