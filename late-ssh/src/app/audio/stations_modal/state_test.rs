use super::*;

fn key(key: &str) -> RadioStation {
    RadioStation::from_key(key).unwrap()
}

#[test]
fn opening_lands_the_cursor_on_the_current_station() {
    let mut state = StationsModalState::default();
    state.open(key("classical"));
    assert!(state.is_open());
    assert_eq!(state.selected_station(), Some(key("classical")));
    assert_eq!(state.stations().len(), RadioStation::enabled().count());
}

#[test]
fn the_cursor_wraps_at_both_ends() {
    let mut state = StationsModalState::default();
    state.open(key("chillsynth"));
    state.move_selection(-1);
    assert_eq!(
        state.selected_station(),
        RadioStation::enabled().last(),
        "up from the top wraps to the bottom"
    );
    state.move_selection(1);
    assert_eq!(
        state.selected_station(),
        Some(key("chillsynth")),
        "down from the bottom wraps to the top"
    );
}

#[test]
fn closing_resets_everything() {
    let mut state = StationsModalState::default();
    state.open(key("rektify"));
    state.close();
    assert!(!state.is_open());
    assert_eq!(state.selected(), 0);
}
