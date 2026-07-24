use crate::ircd::proj::*;
use late_core::models::chat_room::ChatRoom;

fn room(kind: &str, visibility: &str, slug: Option<&str>) -> ChatRoom {
    ChatRoom {
        id: uuid::Uuid::new_v4(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        kind: kind.to_string(),
        visibility: visibility.to_string(),
        auto_join: false,
        permanent: false,
        slug: slug.map(str::to_string),
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    }
}

#[test]
fn channel_names_follow_room_kinds() {
    assert_eq!(
        channel_name(&room("lounge", "public", Some("lounge"))).as_deref(),
        Some("#lounge")
    );
    assert_eq!(
        channel_name(&room("topic", "private", Some("sekrit"))).as_deref(),
        Some("#sekrit")
    );
    assert_eq!(channel_name(&room("game", "public", Some("poker-1"))), None);
    assert_eq!(channel_name(&room("dm", "dm", None)), None);
    assert_eq!(channel_name(&room("lounge", "public", None)), None);
}

#[test]
fn split_body_respects_newlines_and_utf8_boundaries() {
    assert_eq!(split_body("a\nb", 400), vec!["a", "b"]);
    assert_eq!(split_body("", 400), Vec::<String>::new());
    // multi-byte chars must not be split mid-codepoint
    let body = "é".repeat(300); // 600 bytes
    let lines = split_body(&body, 400);
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l.len() <= 400));
    assert_eq!(lines.join(""), body);
}

#[test]
fn ctcp_action_round_trip() {
    assert_eq!(parse_ctcp_action("\u{1}ACTION waves\u{1}"), Some("waves"));
    assert_eq!(parse_ctcp_action("hello"), None);
    assert_eq!(action_to_body("waves"), "\u{1}ACTION waves\u{1}");
}

#[test]
fn channel_lookup_helpers() {
    assert_eq!(slug_for_channel("#lounge"), Some("lounge"));
    assert_eq!(slug_for_channel("lounge"), None);
    assert_eq!(slug_for_channel("#"), None);
    assert_eq!(normalize_channel("#LOUNGE"), "#lounge");
}

#[test]
fn nick_projection_substitutes_dots_reversibly() {
    assert_eq!(nick_for_username("alice.smith"), "alice^smith");
    assert_eq!(nick_for_username(".alice."), "^alice^");
    assert_eq!(username_for_nick("alice^smith"), "alice.smith");
    assert_eq!(username_for_nick("^alice^"), ".alice.");
}

#[test]
fn leading_mentions_are_projected_to_irc_nicks() {
    let usernames = ["alice.smith", "Bob.Dot"];
    let lookup = |candidate: &str| {
        usernames
            .iter()
            .find(|username| username.eq_ignore_ascii_case(candidate))
            .map(|username| (*username).to_string())
    };

    assert_eq!(
        rewrite_leading_mention_for_irc("@alice.smith hello", lookup),
        "@alice^smith hello"
    );
    assert_eq!(
        rewrite_leading_mention_for_irc("  Bob.Dot: hello", lookup),
        "  Bob^Dot: hello"
    );
    assert_eq!(
        rewrite_leading_mention_for_irc("alice.smith, hello", lookup),
        "alice^smith, hello"
    );
}

#[test]
fn leading_mentions_are_projected_to_late_usernames() {
    let usernames = ["alice.smith", "Bob.Dot"];
    let lookup = |candidate: &str| {
        usernames
            .iter()
            .find(|username| nick_for_username(username).eq_ignore_ascii_case(candidate))
            .map(|username| (*username).to_string())
    };

    assert_eq!(
        rewrite_leading_mention_for_late("@alice^smith hello", lookup),
        "@alice.smith hello"
    );
    assert_eq!(
        rewrite_leading_mention_for_late("  Bob^Dot: hello", lookup),
        "  Bob.Dot: hello"
    );
    assert_eq!(
        rewrite_leading_mention_for_late("alice^smith, hello", lookup),
        "alice.smith, hello"
    );
}

#[test]
fn leading_mention_projection_ignores_non_matches() {
    let lookup = |candidate: &str| (candidate == "alice.smith").then(|| candidate.to_string());

    assert_eq!(
        rewrite_leading_mention_for_irc("hello @alice.smith", lookup),
        "hello @alice.smith"
    );
    assert_eq!(
        rewrite_leading_mention_for_irc("@alice.smithsonian hello", lookup),
        "@alice.smithsonian hello"
    );
    assert_eq!(
        rewrite_leading_mention_for_irc("@missing.user hello", lookup),
        "@missing.user hello"
    );
}
