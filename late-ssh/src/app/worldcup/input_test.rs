use super::state::State;
use crate::app::worldcup::input::*;
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
    for b in *b"7q\t?x" {
        assert!(!handle_key(&mut s, b));
    }
    assert_eq!(s.view, View::Overview);
}
