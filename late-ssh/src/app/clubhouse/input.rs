//! Clubhouse input: roguelike walking plus the thin bits that make the room
//! social. Plain arrows/hjkl move your avatar; `i` (or Enter in the open)
//! opens the #lounge composer, and what you send floats over your head as a
//! speech bubble; `w` waves and `x` dances for everyone; `t` at the bar
//! pours a `@bartender ` mention into the composer. Enter next to a landmark
//! prop follows its signpost: the arcade cabinet, the heavy door, the poker
//! table, and the easel jump to their app pages (2/3/4/5), the jukebox opens
//! the Music Booth, and the dog gets petted where everyone can see it.
//! Returns `false` for anything it does not own so global keys (numbers,
//! Tab, `q`, `?`, `v` music chords, ...) keep working, and returns `false`
//! outright while composing so the shared composer pipeline gets the bytes.

use crate::app::common::primitives::Screen;
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::lobby::Emote;
use super::map::Interactive;

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // While typing, the global composer pipeline owns every byte.
    if app.chat.is_composing() {
        return false;
    }
    // Chat overlays opened elsewhere are handled by the shared overlay path.
    if app.chat.has_overlay() {
        return false;
    }

    if let Some(byte) = event_byte(event) {
        // A tutorial popup wants Enter before anything else; Esc resolves
        // through `dispatch_escape` in `app::input`, which owns the
        // tutorial-skip arm.
        if matches!(byte, b'\r' | b'\n') && app.clubhouse.tutorial_capturing_keys() {
            if app.clubhouse.tutorial_advance() {
                app.persist_clubhouse_tutorial_done();
            }
            return true;
        }

        match byte {
            b'i' | b'I' => {
                if let Some(lounge_id) = app.chat.lounge_room_id() {
                    app.chat.start_composing_in_room(lounge_id);
                }
                return true;
            }
            b'w' | b'W' => {
                app.clubhouse.emote(Emote::Wave);
                return true;
            }
            b'x' | b'X' => {
                app.clubhouse.emote(Emote::Dance);
                return true;
            }
            b't' | b'T' if app.clubhouse.nearby() == Some(Interactive::Bartender) => {
                if let Some(lounge_id) = app.chat.lounge_room_id() {
                    app.chat.insert_mention_in_room(lounge_id, "bartender");
                }
                return true;
            }
            b'\r' | b'\n' => {
                match app.clubhouse.nearby() {
                    Some(Interactive::Jukebox) => {
                        let submit_enabled = app.audio.booth_submit_enabled();
                        app.booth_modal_state.open(submit_enabled);
                    }
                    Some(Interactive::Bartender) => {
                        if let Some(lounge_id) = app.chat.lounge_room_id() {
                            app.chat.insert_mention_in_room(lounge_id, "bartender");
                        }
                    }
                    Some(Interactive::Dog) => app.clubhouse.pet_dog(),
                    // The landmark props are signposts: Enter walks through.
                    Some(Interactive::Arcade) => app.set_screen(Screen::Arcade),
                    Some(Interactive::Doors) => app.set_screen(Screen::Games),
                    Some(Interactive::Poker) => app.set_screen(Screen::Rooms),
                    Some(Interactive::Easel) => app.set_screen(Screen::Artboard),
                    _ => {
                        if let Some(lounge_id) = app.chat.lounge_room_id() {
                            app.chat.start_composing_in_room(lounge_id);
                        }
                    }
                }
                return true;
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
    app.clubhouse.walk(dx, dy);
    if app.clubhouse.tutorial_reached_bar() {
        app.send_clubhouse_bartender_greeting();
    }
    true
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
