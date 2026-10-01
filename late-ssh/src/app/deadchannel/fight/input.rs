//! Keys while the fight scene is open over the street: `a` attacks, `r`
//! runs, Enter closes a finished scene. Everything else is swallowed so
//! the runner does not walk under the panel; a lone Esc never arrives
//! through `handle_event` (the root's `dispatch_escape` calls
//! `handle_escape`: a run while the fight is on, a close once it is over).
//!
//! Before it, the picker (`handle_picker`): `f` steps in against the glyph
//! of your level, `g` against the one below (its neighbour on the keys),
//! up and down move the cursor and Enter takes it. Esc closes it from the
//! root, the same way.

use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::state::{Command, Pick};

/// Keys while the picker is open. The row decides what a step in meets;
/// the picker only keeps `g` from asking for a glyph under the flicker.
pub fn handle_picker(app: &mut App, event: &ParsedInput) -> bool {
    let Some(picker) = &app.fight.picker else {
        return false;
    };
    let cursor = picker.cursor;
    let below = app
        .fight
        .sheet
        .as_ref()
        .is_some_and(|sheet| sheet.level > 1);
    match event {
        ParsedInput::Byte(b'f') | ParsedInput::Char('f') => {
            app.fight.step_in(Pick::Fair);
            true
        }
        ParsedInput::Byte(b'g') | ParsedInput::Char('g') => {
            if below {
                app.fight.step_in(Pick::Lower);
            }
            true
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.fight.step_in(cursor);
            true
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => {
            app.fight.pick_up();
            true
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => {
            app.fight.pick_down();
            true
        }
        // Digits, Tab, `q` stay global, as in the scene.
        ParsedInput::Byte(b'0'..=b'9')
        | ParsedInput::Byte(b'\t')
        | ParsedInput::Byte(b'q')
        | ParsedInput::Byte(b'?') => false,
        _ => true,
    }
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let Some(scene) = &app.fight.scene else {
        return false;
    };
    let over = scene.over;
    let waiting = scene.waiting;
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            if over {
                app.fight.close();
            }
            true
        }
        ParsedInput::Byte(b'a') | ParsedInput::Char('a') if !over && !waiting => {
            app.fight.request(Command::Attack);
            true
        }
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') if !over && !waiting => {
            app.fight.request(Command::Run);
            true
        }
        // Digits, Tab, `q` stay global (`?` is the guide's, taken in
        // `city/input.rs` before the scene sees it); the rest is the scene's.
        ParsedInput::Byte(b'0'..=b'9')
        | ParsedInput::Byte(b'\t')
        | ParsedInput::Byte(b'q')
        | ParsedInput::Byte(b'?') => false,
        _ => true,
    }
}

/// Esc over the scene. There is no stepping out of a fight: while it is on,
/// Esc is the run, dice and free swing included; a finished scene closes;
/// an answer already pending is left to land. A scene whose last answer
/// was the service failing closes too, the fight left on the row: an
/// outage is not a fight to be trapped in.
pub fn handle_escape(app: &mut App) {
    let Some(scene) = &app.fight.scene else {
        return;
    };
    match (scene.over, scene.waiting, scene.failed) {
        (true, _, _) => app.fight.close(),
        (false, true, _) => {}
        (false, false, true) => app.fight.close(),
        (false, false, false) => {
            app.fight.request(Command::Run);
        }
    }
}
