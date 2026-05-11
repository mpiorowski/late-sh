use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'n' | b'N' => {
            state.new_personal_board();
            return true;
        }
        b'p' | b'P' => {
            state.show_personal();
            return true;
        }
        b'd' | b'D' => {
            state.show_daily();
            return true;
        }
        b'[' => {
            state.prev_difficulty();
            return true;
        }
        b']' => {
            state.next_difficulty();
            return true;
        }
        _ => {}
    }

    if state.is_game_over {
        return false;
    }

    match byte {
        b'k' | b'K' => {
            state.move_cursor(-1, 0);
            true
        }
        b'j' | b'J' => {
            state.move_cursor(1, 0);
            true
        }
        b'h' | b'H' => {
            state.move_cursor(0, -1);
            true
        }
        b'l' | b'L' => {
            state.move_cursor(0, 1);
            true
        }
        b' ' | b'\r' | b'\n' => {
            state.reveal();
            true
        }
        b'f' | b'F' | b'x' | b'X' => {
            state.toggle_flag();
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
            state.move_cursor(-1, 0);
            true
        }
        b'B' => {
            state.move_cursor(1, 0);
            true
        }
        b'C' => {
            state.move_cursor(0, 1);
            true
        }
        b'D' => {
            state.move_cursor(0, -1);
            true
        }
        _ => false,
    }
}
