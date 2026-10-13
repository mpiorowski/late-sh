use super::*;

#[test]
fn only_fixed_launches_are_accepted() {
    assert_eq!(
        parse_request(b"play zork2 manual"),
        Some(Request::Play(Edition::Zork2, Action::Manual))
    );
    for value in [
        "play ../../etc/passwd new",
        "play zork1 auto; id",
        "play zork1 filename",
        "list extra",
        "play zork4 new",
    ] {
        assert!(parse_request(value.as_bytes()).is_none());
    }
}
