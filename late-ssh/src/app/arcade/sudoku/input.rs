use super::state::{ResetKind, State};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'n' | b'N' => {
            if state.request_reset(ResetKind::NewBoard) {
                state.new_personal_board();
            }
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

    if byte == b'r' || byte == b'R' {
        if state.request_reset(ResetKind::Reset) {
            state.reset_board();
        }
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

        // Toggle pencil (candidate-note) mode.
        b'm' | b'M' => {
            state.toggle_pencil_mode();
            true
        }

        // Digits 1-9: place a value, or toggle a pencil mark in pencil mode.
        b'1'..=b'9' => {
            if state.pencil_mode {
                state.toggle_note(byte - b'0');
            } else {
                state.set_digit(byte - b'0');
            }
            true
        }

        // Clear the cell's value, or its pencil marks in pencil mode.
        b'0' | 0x08 | 0x7F => {
            if state.pencil_mode {
                state.clear_cell_notes();
            } else {
                state.set_digit(0);
            }
            true
        } // 0, Backspace, Delete

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
        } // Up
        b'B' => {
            state.move_cursor(1, 0);
            true
        } // Down
        b'C' => {
            state.move_cursor(0, 1);
            true
        } // Right
        b'D' => {
            state.move_cursor(0, -1);
            true
        } // Left
        _ => false,
    }
}
