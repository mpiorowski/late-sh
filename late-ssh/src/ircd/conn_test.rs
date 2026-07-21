use super::{IrcCapabilities, apply_cap_request, nick_from_ban_mask};

#[test]
fn ban_mask_accepts_nick_identity_shape() {
    assert_eq!(nick_from_ban_mask("alice!*@*"), Some("alice"));
    assert_eq!(nick_from_ban_mask("Alice_123!*@*"), Some("Alice_123"));
}

#[test]
fn ban_mask_rejects_wildcards_hosts_and_plain_nicks() {
    assert_eq!(nick_from_ban_mask("*!*@*"), None);
    assert_eq!(nick_from_ban_mask("alice!*@example.com"), None);
    assert_eq!(nick_from_ban_mask("alice@host!*@*"), None);
    assert_eq!(nick_from_ban_mask("alice"), None);
}

#[test]
fn cap_request_enables_and_lists_supported_caps() {
    let mut caps = IrcCapabilities::default();

    assert!(apply_cap_request(
        &mut caps,
        "message-tags server-time echo-message"
    ));

    assert!(caps.message_tags);
    assert!(caps.server_time);
    assert!(caps.echo_message);
    assert_eq!(caps.as_list(), "message-tags server-time echo-message");
}

#[test]
fn cap_request_naks_unknown_without_changing_enabled_caps() {
    let mut caps = IrcCapabilities::default();
    assert!(apply_cap_request(&mut caps, "message-tags"));

    assert!(!apply_cap_request(&mut caps, "server-time chathistory"));

    assert!(caps.message_tags);
    assert!(!caps.server_time);
    assert!(!caps.echo_message);
    assert_eq!(caps.as_list(), "message-tags");
}

#[test]
fn cap_request_can_disable_supported_caps() {
    let mut caps = IrcCapabilities::default();
    assert!(apply_cap_request(&mut caps, "message-tags server-time"));

    assert!(apply_cap_request(&mut caps, "-server-time"));

    assert!(caps.message_tags);
    assert!(!caps.server_time);
    assert_eq!(caps.as_list(), "message-tags");
}
