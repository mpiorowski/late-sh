use super::state::{PlayerInput, State, TrafficScreen};
use crate::app::arcade::traffic::input::*;
use crate::app::arcade::traffic::tracks::DEFAULT_TRACK;

#[test]
fn picker_enter_starts_a_track() {
    let mut s = State::new();
    handle_key(&mut s, b'\r');
    assert_eq!(s.screen, TrafficScreen::Racing);
}

#[test]
fn race_w_sets_accelerate() {
    let mut s = State::new();
    s.start_track(DEFAULT_TRACK);
    handle_key(&mut s, b'w');
    assert!(matches!(s.input, PlayerInput::Accelerate));
}
