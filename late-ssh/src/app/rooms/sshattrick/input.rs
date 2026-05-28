use sshattrick_core::GameCommand;

use crate::app::rooms::{backend::InputAction, sshattrick::state::State};

use super::svc::Phase;

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    match byte {
        0x1B | b'q' | b'Q' => InputAction::Leave,
        b'w' | b'W' => game_command(state, GameCommand::Up),
        b's' | b'S' => game_command(state, GameCommand::Down),
        b'a' | b'A' => game_command(state, GameCommand::Left),
        b'd' | b'D' => game_command(state, GameCommand::Right),
        b' ' => handle_space(state),
        b'n' | b'N' => handle_reset(state),
        _ => InputAction::Ignored,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let command = match key {
        b'A' => GameCommand::Up,
        b'B' => GameCommand::Down,
        b'C' => GameCommand::Right,
        b'D' => GameCommand::Left,
        _ => return false,
    };
    state.send_command(command);
    true
}

fn game_command(state: &mut State, command: GameCommand) -> InputAction {
    if state.private().seated_as.is_none() {
        return InputAction::Ignored;
    }
    state.send_command(command);
    InputAction::Handled
}

fn handle_space(state: &mut State) -> InputAction {
    if state.private().seated_as.is_none() {
        state.sit();
        return InputAction::Handled;
    }
    if state.public().phase == Phase::Ending {
        state.reset();
        return InputAction::Handled;
    }
    state.send_command(GameCommand::Shoot);
    InputAction::Handled
}

fn handle_reset(state: &mut State) -> InputAction {
    if state.public().phase == Phase::Ending {
        state.reset();
        InputAction::Handled
    } else {
        InputAction::Ignored
    }
}
