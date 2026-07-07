use crate::app::rooms::{
    backend::InputAction,
    tron::state::{Direction, State},
};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    let seated = state.seat_index().is_some();
    match byte {
        0x1B | b'q' | b'Q' => InputAction::Leave,
        b'l' | b'L' => {
            state.leave_seat();
            InputAction::Handled
        }
        b'n' | b'N' => {
            state.start_round();
            InputAction::Handled
        }
        b' ' | b'\r' | b'\n' if !seated => {
            state.sit();
            InputAction::Handled
        }
        b's' | b'S' if !seated => {
            state.sit();
            InputAction::Handled
        }
        b'w' | b'W' => {
            state.steer(Direction::Up);
            InputAction::Handled
        }
        b's' | b'S' => {
            state.steer(Direction::Down);
            InputAction::Handled
        }
        b'a' | b'A' | b'h' | b'H' => {
            state.steer(Direction::Left);
            InputAction::Handled
        }
        b'd' | b'D' => {
            state.steer(Direction::Right);
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
        b'A' => state.steer(Direction::Up),
        b'B' => state.steer(Direction::Down),
        b'C' => state.steer(Direction::Right),
        b'D' => state.steer(Direction::Left),
        _ => return false,
    }
    true
}
