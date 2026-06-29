use super::state::{Face, State, ViewTurn};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'u' => {
            state.apply_relative_move(Face::Up, false);
            true
        }
        b'U' => {
            state.apply_relative_move(Face::Up, true);
            true
        }
        b'd' => {
            state.apply_relative_move(Face::Down, false);
            true
        }
        b'D' => {
            state.apply_relative_move(Face::Down, true);
            true
        }
        b'l' => {
            state.apply_relative_move(Face::Left, false);
            true
        }
        b'L' => {
            state.apply_relative_move(Face::Left, true);
            true
        }
        b'r' => {
            state.apply_relative_move(Face::Right, false);
            true
        }
        b'R' => {
            state.apply_relative_move(Face::Right, true);
            true
        }
        b'f' => {
            state.apply_relative_move(Face::Front, false);
            true
        }
        b'F' => {
            state.apply_relative_move(Face::Front, true);
            true
        }
        b'b' => {
            state.apply_relative_move(Face::Back, false);
            true
        }
        b'B' => {
            state.apply_relative_move(Face::Back, true);
            true
        }
        b's' | b'S' => {
            if state.request_reset() {
                state.reset();
            }
            true
        }
        b'0' => {
            if state.request_reset() {
                state.reset();
            }
            true
        }
        b'v' | b'V' => {
            state.turn_view(ViewTurn::Right);
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => {
            state.turn_view(ViewTurn::Up);
            true
        }
        b'B' => {
            state.turn_view(ViewTurn::Down);
            true
        }
        b'C' => {
            state.turn_view(ViewTurn::Right);
            true
        }
        b'D' => {
            state.turn_view(ViewTurn::Left);
            true
        }
        _ => false,
    }
}
