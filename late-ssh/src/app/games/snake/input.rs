

use super::state::{State};


fn handle_key(&mut state: State, byte: u8) -> bool {
    state.input_queue.insert(0, byte);
    state.handle_key();
    true
}
