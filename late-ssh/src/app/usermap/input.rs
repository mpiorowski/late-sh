//! Keys on the active map: pick a country (which frames it), pan, zoom, or
//! ask for fresh numbers. Esc closes, like every other modal; the root's
//! `dispatch_escape` owns the lone Esc that never reaches a modal's own
//! match (see CONTEXT.md §4).

use crate::app::input::{MouseButton, MouseEventKind, ParsedInput};
use crate::app::state::App;

use super::state::MousePress;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q') | ParsedInput::Char('q') => {
            app.usermap.close();
        }
        // All four arrows pan. Binding up/down to the country list instead
        // was the obvious-looking choice and the wrong one: on a map, arrows
        // move the map, and half of them doing something else reads as two of
        // them being broken.
        ParsedInput::Arrow(b'A') => app.usermap.pan(0.0, -1.0),
        ParsedInput::Arrow(b'B') => app.usermap.pan(0.0, 1.0),
        ParsedInput::Arrow(b'C') => app.usermap.pan(1.0, 0.0),
        ParsedInput::Arrow(b'D') => app.usermap.pan(-1.0, 0.0),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') => app.usermap.move_cursor(1),
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') => app.usermap.move_cursor(-1),
        ParsedInput::Byte(b'+' | b'=') | ParsedInput::Char('+' | '=') => {
            app.usermap.zoom_in();
        }
        ParsedInput::Byte(b'-' | b'_') | ParsedInput::Char('-' | '_') => {
            app.usermap.zoom_out();
        }
        ParsedInput::Byte(b'f') | ParsedInput::Char('f') => app.usermap.fit(),
        // Tab walks the slices: online now, here this month, everyone.
        ParsedInput::Byte(b'\t') => {
            let profile = app.profile_state.service().clone();
            app.usermap.cycle_mode(&profile);
        }
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') => {
            let profile = app.profile_state.service().clone();
            app.usermap.refresh(&profile);
        }
        ParsedInput::Mouse(mouse) => {
            let press = match mouse.kind {
                MouseEventKind::ScrollUp => MousePress::ScrollUp,
                MouseEventKind::ScrollDown => MousePress::ScrollDown,
                MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => MousePress::Down,
                MouseEventKind::Drag if mouse.button == Some(MouseButton::Left) => MousePress::Drag,
                MouseEventKind::Up => MousePress::Up,
                _ => MousePress::Other,
            };
            app.usermap.handle_mouse(press, mouse.x, mouse.y);
        }
        _ => {}
    }
}
