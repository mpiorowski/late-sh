use asterion_core::{Direction, GameCommand};

use crate::app::rooms::{asterion::state::State, backend::InputAction};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    let direction = match byte {
        0x1B | b'q' | b'Q' => return InputAction::Leave,
        b'w' | b'W' => Direction::North,
        b's' | b'S' => Direction::South,
        b'a' | b'A' | b'h' | b'H' => Direction::West,
        b'd' | b'D' | b'l' | b'L' => Direction::East,
        b',' => {
            state.send_command(GameCommand::TurnCounterClockwise);
            return InputAction::Handled;
        }
        b'.' => {
            state.send_command(GameCommand::TurnClockwise);
            return InputAction::Handled;
        }
        b'o' | b'O' => {
            state.send_command(GameCommand::CycleUiOptions);
            return InputAction::Handled;
        }
        _ => return InputAction::Ignored,
    };
    state.send_command(GameCommand::Move { direction });
    InputAction::Handled
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let direction = match key {
        b'A' => Direction::North,
        b'B' => Direction::South,
        b'C' => Direction::East,
        b'D' => Direction::West,
        _ => return false,
    };
    state.send_command(GameCommand::Move { direction });
    true
}
