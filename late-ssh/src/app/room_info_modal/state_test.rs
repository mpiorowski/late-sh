use super::state::{Field, Mode, RoomInfoModalState};

#[test]
fn open_create_starts_empty_and_owned_by_you() {
    let mut s = RoomInfoModalState::default();
    assert!(!s.is_open());
    s.open_create("book-club".to_string());
    assert!(s.is_open());
    assert_eq!(s.focus(), Field::Topic);
    assert_eq!(s.room_label(), "#book-club");
    assert_eq!(s.owner_label(), "you");
    assert!(matches!(s.mode(), Some(Mode::Create { .. })));
    let (topic, rules) = s.values();
    assert!(topic.is_empty());
    assert!(rules.is_empty());
}

#[test]
fn open_edit_prefills_both_fields_and_names_the_owner() {
    let mut s = RoomInfoModalState::default();
    s.open_edit(
        uuid::Uuid::nil(),
        "#book-club".to_string(),
        "gandalf".to_string(),
        Some("We read things"),
        Some("Be kind"),
    );
    assert_eq!(s.owner_label(), "gandalf");
    let (topic, rules) = s.values();
    assert_eq!(topic, "We read things");
    assert_eq!(rules, "Be kind");
}

#[test]
fn focus_toggles_between_the_two_fields() {
    let mut s = RoomInfoModalState::default();
    s.open_create("r".to_string());
    assert_eq!(s.focus(), Field::Topic);
    s.toggle_focus();
    assert_eq!(s.focus(), Field::Rules);
    s.toggle_focus();
    assert_eq!(s.focus(), Field::Topic);
}

#[test]
fn close_clears_everything() {
    let mut s = RoomInfoModalState::default();
    s.open_edit(
        uuid::Uuid::nil(),
        "#r".to_string(),
        "you".to_string(),
        Some("topic"),
        None,
    );
    s.close();
    assert!(!s.is_open());
    assert!(s.mode().is_none());
    assert!(s.owner_label().is_empty());
    let (topic, _) = s.values();
    assert!(topic.is_empty());
}

#[test]
fn whitespace_only_input_reads_as_unset() {
    let mut s = RoomInfoModalState::default();
    s.open_edit(
        uuid::Uuid::nil(),
        "#r".to_string(),
        "you".to_string(),
        Some("   "),
        Some("\t"),
    );
    let (topic, rules) = s.values();
    assert!(topic.is_empty(), "blank topic trims to unset");
    assert!(rules.is_empty(), "blank rules trim to unset");
}
