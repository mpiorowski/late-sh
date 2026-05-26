use nes::joypad::JoypadButton;

use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    let Some(button) = button_for_key(byte) else {
        match byte {
            b'[' | b',' => {
                state.prev_rom();
                return true;
            }
            b']' | b'.' => {
                state.next_rom();
                return true;
            }
            b'r' | b'R' => {
                state.reset();
                return true;
            }
            _ => return false,
        }
    };
    state.press(button);
    true
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
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
        b'k' | b'K' | b'b' | b'B' => Some(JoypadButton::B),
        b'l' | b'L' | b'n' | b'N' => Some(JoypadButton::A),
        b' ' => Some(JoypadButton::SELECT),
        b'\r' | b'\n' => Some(JoypadButton::START),
        _ => None,
    }
}
