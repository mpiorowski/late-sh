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
        b'[' | b'-' => {
            state.decrease_raise();
            InputAction::Handled
        }
        b']' | b'+' | b'=' => {
            state.increase_raise();
            InputAction::Handled
        }
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
        b'b' | b'B' | b'r' | b'R' => {
            state.bet_or_raise();
            InputAction::Handled
        }
        b'a' | b'A' => {
            state.all_in();
            InputAction::Handled
        }
        b'x' | b'X' => {
            state.toggle_auto_check_fold();
            InputAction::Handled
        }
        b'c' | b'C' | b' ' | b'\r' | b'\n' => {
            state.call_or_check();
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}
