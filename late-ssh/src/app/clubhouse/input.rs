//! Clubhouse input: roguelike walking plus a thin routing layer into the
//! embedded #lounge chat. Plain arrows/hjkl move your avatar; `i` (or Enter)
//! opens the composer; Shift+J/K walk the message selection like the Rooms
//! embedded chat; `t` at the bar pours a `@bartender ` mention into the
//! composer. Enter next to a landmark prop follows its signpost: the arcade
//! cabinet, the heavy door, the poker table, and the easel jump to their app
//! pages (2/3/4/5), and the jukebox opens the Music Booth. Returns `false`
//! for anything it does not own so global keys (numbers, Tab, `q`, `?`, `v`
//! music chords, ...) keep working, and returns `false` outright while
//! composing so the shared composer pipeline gets the bytes.

use crate::app::common::primitives::Screen;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::state::App;

use super::map::Interactive;

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // While typing, the global composer pipeline owns every byte.
    if app.chat.is_composing() {
        return false;
    }
    // Chat overlays (/active roster, member list, ...) are handled by the
    // shared overlay path in `app::input`.
    if app.chat.has_overlay() {
        return false;
    }

    let Some(lounge_id) = app.chat.lounge_room_id() else {
        return handle_walk(app, event);
    };

    // Reaction leader (emoji picker digits) gets priority, like Rooms.
    if app.chat.is_reaction_leader_active()
        && let Some(byte) = event_byte(event)
    {
        return crate::app::chat::input::handle_message_action_in_room(app, lounge_id, byte);
    }

    if let ParsedInput::Mouse(mouse) = event {
        let delta = match mouse.kind {
            MouseEventKind::ScrollUp => 1,
            MouseEventKind::ScrollDown => -1,
            _ => return false,
        };
        crate::app::chat::input::handle_scroll_in_room(app, lounge_id, delta);
        return true;
    }

    if let Some(byte) = event_byte(event) {
        match byte {
            // Esc clears a selected message before anything global sees it.
            // NOTE: standalone Esc never lands here — it resolves through
            // `dispatch_escape` in `app::input`, which owns the Clubhouse
            // deselect/overlay/reaction-cancel arms.
            // Shift+J/K and Ctrl+D/U drive the lounge message selection.
            b'J' | b'K' | 0x04 | 0x15 => {
                return crate::app::chat::input::handle_message_action_in_room(
                    app, lounge_id, byte,
                );
            }
            b'i' | b'I' => {
                app.chat.start_composing_in_room(lounge_id);
                return true;
            }
            b't' | b'T' if app.clubhouse.nearby() == Some(Interactive::Bartender) => {
                app.chat.insert_mention_in_room(lounge_id, "bartender");
                return true;
            }
            b'\r' | b'\n' => {
                if app.chat.selected_message_body_in_room(lounge_id).is_some() {
                    return crate::app::chat::input::handle_message_action_in_room(
                        app, lounge_id, byte,
                    );
                }
                match app.clubhouse.nearby() {
                    Some(Interactive::Jukebox) => {
                        let submit_enabled = app.audio.booth_submit_enabled();
                        app.booth_modal_state.open(submit_enabled);
                    }
                    Some(Interactive::Bartender) => {
                        app.chat.insert_mention_in_room(lounge_id, "bartender");
                    }
                    // The landmark props are signposts: Enter walks through.
                    Some(Interactive::Arcade) => app.set_screen(Screen::Arcade),
                    Some(Interactive::Doors) => app.set_screen(Screen::Games),
                    Some(Interactive::Poker) => app.set_screen(Screen::Rooms),
                    Some(Interactive::Easel) => app.set_screen(Screen::Artboard),
                    _ => app.chat.start_composing_in_room(lounge_id),
                }
                return true;
            }
            // Message actions while one is selected (reply, edit, pin, ...).
            _ if app.chat.selected_message_body_in_room(lounge_id).is_some()
                && is_selected_message_key(byte) =>
            {
                return crate::app::chat::input::handle_message_action_in_room(
                    app, lounge_id, byte,
                );
            }
            _ => {}
        }
    }

    handle_walk(app, event)
}

/// Arrow keys and lowercase hjkl move the avatar. Consumes the key even when
/// the step is blocked so walking into a wall doesn't trigger global actions.
fn handle_walk(app: &mut App, event: &ParsedInput) -> bool {
    let (dx, dy) = match event {
        ParsedInput::Arrow(b'A') => (0, -1),
        ParsedInput::Arrow(b'B') => (0, 1),
        ParsedInput::Arrow(b'C') => (1, 0),
        ParsedInput::Arrow(b'D') => (-1, 0),
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') => (0, -1),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') => (0, 1),
        ParsedInput::Byte(b'l') | ParsedInput::Char('l') => (1, 0),
        ParsedInput::Byte(b'h') | ParsedInput::Char('h') => (-1, 0),
        _ => return false,
    };
    // A consumed movement key also cancels a pending `v` music chord, like
    // any locally-handled key would on the chat screens.
    app.music_prefix_armed = false;
    app.clubhouse.try_move(dx, dy);
    true
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}

/// Mirror of the Rooms embedded-chat selected-message key set.
fn is_selected_message_key(byte: u8) -> bool {
    matches!(
        byte,
        b'd' | b'D' | b'r' | b'R' | b'e' | b'E' | b'p' | b'c' | b'f' | b'F' | b'g' | 0x10
    )
}
