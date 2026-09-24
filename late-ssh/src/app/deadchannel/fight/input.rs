//! Keys while the fight scene is open over the street: `a` attacks, `r`
//! runs, Enter closes a finished scene. Everything else is swallowed so
//! the runner does not walk under the panel; a lone Esc never arrives
//! here (the root's `dispatch_escape` closes the scene).

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
        // Digits, Tab, `q`, `?` stay global; the rest is the scene's.
        ParsedInput::Byte(b'0'..=b'9')
        | ParsedInput::Byte(b'\t')
        | ParsedInput::Byte(b'q')
        | ParsedInput::Byte(b'?') => false,
        _ => true,
    }
}
