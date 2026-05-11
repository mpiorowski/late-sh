use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    if state.is_game_over {
        if byte == b'r' || byte == b'R' {
            state.reset();
            return true;
        }
        return false;
    }

    match byte {
        b'k' | b'K' => {
            state.move_up();
            true
        }
        b'j' | b'J' => {
            state.move_down();
            true
        }
        b'h' | b'H' => {
            state.move_left();
            true
        }
        b'l' | b'L' => {
            state.move_right();
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    if state.is_game_over {
        return matches!(key, b'A' | b'B' | b'C' | b'D');
    }

    match key {
        b'A' => {
            state.move_up();
            true
        }
        b'B' => {
            state.move_down();
            true
        }
        b'C' => {
            state.move_right();
            true
        }
        b'D' => {
            state.move_left();
            true
        }
        _ => false,
    }
}
