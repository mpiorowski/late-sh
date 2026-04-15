use crate::app::{chat, state::App, vote};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    // The dashboard card always shows #general, so treat cursor movement as
    // if general is the active room. Delegation to the shared chat helper
    // means new message-nav behaviors land once.
    app.chat.select_general_room();
    chat::input::handle_message_arrow(app, key)
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

    // Every message action (d/r/e/j/k/g/i/Ctrl-D/Ctrl-U) lives in the shared
    // chat helper. Pin general as the active room first so the cursor and
    // actions operate on the dashboard feed.
    app.chat.select_general_room();
    chat::input::handle_message_action(app, byte)
}
