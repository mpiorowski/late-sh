use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::State;
use super::ui::WheelCell;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    if state.show_rules() {
        match byte {
            b'!' | 0x1B => state.close_rules(),
            _ => {}
        }
        return true;
    }

    match byte {
        b'!' => {
            state.open_rules();
            true
        }
        b'\r' | b'\n' => state.submit(),
        0x08 | 0x7F => state.pop_letter(),
        b'\t' => state.shuffle(),
        b'a'..=b'z' | b'A'..=b'Z' => state.push_letter(byte as char),
        _ => false,
    }
}

pub fn handle_arrow(_state: &mut State, key: u8) -> bool {
    matches!(key, b'A' | b'B' | b'C' | b'D')
}

/// A left click on a wheel letter types it.
pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    if state.show_rules() {
        return true;
    }
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let letter = match super::ui::hit_test(area, x, y) {
        Some(WheelCell::Hub) => state.hub(),
        Some(WheelCell::Rim(index)) => state.is_loaded().then(|| state.rim()[index]),
        None => None,
    };
    match letter {
        Some(letter) => state.push_letter(letter),
        None => false,
    }
}
