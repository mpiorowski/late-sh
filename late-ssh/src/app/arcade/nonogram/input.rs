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
            state.prev_pack();
            return true;
        }
        b']' => {
            state.next_pack();
            return true;
        }
        _ => {}
    }

    if state.is_game_over() {
        return false;
    }

    if byte == b'r' || byte == b'R' {
        state.reset_board();
        return true;
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
        b' ' => {
            state.toggle_cell();
            true
        }
        b'x' | b'X' => {
            state.toggle_mark();
            true
        }
        b'0' | 0x08 | 0x7F | b'c' | b'C' => {
            state.clear_cell();
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    if state.is_game_over() {
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
