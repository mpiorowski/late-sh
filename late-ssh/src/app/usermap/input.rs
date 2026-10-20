//! Keys on the Map section: pick a country (which frames it), pan, zoom,
//! walk the slices, or ask for fresh numbers. The host page (`app/directory`)
//! calls these for its Map section and keeps what is the page's: `[`/`]`
//! between sections, the page keys, the section row.

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};
use crate::app::state::App;

use super::state::MousePress;

pub(crate) fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' => app.usermap.walk(1),
        b'k' => app.usermap.walk(-1),
        b'+' | b'=' => app.usermap.zoom_in(),
        b'-' | b'_' => app.usermap.zoom_out(),
        b'f' => app.usermap.fit(),
        // `m` walks the slices: online now, here this month, everyone.
        b'm' => {
            let profile = app.profile_state.service().clone();
            app.usermap.cycle_mode(&profile);
        }
        b'r' => {
            let profile = app.profile_state.service().clone();
            app.usermap.refresh(&profile);
        }
        _ => return false,
    }
    true
}

/// All four arrows pan. Binding up/down to the country list instead was the
/// obvious-looking choice and the wrong one: on a map, arrows move the map,
/// and half of them doing something else reads as two of them being broken.
pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => app.usermap.pan(0.0, -1.0),
        b'B' => app.usermap.pan(0.0, 1.0),
        b'C' => app.usermap.pan(1.0, 0.0),
        b'D' => app.usermap.pan(-1.0, 0.0),
        _ => return false,
    }
    true
}

/// True when the press landed on the map or the country list.
pub(crate) fn handle_mouse(app: &mut App, mouse: MouseEvent) -> bool {
    let press = match mouse.kind {
        MouseEventKind::ScrollUp => MousePress::ScrollUp,
        MouseEventKind::ScrollDown => MousePress::ScrollDown,
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => MousePress::Down,
        MouseEventKind::Drag if mouse.button == Some(MouseButton::Left) => MousePress::Drag,
        MouseEventKind::Up => MousePress::Up,
        _ => MousePress::Other,
    };
    app.usermap.handle_mouse(press, mouse.x, mouse.y)
}
