use crate::app::chat::action::*;

#[test]
fn encodes_and_parses_action_body() {
    let body = encode_action_body("waves").expect("action");
    assert_eq!(parse_action_body(&body), Some("waves"));
}

#[test]
fn rejects_empty_action_body() {
    assert_eq!(encode_action_body("   "), None);
    assert_eq!(parse_action_body("\x01ACTION \x01"), None);
}
