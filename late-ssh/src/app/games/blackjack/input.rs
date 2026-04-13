use super::state::{Phase, State};

pub enum InputAction {
    Ignored,
    Handled,
    Leave,
}

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    match state.snapshot.phase {
        Phase::Betting => match byte {
            b'0'..=b'9' => {
                state.append_bet_digit(byte as char);
                InputAction::Handled
            }
            0x08 | 0x7F => {
                state.pop_bet_digit();
                InputAction::Handled
            }
            b'\r' | b'\n' => {
                state.submit_bet_from_buffer();
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
            _ => {
                state.next_hand();
                InputAction::Handled
            }
        },
    }
}
