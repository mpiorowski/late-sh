//! Key routing for the Zork Trilogy. The native edition and save menus take
//! parsed keys ([`handle_menu_event`]); a visible game takes raw bytes
//! ([`handle_raw`]), where backtick steps out onto the workspace cycle
//! instead of reaching the story. `App` only calls in.

use super::state::Mode;
use crate::app::common::primitives::{Banner, Screen};
use crate::app::input::{ParsedInput, launcher_key_byte};
use crate::app::state::{App, DOOR_DETACH_KEY};

/// Enter on the Games card: the edition menu, keeping any live game open
/// behind it (Continue on that edition resumes it).
pub fn launch(app: &mut App) {
    if !app.zork_enabled {
        app.banner = Some(Banner::error("Zork Trilogy is currently unavailable."));
        return;
    }
    app.set_screen(Screen::Zork);
    if let Some(state) = app.zork_state.as_mut() {
        state.open_menu();
    }
}

/// Raw bytes on the Zork screen, ahead of the parser. Returns whether Zork
/// took them: everything during the post-exit grace, so keys typed into a
/// game that just closed cannot drive the menu, and everything while a game
/// is visible.
pub fn handle_raw(app: &mut App, data: &[u8]) -> bool {
    let Some(state) = app.zork_state.as_mut() else {
        return false;
    };
    match (state.in_exit_grace(), state.game_visible()) {
        (true, _) => true,
        (false, false) => false,
        (false, true) if data == DOOR_DETACH_KEY => {
            app.detach_door_game();
            true
        }
        (false, true) => {
            state.forward_input(data);
            true
        }
    }
}

/// A lone Esc on the menus, from `input::dispatch_escape` (a lone Esc never
/// reaches the parsed-key path): one level back, and off to the Games hub
/// from the edition list. A visible game never gets here; its Esc is the
/// story's, forwarded raw by [`handle_raw`].
pub fn handle_escape(app: &mut App) {
    let Some(state) = app.zork_state.as_mut() else {
        return;
    };
    if !state.in_exit_grace() && state.back() {
        app.set_screen(Screen::Games);
    }
}

/// One parsed key on the edition and save menus. Returns whether it was
/// consumed; the rest (digits, Tab, global chords) falls through to the app,
/// except under a confirmation, which owns every key until it is answered.
pub fn handle_menu_event(app: &mut App, event: &ParsedInput) -> bool {
    let Some(state) = app.zork_state.as_mut() else {
        return false;
    };
    if state.in_exit_grace() {
        return true;
    }
    match event {
        ParsedInput::Arrow(b'A') => {
            state.step(false);
            true
        }
        ParsedInput::Arrow(b'B') => {
            state.step(true);
            true
        }
        ParsedInput::Byte(b'q') | ParsedInput::Char('q') => {
            if state.back() {
                app.set_screen(Screen::Games);
            }
            true
        }
        _ => match launcher_key_byte(event) {
            Some(byte) if state.menu_key(byte) => true,
            Some(_) | None => matches!(state.mode(), Mode::Confirm(_)),
        },
    }
}
