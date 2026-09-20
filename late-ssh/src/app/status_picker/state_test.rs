use super::*;

use chrono::Utc;

#[test]
fn opens_on_the_current_status_with_no_timer_armed() {
    let mut picker = StatusPickerState::default();

    picker.open(None);
    assert!(picker.is_open());
    assert_eq!(picker.selected_status(), Status::Focus);

    // Reopening while a status runs starts where the user left off, so
    // changing your mind is not a walk back down the list.
    picker.open(Some(SessionStatus {
        status: Status::Gaming,
        ends_at: Some(Utc::now()),
    }));
    assert_eq!(picker.selected_status(), Status::Gaming);

    // The armed duration always resets to the safe one: a status that clears
    // itself on the next message can never be set by accident.
    assert_eq!(picker.selected_minutes(), None);
}

#[test]
fn selection_and_duration_both_wrap() {
    let mut picker = StatusPickerState::default();
    picker.open(None);

    picker.move_selection(-1);
    assert_eq!(
        picker.selected_status(),
        Status::Away,
        "up from the first row lands on the last"
    );
    picker.move_selection(1);
    assert_eq!(picker.selected_status(), Status::Focus);

    picker.cycle_duration(-1);
    assert_eq!(picker.selected_minutes(), Some(90));
    picker.cycle_duration(1);
    assert_eq!(picker.selected_minutes(), None);
    picker.cycle_duration(2);
    assert_eq!(picker.selected_minutes(), Some(25));
}

/// The number keys are the regular's path, so an out-of-range digit has to be
/// inert rather than land on the nearest row.
#[test]
fn number_keys_select_only_real_rows() {
    let mut picker = StatusPickerState::default();
    picker.open(None);

    assert!(picker.select_row(8));
    assert_eq!(picker.selected_status(), Status::Away);

    assert!(!picker.select_row(9));
    assert_eq!(
        picker.selected_status(),
        Status::Away,
        "an out-of-range digit leaves the highlight alone"
    );
    assert!(!picker.select_row(0));
    assert_eq!(picker.selected_status(), Status::Away);
}
