use late_core::models::character_sheet::SHEET_NAME_MAX_CHARS;
use crate::app::chat::state::SheetOpenRequest;
use uuid::Uuid;
use crate::app::sheet_modal::state::*;

fn request(editable: bool) -> SheetOpenRequest {
    SheetOpenRequest {
        room_id: Uuid::from_u128(7),
        target_username: "frodo".to_string(),
        name: "Frodo".to_string(),
        body: "Ring bearer".to_string(),
        editable,
    }
}

#[test]
fn open_populates_fields() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    assert_eq!(state.target_username(), "frodo");
    assert_eq!(state.name_text(), "Frodo");
    assert_eq!(state.body_text(), "Ring bearer");
    assert!(state.editable());
    assert!(!state.editing());
    assert_eq!(state.take_pending_save(), None);
}

#[test]
fn read_only_sheet_blocks_editing() {
    let mut state = SheetModalState::new();
    state.open(request(false));
    state.start_edit();
    assert!(!state.editing());
}

#[test]
fn name_submit_commits_and_queues_save() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    state.start_edit();
    state.name_input_mut().insert_str(" Baggins");
    state.submit_edit();
    assert!(!state.editing());
    assert_eq!(state.name_text(), "Frodo Baggins");
    let save = state.take_pending_save().expect("queued save");
    assert_eq!(save.room_id, Uuid::from_u128(7));
    assert_eq!(save.name, "Frodo Baggins");
    assert_eq!(save.body, "Ring bearer");
}

#[test]
fn name_cancel_reverts_to_committed_value() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    state.start_edit();
    state.name_input_mut().insert_str(" the Brave");
    state.cancel_edit();
    assert_eq!(state.name_text(), "Frodo");
    assert_eq!(state.take_pending_save(), None);
}

#[test]
fn body_submit_queues_save_with_trimmed_body() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    state.set_focus(SheetField::Body);
    state.start_edit();
    state.body_input_mut().insert_str(" of the Shire\n\n");
    state.submit_edit();
    let save = state.take_pending_save().expect("queued save");
    assert_eq!(save.body, "Ring bearer of the Shire");
}

#[test]
fn submitted_name_is_clamped_to_max_chars() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    state.start_edit();
    let long: String = "x".repeat(SHEET_NAME_MAX_CHARS * 2);
    state.name_input_mut().insert_str(&long);
    state.submit_edit();
    assert!(state.name_text().chars().count() <= SHEET_NAME_MAX_CHARS);
}

#[test]
fn toggle_focus_switches_fields() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    assert_eq!(state.focus(), SheetField::Name);
    state.toggle_focus();
    assert_eq!(state.focus(), SheetField::Body);
    state.toggle_focus();
    assert_eq!(state.focus(), SheetField::Name);
}

#[test]
fn reopen_resets_state_and_drops_stale_pending_save() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    state.start_edit();
    state.name_input_mut().insert_str("!");
    state.submit_edit();
    assert!(state.take_pending_save().is_some());

    state.start_edit();
    state.name_input_mut().insert_str("?");
    state.submit_edit();
    // Re-open before the pump consumed the queued save: it must be dropped.
    let mut second = request(false);
    second.target_username = "sam".to_string();
    second.name = "Sam".to_string();
    second.body = "Gardener".to_string();
    state.open(second);
    assert_eq!(state.take_pending_save(), None);
    assert_eq!(state.target_username(), "sam");
    assert_eq!(state.name_text(), "Sam");
    assert_eq!(state.body_text(), "Gardener");
    assert!(!state.editable());
    assert!(!state.editing());
}

#[test]
fn close_keeps_queued_save_for_the_tick_pump() {
    let mut state = SheetModalState::new();
    state.open(request(true));
    state.start_edit();
    state.name_input_mut().insert_str("!");
    state.submit_edit();
    state.close();
    // The user's last edit must still reach the pump after close.
    assert!(state.take_pending_save().is_some());
}
