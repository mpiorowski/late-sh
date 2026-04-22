use crate::app::{chat, state::App, vote};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    let Some(room_id) = app.dashboard_active_room_id() else {
        return false;
    };
    chat::input::handle_message_arrow_in_room(app, room_id, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    // Dashboard favorite controls — all no-ops at <2 pins and fall
    // through as message-action input in that case.
    //   `[` / `]`   cycle prev / next through pinned favorites
    //   `,`         jump back to the previously-active pin
    //   `g<digit>`  two-key prefix to jump directly to slot 1..9
    let pins_len = app.profile_state.profile().favorite_room_ids.len();

    if app.dashboard_g_prefix_armed {
        app.dashboard_g_prefix_armed = false;
        if (b'1'..=b'9').contains(&byte) {
            app.jump_dashboard_favorite((byte - b'1') as usize);
            app.sync_visible_chat_room();
            return true;
        }
        // Any non-digit disarms and continues through normal handling so
        // the second keystroke isn't silently eaten.
    }

    if byte == b'g' && pins_len >= 2 {
        app.dashboard_g_prefix_armed = true;
        return true;
    }

    if byte == b'[' {
        app.cycle_dashboard_favorite(-1);
        app.sync_visible_chat_room();
        return true;
    }
    if byte == b']' {
        app.cycle_dashboard_favorite(1);
        app.sync_visible_chat_room();
        return true;
    }
    if byte == b',' {
        app.toggle_dashboard_last_favorite();
        app.sync_visible_chat_room();
        return true;
    }

    let active_room_id = app.dashboard_active_room_id();

    if matches!(byte, b'i' | b'I')
        && let Some(room_id) = active_room_id
    {
        app.chat.start_composing_in_room(room_id);
        return true;
    }

    if byte == b'c'
        && let Some(room_id) = active_room_id
        && app.chat.selected_message_body_in_room(room_id).is_some()
    {
        return chat::input::handle_message_action_in_room(app, room_id, byte);
    }

    if vote::input::handle_key(app, byte) {
        return true;
    }

    // Enter is dashboard-specific: copy the CLI install command. Must be
    // checked before delegating because chat compose also binds Enter.
    if matches!(byte, b'\r' | b'\n') {
        app.pending_clipboard =
            Some("curl -fsSL https://cli.late.sh/install.sh | bash".to_string());
        app.banner = Some(crate::app::common::primitives::Banner::success(
            "CLI install command copied!",
        ));
        return true;
    }

    let Some(room_id) = active_room_id else {
        return false;
    };
    chat::input::handle_message_action_in_room(app, room_id, byte)
}
