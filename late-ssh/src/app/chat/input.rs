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

fn leader_reaction_emoji(byte: u8) -> Option<&'static str> {
    match byte {
        b'1' => Some(crate::app::chat::ui_text::reaction_label(1)),
        b'2' => Some(crate::app::chat::ui_text::reaction_label(2)),
        b'3' => Some(crate::app::chat::ui_text::reaction_label(3)),
        b'4' => Some(crate::app::chat::ui_text::reaction_label(4)),
        b'5' => Some(crate::app::chat::ui_text::reaction_label(5)),
        b'6' => Some(crate::app::chat::ui_text::reaction_label(6)),
        b'7' => Some(crate::app::chat::ui_text::reaction_label(7)),
        b'8' => Some(crate::app::chat::ui_text::reaction_label(8)),
        b'9' => Some(crate::app::chat::ui_text::reaction_label(9)),
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
            let keep_open = app.profile_state.profile().keep_composer_focused;
            if let Some(b) = app.chat.submit_composer(keep_open, from_dashboard) {
                app.banner = Some(b);
            }
            handle_post_submit_requests(app, from_dashboard);
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
    app.show_poll_modal = false;
    app.poll_modal_state.close();
    app.help_modal_state
        .set_keep_composer_focused(app.profile_state.profile().keep_composer_focused);
    app.help_modal_state.open(topic);
    app.show_help = true;
}

fn open_settings_modal(app: &mut App) {
    app.show_hub_modal = false;
    app.show_poll_modal = false;
    app.poll_modal_state.close();
    app.settings_modal_state
        .open_from_profile(app.profile_state.profile());
    app.show_settings = true;
}

fn open_mod_modal(app: &mut App) {
    app.show_help = false;
    app.show_settings = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_bonsai_v2_modal = false;
    app.show_poll_modal = false;
    app.poll_modal_state.close();
    app.show_quit_confirm = false;
    app.mod_modal_state
        .open(app.permissions.can_access_mod_surface());
    app.show_mod_modal = true;
}

fn open_poll_modal(app: &mut App, room_id: Uuid) {
    app.show_help = false;
    app.show_settings = false;
    app.show_mod_modal = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_sheet_modal = false;
    app.show_bonsai_modal = false;
    app.show_bonsai_v2_modal = false;
    app.show_quit_confirm = false;
    app.pet_state.cancel_play();
    app.show_cat_modal = false;
    crate::app::input::close_icon_picker(app);
    app.chat.close_overlay();
    app.chat.close_news_modal();
    app.pending_chat_profile_open = None;
    app.poll_modal_state.open(room_id);
    app.show_poll_modal = true;
}

pub(crate) fn open_requested_poll_modal(app: &mut App, room_id: Uuid, allow_poll_modal: bool) {
    if allow_poll_modal {
        open_poll_modal(app, room_id);
    } else {
        app.banner = Some(Banner::error("Polls are available from Home chat"));
    }
}

pub(crate) fn handle_post_submit_requests(app: &mut App, allow_poll_modal: bool) {
    if app.chat.take_requested_quit() {
        crate::app::input::trigger_global_quit(app);
    }
    if let Some(msg) = app.chat.take_requested_brb() {
        app.go_afk(msg);
    }
    if app.chat.take_sent_regular_message() && app.afk.is_some() {
        app.return_from_afk();
    }
    if let Some(url) = app.chat.take_requested_audio_url() {
        app.audio.submit_trusted(url);
    }
    if let Some(url) = app.chat.take_requested_audio_fallback_url() {
        app.audio.set_youtube_fallback(url);
    }
    if app.chat.take_requested_audio_skip() {
        app.audio.skip_trusted();
    }
    if let Some(command) = app.chat.take_requested_voice_command() {
        let banner = match command {
            crate::app::chat::state::VoiceCommand::Join => app.voice_toggle_join(),
            crate::app::chat::state::VoiceCommand::Mute => app.voice_toggle_muted(),
        };
        app.banner = Some(banner);
    }
    if let Some(topic) = app.chat.take_requested_help_topic() {
        open_help_modal(app, topic);
    }
    if app.chat.take_requested_settings_modal() {
        open_settings_modal(app);
    }
    if app.chat.take_requested_mod_modal() {
        open_mod_modal(app);
    }
    if let Some(room_id) = app.chat.take_requested_poll_room() {
        open_requested_poll_modal(app, room_id, allow_poll_modal);
    }
    if app.chat.take_requested_ultimate_modal() {
        crate::app::ultimates::open_ultimate_modal(app);
    }
    if app.chat.take_requested_icon_picker() {
        crate::app::input::try_open_icon_picker(app);
    }
    if let Some(request) = app.chat.take_requested_petname() {
        app.banner = Some(apply_petname_request(app, request));
    }
    if let Some(upload) = app.chat.take_requested_url_upload() {
        crate::app::input::trigger_url_image_upload(app, upload.url, upload.room_id);
    }
    if let Some(upload) = app.chat.take_requested_clipboard_image_upload() {
        if app.request_paired_clipboard_image_upload(upload.room_id) {
            app.banner = Some(Banner::success(
                "Reading image from paired CLI clipboard...",
            ));
        } else {
            app.chat.clear_pending_clipboard_image_upload();
            app.banner = Some(Banner::error(
                "No paired CLI with clipboard image support. Update and run `late`.",
            ));
        }
    }
}

/// Apply a parsed `/petname` command to the user's cat and produce the
/// banner to show.
fn apply_petname_request(
    app: &mut App,
    request: crate::app::chat::state::PetnameRequest,
) -> Banner {
    use crate::app::chat::state::PetnameRequest;
    match request {
        PetnameRequest::Show => match app.pet_state.name.as_deref() {
            Some(name) => Banner::success(&format!("🐈 your cat is named {name}")),
            None => {
                Banner::error("your cat doesn't have a name yet — use /petname <name> to set one")
            }
        },
        PetnameRequest::Set(name) => {
            app.pet_state.set_name(Some(name.clone()));
            Banner::success(&format!("🐈 named your cat {name}"))
        }
        PetnameRequest::Clear => {
            app.pet_state.set_name(None);
            Banner::success("cleared your cat's name")
        }
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
    select_message_in_room(app, room_id, delta);
}

pub fn handle_scroll_in_room(app: &mut App, room_id: Uuid, delta: isize) {
    select_message_in_room(app, room_id, delta);
}

fn select_message_in_room(app: &mut App, room_id: Uuid, delta: isize) {
    app.chat.select_message_in_room(room_id, delta);
}

fn switch_room(app: &mut App, delta: isize) {
    if app.chat.move_selection(delta) {
        app.chat.reset_composer();
        app.sync_visible_chat_room();
        app.chat.request_list();
    }
}

fn toggle_selected_room_favorite(app: &mut App) -> bool {
    let Some(room_id) = app.chat.selected_favorite_room_id() else {
        return false;
    };
    let added = app.profile_state.toggle_favorite_room(room_id);
    app.chat
        .set_favorite_room_ids(app.profile_state.profile().favorite_room_ids.clone());
    app.banner = Some(if added {
        Banner::success("Room added to favorites")
    } else {
        Banner::success("Room removed from favorites")
    });
    true
}

fn move_selected_favorite(app: &mut App, delta: isize) -> bool {
    let Some(room_id) = app.chat.selected_favorite_room_id() else {
        return false;
    };
    if !app.chat.favorite_room_ids().contains(&room_id) {
        return false;
    }
    if !app.profile_state.move_favorite_room(room_id, delta) {
        return false;
    }
    app.chat
        .set_favorite_room_ids(app.profile_state.profile().favorite_room_ids.clone());
    true
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
        if let Some(emoji) = leader_reaction_emoji(byte) {
            if let Some(banner) = app
                .chat
                .react_to_selected_message_in_room(room_id, emoji.to_string())
            {
                app.banner = Some(banner);
            }
            return true;
        }
        if byte == b'0' {
            crate::app::input::try_open_reaction_picker(app, room_id);
            return true;
        }
        if matches!(byte, b'f' | b'F') {
            app.chat.open_selected_message_reactions_in_room(room_id);
            return true;
        }
        app.chat.cancel_reaction_leader();
        return true;
    }

    // `d` deletes and keeps the cursor on the adjacent message so you can
    // reap a run of your own messages with repeated presses.
    // `r` enters reply mode and drops the selection.
    // `e` enters edit mode and drops the selection.
    // `Ctrl-P` toggles the selected message's pinned dashboard status.
    // `p` opens a read-only profile modal for the selected author.
    match byte {
        b'f' | b'F' if app.chat.begin_reaction_leader() => return true,
        0x10 => {
            if let Some(b) = app.chat.toggle_pin_selected_message_in_room(room_id) {
                app.banner = Some(b);
                return true;
            }
        }
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
                app.show_sheet_modal = false;
                app.sheet_modal_state.close();
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
        b'\r' | b'\n' if app.chat.open_selected_image_modal_in_room(room_id) => {
            return true;
        }
        b'\r' | b'\n' if app.chat.open_selected_news_modal_in_room(room_id) => {
            return true;
        }
        b'\r' | b'\n' if app.chat.try_jump_to_selected_reply_target_in_room(room_id) => {
            return true;
        }
        _ => {}
    }

    if !matches!(byte, b'j' | b'J' | b'k' | b'K' | 0x04 | 0x15) {
        app.chat.clear_message_selection();
    }

    match byte {
        b'j' | b'J' => {
            select_message_in_room(app, room_id, -1);
            true
        }
        b'k' | b'K' => {
            select_message_in_room(app, room_id, 1);
            true
        }
        0x04 => {
            // Ctrl-D: half-page down. `select_message_in_room` delta is in
            // MESSAGES, not rows, and chat messages wrap to ~3 rows each,
            // so divide terminal height by 6 to feel like half a visible page.
            let step = (app.size.1 / 6).max(1) as isize;
            select_message_in_room(app, room_id, -step);
            true
        }
        0x15 => {
            // Ctrl-U: half-page up. Same rationale as Ctrl-D above.
            let step = (app.size.1 / 6).max(1) as isize;
            select_message_in_room(app, room_id, step);
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
            select_message_in_room(app, room_id, 1);
            true
        }
        b'B' => {
            select_message_in_room(app, room_id, -1);
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
    if app.chat.feeds_selected {
        return super::feeds::input::handle_arrow(app, key);
    }
    if app.chat.news_selected {
        return super::news::input::handle_arrow(app, key);
    }
    if app.chat.showcase_selected {
        return super::showcase::input::handle_arrow(app, key);
    }
    if app.chat.work_selected {
        return super::work::input::handle_arrow(app, key);
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

    if let Some(room_id) = app.chat.selected_bumped_join_room_id() {
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        if matches!(byte, b'\r' | b'\n') {
            let slug = app
                .shop_state
                .active_room_effects()
                .get(&room_id)
                .and_then(|effects| effects.first())
                .and_then(|effect| effect.room_slug.clone());
            if let Some(slug) = slug {
                app.banner = Some(app.chat.join_bumped_public_room(room_id, slug));
            } else {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "Could not join bumped room",
                ));
            }
            return true;
        }
        return false;
    }

    if app.chat.feeds_selected {
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        return super::feeds::input::handle_byte(app, byte);
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

    if app.chat.work_selected {
        if is_next_room_key(byte) {
            switch_room(app, 1);
            return true;
        }
        if is_prev_room_key(byte) {
            switch_room(app, -1);
            return true;
        }
        return super::work::input::handle_byte(app, byte);
    }

    if byte == b'[' && move_selected_favorite(app, -1) {
        return true;
    }
    if byte == b']' && move_selected_favorite(app, 1) {
        return true;
    }

    if handle_message_action(app, byte) {
        return true;
    }

    if matches!(byte, b'f' | b'F') && toggle_selected_room_favorite(app) {
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
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_next_room_key, is_prev_room_key, leader_reaction_emoji};

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
    fn leader_reaction_keys_are_plain_digits_except_custom_zero() {
        assert_eq!(leader_reaction_emoji(b'0'), None);
        assert_eq!(leader_reaction_emoji(b'1'), Some("👍"));
        assert_eq!(leader_reaction_emoji(b'5'), Some("🔥"));
        assert_eq!(leader_reaction_emoji(b'6'), Some("🙌"));
        assert_eq!(leader_reaction_emoji(b'7'), Some("🚀"));
        assert_eq!(leader_reaction_emoji(b'8'), Some("🤔"));
        assert_eq!(leader_reaction_emoji(b'9'), Some("💩"));
        assert_eq!(leader_reaction_emoji(b'!'), None);
    }
}
