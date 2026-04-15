use crate::app::{chat, state::App, vote};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    let Some(room_id) = app.chat.general_room_id() else {
        return false;
    };
    chat::input::handle_message_arrow_in_room(app, room_id, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
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

    let Some(room_id) = app.chat.general_room_id() else {
        return false;
    };
    chat::input::handle_message_action_in_room(app, room_id, byte)
}
