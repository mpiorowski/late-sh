// Watch keys. A preview is the Games hub's rail sitting on a live row, so
// the hub keeps the rail keys (Up/Down, j/k, h/l step along the rail, which
// is how one browses the live games) and offers each event here first for
// the one key a preview adds: Enter opens it. An open watch owns the page,
// so the hub hands it every event: F4 hides and shows its chat column
// (`toggle_watch_chat`, reached from the generic F4 arm of `app/input.rs`,
// so it works from the composer too), and while the column is up `i`
// writes, j/k and the arrows select messages, Ctrl+D/Ctrl+U, the page keys
// and the wheel scroll, the message-action keys act on a selected message.
// Backtick hops away on the workspace cycle with the watch kept open
// (`App::away_watches`: the hub shows its cards until the cycle or the live
// strip brings one back). Esc peels a chat selection, then closes the watch
// back to its preview, then leaves the preview (`dispatch_escape`). While
// the composer is open no other key reaches this handler (the composer gate
// in `app/input.rs`).
//
// The player's own chat, over their running game, is shut until F4
// (`is_f4`, read off raw input ahead of the game's passthrough in
// `App::handle_input`) opens it, and while it is open it takes every key
// (`handle_own_chat_event`): `i` writes, j/k and the arrows select messages
// and the message keys act on them, reactions and gilding included, the page
// keys scroll. Esc peels a selection, then closes it (`handle_own_escape`);
// F4 closes it from anywhere, the composer included.

use uuid::Uuid;

use super::state::{LiveGameKey, SpectateGame, WatchMode};
use crate::app::chat::input as chat_input;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::state::App;

/// F4 in an open watch: hide the chat column, with anything of it still in
/// hand, or show it again. Nothing without an open watch.
pub fn toggle_watch_chat(app: &mut App) {
    if let Some(room_id) = app.spectate_chat_room_id() {
        drop_chat_focus(app, room_id);
    }
    if let Some(state) = app.spectate_state.as_mut().filter(|state| state.is_open()) {
        state.toggle_chat();
    }
}

/// A watch chat going off screen: a half-typed line and a picked message in
/// `room_id` would otherwise keep the keys of a room no longer drawn.
pub fn drop_chat_focus(app: &mut App, room_id: Uuid) {
    if app.chat.composer_room_id() == Some(room_id) {
        app.chat.reset_composer();
    }
    if app.chat.selected_message_body_in_room(room_id).is_some() {
        app.chat.clear_message_selection();
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
        ParsedInput::Arrow(key) => match app.spectate_chat_room_id() {
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
    let Some(room_id) = app.spectate_chat_room_id() else {
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
    match app.spectate_chat_room_id() {
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
    if let Some(room_id) = app.spectate_chat_room_id()
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
        .any(|row| row.key == key);
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
        (false, false) => app.start_spectating(key),
    }
    app.open_watch();
    app.set_screen(crate::app::common::primitives::Screen::Games);
    true
}

/// F4 in both encodings a terminal sends it (SS3 `ESC O S`, CSI
/// `ESC [ 14 ~`), matched as a whole input chunk, read off raw input ahead
/// of a running game's passthrough. None of the watchable doors binds it
/// (F1 is the roguelikes' help remap, F3 Zork's suggestion).
pub fn is_f4(data: &[u8]) -> bool {
    data == b"\x1bOS" || data == b"\x1b[14~"
}

/// One raw input chunk on a watchable door whose game this player runs,
/// read ahead of the game's passthrough: `true` when the chat took it (or
/// swallowed it), `false` when the game gets it. F4 opens the chat over the
/// game or closes it, composer open or not, except under a modal the
/// chat's keys opened (Settings on Ctrl+O): closing the chat there would
/// hand the modal's keys to the game, so F4 waits for it to close. While
/// the chat takes the keys they go through the ordinary parser and the
/// chat gates. Zork's F3 is the one exception: taking a watcher's
/// suggestion closes the chat and types it, so Enter is the game's again.
pub fn handle_own_raw(app: &mut App, game: SpectateGame, data: &[u8]) -> bool {
    if is_f4(data) {
        if !app.foreground_overlay_open() {
            toggle_own_chat(app, game);
        }
        return true;
    }
    if !own_chat_takes_keys(app) {
        return false;
    }
    if game == SpectateGame::Zork && crate::app::door::zork::input::is_f3(data) {
        close_own_chat(app);
        return false;
    }
    crate::app::input::handle(app, data);
    true
}

/// F4 in a running game: open the watchers' chat over it, taking every key,
/// or close it and hand the keys back to the game.
pub fn toggle_own_chat(app: &mut App, game: SpectateGame) {
    match app.own_watch_chat_open {
        Some(_) => close_own_chat(app),
        None => {
            app.own_watch_chat_open = Some(game);
            app.own_chat_undrawn.set(true);
        }
    }
}

/// The game whose chat is open on this screen: the door F4 opened it over,
/// while that door is the screen. `None` shut, and on every other screen.
pub fn open_own_chat_game(app: &App) -> Option<SpectateGame> {
    let game = SpectateGame::of_screen(app.screen)?;
    match app.own_watch_chat_open {
        Some(open) if open == game => Some(game),
        Some(_) | None => None,
    }
}

/// Close the player's open chat, with anything of it still in hand
/// (`drop_chat_focus`).
pub fn close_own_chat(app: &mut App) {
    let Some(game) = app.own_watch_chat_open.take() else {
        return;
    };
    if let Some(room_id) = app.own_watch_chat_room_id(game) {
        drop_chat_focus(app, room_id);
    }
}

/// Whether the player's chat is open over their running game on screen and
/// takes every key: open for this screen's door, and drawn by the last
/// frame (`App::own_chat_hit`) or opened since it (`App::own_chat_undrawn`,
/// keys queued behind F4 in one input batch). Keys never go to a column a
/// frame chose not to draw (Zork's menus).
pub fn own_chat_takes_keys(app: &App) -> bool {
    open_own_chat_game(app).is_some()
        && (app.own_chat_undrawn.get() || app.own_chat_hit.get().is_some())
}

/// The room of the player's open chat, once this session is in it: the
/// room every chat gate on a watchable door's screen addresses
/// (`app/input.rs` `embedded_chat_room_id`). `None` while it is closed.
pub fn own_chat_room_id(app: &App) -> Option<Uuid> {
    if !own_chat_takes_keys(app) {
        return None;
    }
    app.own_watch_chat_room_id(SpectateGame::of_screen(app.screen)?)
}

/// One event while the player's chat is open over their game: it is the
/// chat's or nobody's, so nothing reaches the game or a global shortcut.
/// The backtick still hops along the workspace cycle; mouse reports and
/// terminal replies go on to the shared chat gates.
pub fn handle_own_chat_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(byte) => handle_own_chat_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_own_chat_key(app, *ch as u8),
        ParsedInput::Arrow(key) => {
            if let Some(room_id) = own_chat_room_id(app) {
                chat_input::handle_message_arrow_in_room(app, room_id, *key);
            }
            true
        }
        ParsedInput::PageUp => {
            scroll_own_chat(app, page_step(app));
            true
        }
        ParsedInput::PageDown => {
            scroll_own_chat(app, -page_step(app));
            true
        }
        ParsedInput::Mouse(_)
        | ParsedInput::FocusGained
        | ParsedInput::FocusLost
        | ParsedInput::TerminalVersion(_)
        | ParsedInput::TerminalCapabilities(_)
        | ParsedInput::DeviceAttributes(_) => false,
        _ => true,
    }
}

fn handle_own_chat_key(app: &mut App, byte: u8) -> bool {
    if byte == b'`' {
        crate::app::workspace::cycle::cycle_game_workspace(app);
        return true;
    }
    let Some(room_id) = own_chat_room_id(app) else {
        return true;
    };
    // `i`, j/k, Ctrl+D/Ctrl+U and a reaction leader's follow-up always go to
    // the chat; the message-action keys while a message is selected.
    if chat_input::chat_priority_key(app, byte) || chat_input::selected_chat_key(app, room_id, byte)
    {
        chat_input::handle_message_action_in_room(app, room_id, byte);
    }
    true
}

fn scroll_own_chat(app: &mut App, delta: isize) {
    if let Some(room_id) = own_chat_room_id(app) {
        chat_input::handle_scroll_in_room(app, room_id, delta);
    }
}

/// Esc in the player's open chat: drop a selected message first, then close
/// the chat back to the game.
pub fn handle_own_escape(app: &mut App) {
    if let Some(room_id) = own_chat_room_id(app)
        && app.chat.selected_message_body_in_room(room_id).is_some()
    {
        app.chat.clear_message_selection();
        return;
    }
    close_own_chat(app);
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
