//! Key handling for the World Cup screen.
//!
//! Only a tiny set of keys is screen-local: `Space` toggles between the
//! overview and the bracket, and `j`/`k` scroll the active view. Arrow keys
//! and the mouse wheel are adapted into these by the dispatcher in
//! `app/input.rs`. Everything else (Tab, the page-number keys, `?`, `q`, …)
//! is intentionally left unhandled so it falls through to global handling.

use super::state::State;

/// Handles one key byte. Returns `true` only when the key was consumed.
pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b' ' => {
            state.toggle_view();
            true
        }
        b'j' | b'J' => {
            state.scroll_down();
            true
        }
        b'k' | b'K' => {
            state.scroll_up();
            true
        }
        _ => false,
    }
}
