use crate::app::rooms::{backend::InputAction, chess::state::State};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    let seated = state.seat_index().is_some();

    match byte {
        0x1B | b'q' | b'Q' => InputAction::Leave,
        b' ' | b'\r' | b'\n' => {
            state.select_or_move();
            InputAction::Handled
        }
        b'n' | b'N' => {
            state.start_game();
            InputAction::Handled
        }
        b'l' | b'L' => {
            state.leave_seat();
            InputAction::Handled
        }
        b'r' | b'R' => {
            state.resign();
            InputAction::Handled
        }
        b's' | b'S' if !seated => {
            state.sit();
            InputAction::Handled
        }
        b'w' | b'W' => {
            state.move_cursor(0, 1);
            InputAction::Handled
        }
        b's' | b'S' => {
            state.move_cursor(0, -1);
            InputAction::Handled
        }
        b'a' | b'A' | b'h' | b'H' => {
            state.move_cursor(-1, 0);
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
    if state.seat_index().is_none() {
        return false;
    }
    match key {
        b'A' => state.move_cursor(0, 1),
        b'B' => state.move_cursor(0, -1),
        b'C' => state.move_cursor(1, 0),
        b'D' => state.move_cursor(-1, 0),
        _ => return false,
    }
    true
}
