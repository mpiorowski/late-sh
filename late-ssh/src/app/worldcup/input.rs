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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::worldcup::state::View;

    #[test]
    fn space_toggles_view() {
        let mut s = State::default();
        assert!(handle_key(&mut s, b' '));
        assert_eq!(s.view, View::Bracket);
    }

    #[test]
    fn j_and_k_scroll_active_view() {
        let mut s = State::default();
        assert!(handle_key(&mut s, b'j'));
        assert_eq!(s.overview_scroll, 1);
        assert!(handle_key(&mut s, b'k'));
        assert_eq!(s.overview_scroll, 0);
    }

    #[test]
    fn other_keys_fall_through() {
        let mut s = State::default();
        for b in [b'7', b'q', b'\t', b'?', b'x'] {
            assert!(!handle_key(&mut s, b));
        }
        assert_eq!(s.view, View::Overview);
    }
}
