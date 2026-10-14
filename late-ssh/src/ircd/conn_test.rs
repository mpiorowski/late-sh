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
        "message-tags server-time echo-message away-notify"
    ));

    assert!(caps.message_tags);
    assert!(caps.server_time);
    assert!(caps.echo_message);
    assert!(caps.away_notify);
    assert_eq!(
        caps.as_list(),
        "message-tags server-time echo-message away-notify"
    );
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

#[test]
fn history_capabilities_can_be_negotiated_independently_and_disabled() {
    let mut caps = IrcCapabilities::default();
    assert!(apply_cap_request(&mut caps, "batch server-time"));
    assert!(caps.automatic_history());
    assert!(!caps.message_tags);
    assert!(apply_cap_request(&mut caps, "draft/chathistory"));
    assert!(!caps.automatic_history());
    assert!(caps.as_list().contains("batch draft/chathistory"));
    assert!(!apply_cap_request(&mut caps, "-batch unknown-history-cap"));
    assert!(caps.batch);
    assert!(apply_cap_request(&mut caps, "-batch -server-time"));
    assert!(caps.chathistory);
    assert!(!caps.batch);
    assert!(!caps.server_time);
    assert!(!caps.automatic_history());
    assert!(apply_cap_request(
        &mut caps,
        "-draft/chathistory batch server-time"
    ));
    assert!(caps.automatic_history());
}

#[test]
fn automatic_history_suppresses_only_queued_creations_and_preserves_edits() {
    let replayed = uuid::Uuid::new_v4();
    let new = uuid::Uuid::new_v4();
    let mut channel = super::JoinedChannel {
        name: "#room".into(),
        slug: "room".into(),
        is_lounge: false,
        replayed: [replayed].into_iter().collect(),
    };
    assert!(!channel.suppress_replayed_create(new, false));
    assert!(!channel.suppress_replayed_create(replayed, true));
    assert!(channel.suppress_replayed_create(replayed, false));
    assert!(!channel.suppress_replayed_create(replayed, true));
    assert!(channel.replayed.is_empty());
}
