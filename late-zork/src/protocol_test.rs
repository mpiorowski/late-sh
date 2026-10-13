use super::*;

#[test]
fn only_fixed_launches_are_accepted() {
    assert_eq!(
        parse_request(b"play zork2 manual alice_99"),
        Some(Request::Play(
            Edition::Zork2,
            Action::Manual,
            "alice_99".into()
        ))
    );
    for value in [
        "play ../../etc/passwd new alice",
        "play zork1 auto; id",
        "play zork1 filename alice",
        "list extra",
        "play zork4 new alice",
        "play zork1 new",
        "play zork1 new ../alice",
        "play zork1 new 9lives",
        "play zork1 new al",
        "play zork1 new alice extra",
    ] {
        assert!(parse_request(value.as_bytes()).is_none(), "{value}");
    }
}

#[test]
fn playnames_are_arcade_handles() {
    assert_eq!(parse_playname("Mat_42"), Some("Mat_42".into()));
    assert_eq!(parse_playname(&"a".repeat(20)), Some("a".repeat(20)));
    for value in ["", "ab", "_mat", "mat-1", "mat 1", &"a".repeat(21)] {
        assert_eq!(parse_playname(value), None, "{value}");
    }
}
