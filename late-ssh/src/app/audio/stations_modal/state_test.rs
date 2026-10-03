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
fn the_cursor_wraps_and_the_filter_narrows_by_label_provider_and_tag() {
    let mut state = StationsModalState::default();
    state.open(key("chillsynth"));
    state.move_selection(-1);
    assert_eq!(
        state.selected_station(),
        RadioStation::enabled().last(),
        "up from the top wraps to the bottom"
    );

    state.start_filter();
    for ch in "late".chars() {
        state.push_filter(ch);
    }
    assert!(state.filter_active());
    state.apply_filter();
    assert!(!state.filter_active());
    let labels: Vec<&str> = state.stations().iter().map(|s| s.label()).collect();
    assert_eq!(labels, vec!["classical", "lofi"], "provider label matches");
    assert!(
        state.selected() < 2,
        "the cursor was clamped into the shorter list"
    );

    state.clear_filter();
    for ch in "synthwave".chars() {
        state.push_filter(ch);
    }
    // chillsynth, nightride, datawave, spacesynth, darksynth, horrorsynth.
    assert_eq!(state.stations().len(), 6, "tag matches");

    state.cancel_filter();
    assert_eq!(state.filter_query(), "");
    assert_eq!(state.stations().len(), RadioStation::enabled().count());
}

#[test]
fn closing_resets_everything() {
    let mut state = StationsModalState::default();
    state.open(key("rektify"));
    state.start_filter();
    state.push_filter('x');
    state.close();
    assert!(!state.is_open());
    assert!(!state.filter_active());
    assert_eq!(state.filter_query(), "");
    assert_eq!(state.selected(), 0);
}
