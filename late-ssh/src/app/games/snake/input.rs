

use super::state::{State};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'r' | b'R' | b'n' | b'N' => {
            state.reset_game();
            true
        }
        b'p' | b'P' => {
            state.toggle_pause();
            true
        }
        b'k' | b'K' | b'w' | b'W' |
        b'j' | b'J' | b's' | b'S' |
        b'h' | b'H' | b'a' | b'A' |
        b'l' | b'L' | b'd' | b'D' => {
            state.input_queue.insert(0, byte);
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' | b'B' | b'C' | b'D' => {
            state.input_queue.insert(0, key);
            true
        }
        _ => false,
    }
}
