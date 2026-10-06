// Watch-view keys. A watch is the Games hub's rail sitting on a live row, so
// the hub keeps the selection keys (Up/Down, j/k, h/l step along the rail,
// which is how one switches games) and offers every event here first for
// what belongs to the docked watch chat: `i` or Enter writes, Ctrl+D/Ctrl+U,
// the page keys and the wheel scroll it, and the message-action keys act
// while a message is selected (a click selects one; j/k are the rail's).
// Esc peels a chat selection, then leaves the watch (`dispatch_escape`).
// While the composer is open no key reaches this handler at all (the
// composer gate in `app/input.rs`).

use uuid::Uuid;

use super::state::SpectateGame;
use super::ui::{ChatDock, chat_dock};
use crate::app::chat::input as chat_input;
use crate::app::common::primitives::Banner;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::state::App;

/// The chat room the watch view has on screen: the watched player's room,
/// once this session is in it and the terminal has room to dock the pane.
/// `None` when this session is not watching.
pub fn chat_room_id(app: &App) -> Option<Uuid> {
    let room_id = app.spectate_chat_room_id()?;
    let pane =
        crate::app::door::hub::ui::watch_pane_area(crate::app::input::app_content_area(app))?;
    match chat_dock(pane) {
        ChatDock::Right | ChatDock::Below => Some(room_id),
        ChatDock::Hidden => None,
    }
}

/// Offer one event to the watch chat. Returns whether it was consumed; the
/// hub handles what is left (the rail keys).
pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.spectate_state.is_none() {
        return false;
    }
    match event {
        ParsedInput::Byte(byte) => handle_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_key(app, *ch as u8),
        ParsedInput::PageUp => scroll_chat(app, page_step(app)),
        ParsedInput::PageDown => scroll_chat(app, -page_step(app)),
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => scroll_chat(app, 1),
            MouseEventKind::ScrollDown => scroll_chat(app, -1),
            _ => false,
        },
        _ => false,
    }
}

fn handle_key(app: &mut App, byte: u8) -> bool {
    let Some(room_id) = chat_room_id(app) else {
        return false;
    };
    // A reaction leader's follow-up key, and the keys that act on a selected
    // message, go to the chat before anything else can claim them.
    if app.chat.is_reaction_leader_active() || chat_input::selected_chat_key(app, room_id, byte) {
        return chat_input::handle_message_action_in_room(app, room_id, byte);
    }
    match byte {
        // Enter writes too: with no card under the selection it has nothing
        // to launch.
        b'i' | b'I' | b'\r' => chat_input::handle_message_action_in_room(app, room_id, b'i'),
        // Ctrl+D / Ctrl+U: half a page of chat.
        0x04 | 0x15 => chat_input::handle_message_action_in_room(app, room_id, byte),
        _ => false,
    }
}

/// Scroll the docked chat. Consumed even with no pane on screen: the watch
/// owns the pane beside the rail, and no landing is there to scroll.
fn scroll_chat(app: &mut App, delta: isize) -> bool {
    if let Some(room_id) = chat_room_id(app) {
        chat_input::handle_scroll_in_room(app, room_id, delta);
    }
    true
}

fn page_step(app: &App) -> isize {
    (app.size.1 / 6).max(1) as isize
}

/// Esc in the watch view: drop a selected chat message first, then leave.
pub fn handle_escape(app: &mut App) {
    if let Some(room_id) = chat_room_id(app)
        && app.chat.selected_message_body_in_room(room_id).is_some()
    {
        app.chat.clear_message_selection();
        return;
    }
    app.stop_spectating();
}

/// The `s` key on a watchable door's card: jump the rail to that door's
/// longest-running live game.
pub fn watch_first(app: &mut App, game: SpectateGame) {
    let roster = app.live_games.roster(game);
    match roster.first() {
        Some(first) => app.start_spectating(game, first.playname.clone()),
        None => {
            app.banner = Some(Banner::error(&format!(
                "Nobody is playing {} right now.",
                game.label()
            )));
        }
    }
}
