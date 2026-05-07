use crate::app::rooms::{backend::InputAction, poker::state::State};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    if !state.is_seated() {
        return match byte {
            b's' | b'S' | b'\r' | b'\n' => {
                state.sit();
                InputAction::Handled
            }
            0x1B | b'q' | b'Q' => InputAction::Leave,
            _ => InputAction::Ignored,
        };
    }

    match byte {
        0x1B | b'q' | b'Q' => InputAction::Leave,
        b'l' | b'L' => {
            state.leave_seat();
            InputAction::Handled
        }
        b'n' | b'N' => {
            state.start_hand();
            InputAction::Handled
        }
        b'f' | b'F' => {
            state.fold();
            InputAction::Handled
        }
        b'c' | b'C' | b' ' | b'\r' | b'\n' => {
            state.check();
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}
