//! Keys while the fight scene is open over the street: `a` attacks, `r`
//! runs, Enter closes a finished scene. Everything else is swallowed so
//! the runner does not walk under the panel; a lone Esc never arrives
//! through `handle_event` (the root's `dispatch_escape` calls
//! `handle_escape`: a run while the fight is on, a close once it is over).

use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::state::Command;

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
/// an answer already pending is left to land.
pub fn handle_escape(app: &mut App) {
    let Some(scene) = &app.fight.scene else {
        return;
    };
    match (scene.over, scene.waiting) {
        (true, _) => app.fight.close(),
        (false, true) => {}
        (false, false) => {
            app.fight.request(Command::Run);
        }
    }
}
