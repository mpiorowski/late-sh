// Watch keys. A preview is the Games hub's rail sitting on a live row, so
// the hub keeps the rail keys (Up/Down, j/k, h/l step along the rail, which
// is how one browses the live games) and offers each event here first for
// the one key a preview adds: Enter opens it. An open watch owns the page,
// so the hub hands it every event: `i` writes, j/k and the arrows select
// messages, Ctrl+D/Ctrl+U, the page keys and the wheel scroll, the
// message-action keys act on a selected message, and backtick hops away on
// the workspace cycle with the watch kept open (`App::away_watches`: the hub
// shows its cards until the cycle or the live strip brings one back). Esc
// peels a chat selection, then closes the watch back to its preview, then
// leaves the preview
// (`dispatch_escape`). While the composer is open no key reaches this
// handler at all (the composer gate in `app/input.rs`).
//
// The player's own pane, beside their running game, has two inputs of its
// own, read ahead of the game's passthrough in `App::handle_input`: F2 or a
// click on the pane (`wants_own_chat`) opens the chat composer in their
// watchers' room. From there the composer owns the keys until Enter sends
// or Esc discards, and the game has them again.

use ratatui::layout::Rect;
use uuid::Uuid;

use super::state::{LiveGameKey, WatchMode};
use super::ui::{ChatDock, chat_dock};
use crate::app::chat::input as chat_input;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::state::App;

/// The chat room the open watch has on screen: the watched player's room,
/// once this session is in it and the terminal has room to dock the pane.
/// `None` for a preview, or when this session is not watching.
pub fn chat_room_id(app: &App) -> Option<Uuid> {
    let room_id = app.spectate_chat_room_id()?;
    let game = app.spectate_state.as_ref()?.game();
    match chat_dock(crate::app::input::app_content_area(app), game) {
        ChatDock::Right | ChatDock::Below => Some(room_id),
        ChatDock::Hidden => None,
    }
}

/// The watch's mode, `None` when this session is not watching.
pub fn mode(app: &App) -> Option<WatchMode> {
    app.spectate_state.as_ref().map(|state| state.mode())
}

/// Offer one event to a preview. Returns whether it was consumed; the hub
/// handles what is left (the rail keys).
pub fn handle_preview_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(b'\r') => {
            app.open_watch();
            true
        }
        _ => false,
    }
}

/// One event in an open watch. Returns whether it was consumed.
pub fn handle_open_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(byte) => handle_open_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_open_key(app, *ch as u8),
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

fn handle_open_key(app: &mut App, byte: u8) -> bool {
    if byte == b'`' {
        return crate::app::workspace::cycle::cycle_game_workspace(app);
    }
    let Some(room_id) = chat_room_id(app) else {
        return false;
    };
    // `i`, j/k, Ctrl+D/Ctrl+U and a reaction leader's follow-up always go to
    // the chat; the message-action keys while a message is selected.
    if chat_input::chat_priority_key(app, byte) || chat_input::selected_chat_key(app, room_id, byte)
    {
        return chat_input::handle_message_action_in_room(app, room_id, byte);
    }
    false
}

fn scroll_chat(app: &mut App, delta: isize) -> bool {
    match chat_room_id(app) {
        Some(room_id) => {
            chat_input::handle_scroll_in_room(app, room_id, delta);
            true
        }
        None => false,
    }
}

fn page_step(app: &App) -> isize {
    (app.size.1 / 6).max(1) as isize
}

/// Esc in a watch: drop a selected chat message first, then close an open
/// watch back to its preview, then leave the preview.
pub fn handle_escape(app: &mut App) {
    if let Some(room_id) = chat_room_id(app)
        && app.chat.selected_message_body_in_room(room_id).is_some()
    {
        app.chat.clear_message_selection();
        return;
    }
    match mode(app) {
        Some(WatchMode::Open) => app.close_watch(),
        Some(WatchMode::Preview) | None => app.stop_spectating(),
    }
}

/// `o` or a click on the live strip while it shows a live game: open the
/// watch on it, on the Games screen. A watch already on that game, on the
/// hub or stepped away from, keeps its stream. `false` when the game is no
/// longer listed.
pub fn open_live_game(app: &mut App, key: LiveGameKey) -> bool {
    let listed = app
        .live_games
        .roster(key.game())
        .iter()
        .any(|game| game.playname == key.playname());
    if !listed {
        return false;
    }
    let away = app.away_watches.iter().any(|state| state.key() == key);
    let on_hub = app
        .spectate_state
        .as_ref()
        .is_some_and(|state| state.key() == key);
    match (away, on_hub) {
        (true, _) => app.resume_watch(key),
        (false, true) => {}
        (false, false) => app.start_spectating(key.game(), key.playname().to_string()),
    }
    app.open_watch();
    app.set_screen(crate::app::common::primitives::Screen::Games);
    true
}

/// F2 in both encodings a terminal sends it (SS3 `ESC O Q`, CSI
/// `ESC [ 12 ~`), matched as a whole input chunk like the doors' F1 remap.
/// None of the three roguelikes binds it.
pub fn is_f2(data: &[u8]) -> bool {
    data == b"\x1bOQ" || data == b"\x1b[12~"
}

/// Whether a chunk of raw input, read ahead of a running game's
/// passthrough, asks for the player's own watch-chat composer: F2, or a left
/// click on `pane` (the pane as the last frame drew it). Everything else is
/// the game's.
pub fn wants_own_chat(data: &[u8], pane: Rect) -> bool {
    if is_f2(data) {
        return true;
    }
    match sgr_left_press(data) {
        Some((x, y)) => {
            x >= pane.x
                && x < pane.x.saturating_add(pane.width)
                && y >= pane.y
                && y < pane.y.saturating_add(pane.height)
        }
        None => false,
    }
}

/// The first SGR left-button press in `data` (`ESC [ < 0 ; x ; y M`, with
/// any modifier), as 0-based cells. Motion, drags, releases and the wheel
/// are not presses. The games strip every mouse report, so nothing read
/// here is taken from them.
fn sgr_left_press(data: &[u8]) -> Option<(u16, u16)> {
    let mut rest = data;
    while let Some(start) = rest.windows(3).position(|window| window == b"\x1b[<") {
        let seq = &rest[start + 3..];
        let end = seq.iter().position(|byte| *byte == b'M' || *byte == b'm')?;
        let (params, action) = (&seq[..end], seq[end]);
        rest = &seq[end + 1..];
        if action != b'M' {
            continue;
        }
        let mut fields = params
            .split(|byte| *byte == b';')
            .map(|field| std::str::from_utf8(field).ok()?.parse::<u16>().ok());
        let (Some(Some(button)), Some(Some(x)), Some(Some(y))) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        // The low bits name the button (0 is left); 32 marks motion, 64 the
        // wheel. Shift, Alt and Ctrl (4, 8, 16) still click.
        if button & 0b0110_0011 != 0 {
            continue;
        }
        return Some((x.checked_sub(1)?, y.checked_sub(1)?));
    }
    None
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
