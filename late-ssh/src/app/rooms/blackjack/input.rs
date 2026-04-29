use super::state::{Phase, State};

pub enum InputAction {
    Ignored,
    Handled,
    Leave,
}

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    if !state.is_seated() {
        return match byte {
            b's' | b'S' | b'\r' | b'\n' => {
                state.sit();
                InputAction::Handled
            }
            0x1B => InputAction::Leave,
            _ => InputAction::Ignored,
        };
    }

    if matches!(byte, b'l' | b'L') {
        state.leave_seat();
        return InputAction::Handled;
    }

    match state.snapshot.phase {
        Phase::Betting => match byte {
            b'[' | b'a' | b'A' => {
                state.move_chip_selection(-1);
                InputAction::Handled
            }
            b']' | b'd' | b'D' => {
                state.move_chip_selection(1);
                InputAction::Handled
            }
            b' ' => {
                state.throw_selected_chip();
                InputAction::Handled
            }
            0x08 | 0x7F => {
                state.pull_last_chip();
                InputAction::Handled
            }
            0x17 | b'c' | b'C' => {
                state.clear_stake();
                InputAction::Handled
            }
            b'\r' | b'\n' => {
                state.submit_stake();
                InputAction::Handled
            }
            0x1B => InputAction::Leave,
            _ => InputAction::Ignored,
        },
        Phase::BetPending => InputAction::Ignored,
        Phase::PlayerTurn => match byte {
            b'h' | b'H' | b' ' => {
                state.hit();
                InputAction::Handled
            }
            b's' | b'S' => {
                state.stand();
                InputAction::Handled
            }
            0x1B => {
                state.stand();
                InputAction::Leave
            }
            _ => InputAction::Ignored,
        },
        Phase::DealerTurn => InputAction::Ignored,
        Phase::Settling => match byte {
            0x1B => InputAction::Leave,
            b'n' | b'N' | b'\r' | b'\n' | b' ' => {
                state.next_hand();
                InputAction::Handled
            }
            _ => InputAction::Ignored,
        },
    }
}
