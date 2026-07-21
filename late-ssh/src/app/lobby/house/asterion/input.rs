use asterion_core::{Direction, GameCommand};

use crate::app::lobby::house::{asterion::state::State, types::InputAction};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    let direction = if let Some(direction) = movement_direction_for_key(byte) {
        direction
    } else {
        match byte {
            0x1B | b'q' | b'Q' => return InputAction::Leave,
            b',' => {
                state.send_command(GameCommand::TurnCounterClockwise);
                return InputAction::Handled;
            }
            b'.' => {
                state.send_command(GameCommand::TurnClockwise);
                return InputAction::Handled;
            }
            _ => return InputAction::Ignored,
        }
    };
    state.send_command(GameCommand::Move { direction });
    InputAction::Handled
}

fn movement_direction_for_key(byte: u8) -> Option<Direction> {
    match byte {
        b'w' | b'W' => Some(Direction::North),
        b's' | b'S' => Some(Direction::South),
        b'a' | b'A' | b'h' | b'H' => Some(Direction::West),
        b'd' | b'D' | b'l' | b'L' => Some(Direction::East),
        _ => None,
    }
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

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
