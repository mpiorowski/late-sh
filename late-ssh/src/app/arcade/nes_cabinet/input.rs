use nes::joypad::JoypadButton;

use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'H' => {
            state.pan_zoom(-1, 0);
            return true;
        }
        b'L' => {
            state.pan_zoom(1, 0);
            return true;
        }
        b'K' => {
            state.pan_zoom(0, -1);
            return true;
        }
        b'J' => {
            state.pan_zoom(0, 1);
            return true;
        }
        _ => {}
    }

    let Some(button) = button_for_key(byte) else {
        match byte {
            b'r' | b'R' => {
                state.reset();
                return true;
            }
            b'z' | b'Z' => {
                state.toggle_zoom();
                return true;
            }
            _ => return false,
        }
    };
    state.press(button);
    true
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    if state.zoomed() {
        match key {
            b'A' => state.pan_zoom(0, -1),
            b'B' => state.pan_zoom(0, 1),
            b'C' => state.pan_zoom(1, 0),
            b'D' => state.pan_zoom(-1, 0),
            _ => return false,
        }
        return true;
    }

    let button = match key {
        b'A' => JoypadButton::UP,
        b'B' => JoypadButton::DOWN,
        b'C' => JoypadButton::RIGHT,
        b'D' => JoypadButton::LEFT,
        _ => return false,
    };
    state.press(button);
    true
}

fn button_for_key(byte: u8) -> Option<JoypadButton> {
    match byte {
        b'w' | b'W' => Some(JoypadButton::UP),
        b's' | b'S' => Some(JoypadButton::DOWN),
        b'a' | b'A' => Some(JoypadButton::LEFT),
        b'd' | b'D' => Some(JoypadButton::RIGHT),
        b'k' | b'b' | b'B' => Some(JoypadButton::B),
        b'l' | b'n' | b'N' => Some(JoypadButton::A),
        b' ' => Some(JoypadButton::SELECT),
        b'\r' | b'\n' => Some(JoypadButton::START),
        _ => None,
    }
}
