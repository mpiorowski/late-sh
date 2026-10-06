// Watch-view keys, routed here by the Games hub while this session spectates.
// The docked watch chat owns its keys first, the way a house table's chat
// does: `i`/`j`/`k`/Ctrl+D/Ctrl+U always, the message-action keys while a
// message is selected, Up/Down, the page keys and the wheel. Left/right (or
// h/l) switch to the previous/next live game; Esc peels a chat selection,
// then leaves (`dispatch_escape`). Everything else returns `false` so the
// global keys (numbers, Tab, `q`, ...) keep working; leaving the hub ends
// the watch. While the composer is open no key reaches this handler at all
// (the composer gate in `app/input.rs`).

use uuid::Uuid;

use super::state::{SpectateGame, step_target};
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
    match chat_dock(crate::app::input::app_content_area(app)) {
        ChatDock::Right | ChatDock::Below => Some(room_id),
        ChatDock::Hidden => None,
    }
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.spectate_state.is_none() {
        return false;
    }
    match event {
        ParsedInput::Byte(byte) => handle_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_key(app, *ch as u8),
        ParsedInput::Arrow(b'C') => {
            switch_game(app, true);
            true
        }
        ParsedInput::Arrow(b'D') => {
            switch_game(app, false);
            true
        }
        ParsedInput::Arrow(key) => match chat_room_id(app) {
            Some(room_id) => chat_input::handle_message_arrow_in_room(app, room_id, *key),
            None => false,
        },
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
    if let Some(room_id) = chat_room_id(app) {
        if chat_input::chat_priority_key(app, byte)
            && chat_input::handle_message_action_in_room(app, room_id, byte)
        {
            return true;
        }
        if chat_input::selected_chat_key(app, room_id, byte)
            && chat_input::handle_message_action_in_room(app, room_id, byte)
        {
            return true;
        }
    }
    match byte {
        b'l' | b'L' => {
            switch_game(app, true);
            true
        }
        b'h' | b'H' => {
            switch_game(app, false);
            true
        }
        _ => false,
    }
}

/// Step to the next or previous live game. A no-op with nobody else playing.
fn switch_game(app: &mut App, forward: bool) {
    let Some(state) = app.spectate_state.as_ref() else {
        return;
    };
    let game = state.game();
    let roster = app.live_games.roster(game);
    if let Some(next) = step_target(&roster, state.playname(), forward) {
        app.start_spectating(game, next.to_string());
    }
}

/// Scroll the docked chat. Consumed even with no pane on screen: the watch
/// view owns the hub, and the hidden landing behind it must not scroll.
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

/// The hub's `s` key: start watching `game`'s longest-running live game.
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
