use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'r' | b'R' | b'n' | b'N' => {
            state.reset();
            true
        }
        b'p' | b'P' => {
            state.toggle_pause();
            true
        }
        b'k' | b'K' | b'w' | b'W' | b'x' | b'X' => state.rotate_cw(),
        b'j' | b'J' | b's' | b'S' => state.soft_drop(),
        b'h' | b'H' | b'a' | b'A' => state.move_left(),
        b'l' | b'L' | b'd' | b'D' => state.move_right(),
        b' ' => state.hard_drop(),
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => state.rotate_cw(),
        b'B' => state.soft_drop(),
        b'C' => state.move_right(),
        b'D' => state.move_left(),
        _ => false,
    }
}
