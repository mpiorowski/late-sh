use super::state::{Field, Mode, RoomInfoModalState, TITLE_MAX};

#[test]
fn open_create_seeds_the_name_and_focuses_it() {
    let mut s = RoomInfoModalState::default();
    assert!(!s.is_open());
    s.open_create(false, "book-club".to_string(), "book club");
    assert!(s.is_open());
    assert_eq!(s.focus(), Field::Title);
    assert!(matches!(
        s.mode(),
        Some(Mode::Create {
            is_private: false,
            ..
        })
    ));
    let (title, about, rules) = s.values();
    assert_eq!(title, "book club");
    assert!(about.is_empty());
    assert!(rules.is_empty());
}

#[test]
fn open_edit_prefills_all_fields() {
    let mut s = RoomInfoModalState::default();
    s.open_edit(
        uuid::Uuid::nil(),
        Some("Book Club"),
        Some("We read things"),
        Some("Be kind"),
    );
    let (title, about, rules) = s.values();
    assert_eq!(title, "Book Club");
    assert_eq!(about, "We read things");
    assert_eq!(rules, "Be kind");
}

#[test]
fn typing_lands_in_the_focused_field_and_respects_the_cap() {
    let mut s = RoomInfoModalState::default();
    s.open_create(true, "x".to_string(), "");
    for _ in 0..(TITLE_MAX + 20) {
        s.push('a');
    }
    let (title, _, _) = s.values();
    assert_eq!(title.chars().count(), TITLE_MAX, "title should cap out");

    s.focus_next(); // -> About
    assert_eq!(s.focus(), Field::About);
    s.push('h');
    s.push('i');
    let (_, about, _) = s.values();
    assert_eq!(about, "hi");
}

#[test]
fn focus_cycles_forward_and_back() {
    let mut s = RoomInfoModalState::default();
    s.open_create(false, "r".to_string(), "");
    assert_eq!(s.focus(), Field::Title);
    s.focus_next();
    assert_eq!(s.focus(), Field::About);
    s.focus_next();
    assert_eq!(s.focus(), Field::Rules);
    s.focus_next();
    assert_eq!(s.focus(), Field::Title);
    s.focus_prev();
    assert_eq!(s.focus(), Field::Rules);
}

#[test]
fn close_clears_everything() {
    let mut s = RoomInfoModalState::default();
    s.open_create(false, "r".to_string(), "name");
    s.close();
    assert!(!s.is_open());
    assert!(s.mode().is_none());
    let (title, _, _) = s.values();
    assert!(title.is_empty());
}

#[test]
fn empty_name_is_reported_by_values_for_the_submit_guard() {
    let mut s = RoomInfoModalState::default();
    s.open_create(false, "r".to_string(), "   ");
    let (title, _, _) = s.values();
    assert!(title.is_empty(), "whitespace-only name trims to empty");
}
