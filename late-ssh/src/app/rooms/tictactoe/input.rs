use crate::app::rooms::{backend::InputAction, tictactoe::state::State};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    if (b'1'..=b'9').contains(&byte) {
        state.set_cursor((byte - b'1') as usize);
        if state.seat_index().is_some() {
            state.place_at_cursor();
        }
        return InputAction::Handled;
    }

    match byte {
        0x1B | b'q' | b'Q' => InputAction::Leave,
        b's' | b'S' => {
            state.sit();
            InputAction::Handled
        }
        b'l' | b'L' => {
            state.leave_seat();
            InputAction::Handled
        }
        b' ' | b'\r' | b'\n' => {
            if state.seat_index().is_some() {
                state.place_at_cursor();
            } else {
                state.sit();
            }
            InputAction::Handled
        }
        b'n' | b'N' => {
            state.reset();
            InputAction::Handled
        }
        b'h' | b'H' | b'a' | b'A' => {
            state.move_cursor(-1, 0);
            InputAction::Handled
        }
        b'w' | b'W' => {
            state.move_cursor(0, -1);
            InputAction::Handled
        }
        b'x' | b'X' => {
            state.move_cursor(0, 1);
            InputAction::Handled
        }
        b'd' | b'D' => {
            state.move_cursor(1, 0);
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => state.move_cursor(0, -1),
        b'B' => state.move_cursor(0, 1),
        b'C' => state.move_cursor(1, 0),
        b'D' => state.move_cursor(-1, 0),
        _ => return false,
    }
    true
}
