use super::state::{CubeMove, Face, State, ViewTurn};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'u' => {
            state.apply_move(CubeMove {
                face: Face::Up,
                inverse: false,
            });
            true
        }
        b'U' => {
            state.apply_move(CubeMove {
                face: Face::Up,
                inverse: true,
            });
            true
        }
        b'd' => {
            state.apply_move(CubeMove {
                face: Face::Down,
                inverse: false,
            });
            true
        }
        b'D' => {
            state.apply_move(CubeMove {
                face: Face::Down,
                inverse: true,
            });
            true
        }
        b'l' => {
            state.apply_move(CubeMove {
                face: Face::Left,
                inverse: false,
            });
            true
        }
        b'L' => {
            state.apply_move(CubeMove {
                face: Face::Left,
                inverse: true,
            });
            true
        }
        b'r' => {
            state.apply_move(CubeMove {
                face: Face::Right,
                inverse: false,
            });
            true
        }
        b'R' => {
            state.apply_move(CubeMove {
                face: Face::Right,
                inverse: true,
            });
            true
        }
        b'f' => {
            state.apply_move(CubeMove {
                face: Face::Front,
                inverse: false,
            });
            true
        }
        b'F' => {
            state.apply_move(CubeMove {
                face: Face::Front,
                inverse: true,
            });
            true
        }
        b'b' => {
            state.apply_move(CubeMove {
                face: Face::Back,
                inverse: false,
            });
            true
        }
        b'B' => {
            state.apply_move(CubeMove {
                face: Face::Back,
                inverse: true,
            });
            true
        }
        b's' | b'S' => {
            state.reset();
            true
        }
        b'0' => {
            state.reset();
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
