use super::*;
use chrono::Duration as ChronoDuration;
use late_core::models::chat_poll::{ChatPoll, ChatPollOptionSummary};

/// A slur can lengthen the text (a thickened `s`, a `*hic*`). Text that
/// fit the column as typed and would not once slurred goes out as typed,
/// so a wasted patron's long message is never lost to the insert's CHECK.
#[test]
fn drunk_text_that_outgrows_the_cap_goes_out_as_typed() {
    use late_core::models::chat_message::BODY_MAX_CHARS;
    let typed = format!("{}ab", "lasso ".repeat(333));
    assert_eq!(typed.chars().count(), BODY_MAX_CHARS);
    assert!(
        slur::slur(&typed, 4, 7).chars().count() > BODY_MAX_CHARS,
        "the slur alone outgrows the cap"
    );
    assert_eq!(drunk_body(&typed, 4, 7), typed);
}

/// The moon and the star stand apart when a name carries both, and each
/// stands alone otherwise.
#[test]
fn ladder_marks_put_a_space_between_the_moon_and_the_star() {
    let streak = |consecutive_days| DailyStreakSnapshot {
        consecutive_days,
        last_completed_date: None,
    };
    let marks = |days, starred| ladder_marks(Some(&streak(days)), starred);
    assert_eq!(marks(30, true).as_deref(), Some("● ✦"));
    assert_eq!(marks(30, false).as_deref(), Some("●"));
    assert_eq!(marks(1, true).as_deref(), Some("✦"));
    assert_eq!(marks(1, false), None);
    assert_eq!(ladder_marks(None, true).as_deref(), Some("✦"));
}

#[test]
fn contains_link_catches_schemes_www_and_bare_domains() {
    for spam in [
        "click https://evil.example/win",
        "HTTP://EVIL.io free chips",
        "go to www.evil.io now",
        "buy at evil.io/now",
        "join evil.gg or evil.xyz",
        "dm me on telegram t.me/scammer",
    ] {
        assert!(contains_link(spam), "should flag: {spam}");
    }
    for clean in [
        "hello there, how are you?",
        "i finished 2048 and got a high score",
        "see you at 3pm. thanks!",
        "node.js is fine to mention",
        "e.g. that idea is good",
    ] {
        assert!(!contains_link(clean), "should not flag: {clean}");
    }
}

#[test]
fn link_cooldown_tiers_by_account_age() {
    let hour = 3_600;
    let day = 24 * hour;
    // Fresh (< 1 day): 30 minutes.
    assert_eq!(link_cooldown_for_age(0), Some(LINK_COOLDOWN_FRESH));
    assert_eq!(link_cooldown_for_age(23 * hour), Some(LINK_COOLDOWN_FRESH));
    // Young (1–7 days): 5 minutes.
    assert_eq!(link_cooldown_for_age(day), Some(LINK_COOLDOWN_YOUNG));
    assert_eq!(link_cooldown_for_age(6 * day), Some(LINK_COOLDOWN_YOUNG));
    // Established (7d+): no cooldown.
    assert_eq!(link_cooldown_for_age(7 * day), None);
    assert_eq!(link_cooldown_for_age(365 * day), None);
}

/// A body past the column's cap is a refusal that names the limit, never
/// the generic failure the insert's CHECK would produce. IRC sends land
/// here too, so the service is where it is checked.
#[tokio::test]
async fn a_body_past_the_cap_is_refused_by_name() {
    use late_core::models::chat_message::BODY_MAX_CHARS;
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "too_long").await;
    let room = ChatRoom::get_or_create_public_room(&client, "too-long-room")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join room");
    let notifications =
        crate::app::chat::notifications::svc::NotificationService::new(test_db.db.clone());
    let service = ChatService::new(test_db.db.clone(), notifications);

    let params = |body: String| SendMessageParams {
        user_id: user.id,
        room_id: room.id,
        room_slug: room.slug.clone(),
        body,
        reply_to_message_id: None,
        reply_to_user_id: None,
        is_admin: false,
        wording: Wording::Typed,
    };
    match service
        .send_message(params("x".repeat(BODY_MAX_CHARS + 1)))
        .await
    {
        Err(SendError::Refused(refusal)) => assert_eq!(
            refusal.message(),
            "Message too long: 2001 characters, the limit is 2000."
        ),
        other => panic!("expected a too-long refusal, got {other:?}"),
    }
    // Characters, not bytes: 2000 two-byte characters still fit.
    service
        .send_message(params("é".repeat(BODY_MAX_CHARS)))
        .await
        .expect("a body at the cap sends");
}

#[test]
fn send_refusal_explains_report_only_rooms() {
    let bugs = SendRefusal::ReportOnly(ReportKind::Bug).message();
    assert!(bugs.contains("#bugs"), "{bugs}");
    assert!(bugs.contains("/bug"), "{bugs}");
    let suggestions = SendRefusal::ReportOnly(ReportKind::Suggestion).message();
    assert!(suggestions.contains("#suggestions"), "{suggestions}");
    assert!(suggestions.contains("/suggest"), "{suggestions}");
}

/// The refusals that carry a wait say where it holds and for how long; a
/// send that named no room still gets a sentence.
#[test]
fn send_refusal_words_the_wait() {
    let in_room = SendRefusal::SlowMode {
        remaining: Duration::from_secs(90),
        scope: SlowModeScope::Room,
        room_slug: Some("lounge".to_string()),
    };
    assert_eq!(
        in_room.message(),
        "Slow mode in #lounge: wait 1m 30s before sending again."
    );
    let unnamed = SendRefusal::SlowMode {
        remaining: Duration::from_secs(5),
        scope: SlowModeScope::Room,
        room_slug: None,
    };
    assert_eq!(
        unnamed.message(),
        "Slow mode in this room: wait 5s before sending again."
    );
    let server = SendRefusal::SlowMode {
        remaining: Duration::from_secs(5),
        scope: SlowModeScope::Server,
        room_slug: Some("lounge".to_string()),
    };
    assert!(server.message().starts_with("Slow mode in server: "));
    let link = SendRefusal::LinkCooldown {
        remaining: Duration::from_secs(120),
    };
    assert!(
        link.message().ends_with("next link in 2m 00s."),
        "{}",
        link.message()
    );
}

#[test]
fn report_kind_maps_room_slugs() {
    assert_eq!(ReportKind::for_room_slug("bugs"), Some(ReportKind::Bug));
    assert_eq!(
        ReportKind::for_room_slug("suggestions"),
        Some(ReportKind::Suggestion)
    );
    assert_eq!(ReportKind::for_room_slug("lounge"), None);
}

/// Only the public topic rooms are report-only: a private room may share
/// the slug and stays free text.
#[test]
fn report_kind_enforces_public_topic_rooms_only() {
    let room = |kind: RoomKind, slug: &str| ChatRoom {
        id: Uuid::from_u128(1),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        kind,
        auto_join: false,
        permanent: false,
        slug: Some(slug.to_string()),
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        topic: None,
        rules: None,
        created_by: None,
    };
    assert_eq!(
        ReportKind::for_room(&room(RoomKind::Topic(RoomVisibility::Public), "bugs")),
        Some(ReportKind::Bug)
    );
    assert_eq!(
        ReportKind::for_room(&room(
            RoomKind::Topic(RoomVisibility::Public),
            "suggestions"
        )),
        Some(ReportKind::Suggestion)
    );
    assert_eq!(
        ReportKind::for_room(&room(RoomKind::Topic(RoomVisibility::Private), "bugs")),
        None
    );
    assert_eq!(
        ReportKind::for_room(&room(RoomKind::Game(RoomVisibility::Public), "bugs")),
        None
    );
    assert_eq!(
        ReportKind::for_room(&room(RoomKind::Topic(RoomVisibility::Public), "lounge")),
        None
    );
}

#[test]
fn format_cooldown_is_compact() {
    assert_eq!(format_cooldown(0), "1s");
    assert_eq!(format_cooldown(45), "45s");
    assert_eq!(format_cooldown(60), "1m 00s");
    assert_eq!(format_cooldown(29 * 60 + 30), "29m 30s");
}

fn test_poll(options: Vec<(&str, i64)>) -> ActiveChatPoll {
    let now = Utc::now();
    ActiveChatPoll {
        poll: ChatPoll {
            id: Uuid::from_u128(1),
            created: now,
            updated: now,
            room_id: Uuid::from_u128(2),
            user_id: Uuid::from_u128(3),
            question: "Which editor wins?".to_string(),
            starts_at: now - ChronoDuration::minutes(10),
            ends_at: now,
            active: false,
        },
        options: options
            .into_iter()
            .enumerate()
            .map(|(index, (label, vote_count))| ChatPollOptionSummary {
                id: Uuid::from_u128(10 + index as u128),
                position: (index + 1) as i32,
                label: label.to_string(),
                vote_count,
            })
            .collect(),
        my_vote_option_id: None,
        author_username: Some("polly".to_string()),
    }
}

#[test]
fn poll_results_message_reports_winner_and_percentages() {
    let poll = test_poll(vec![("vim", 4), ("emacs", 3), ("nano", 0)]);

    assert_eq!(
        format_poll_results_message(&poll),
        "---POLL RESULTS---\nWhich editor wins?\n1. vim - 4 votes (57%)\n2. emacs - 3 votes (43%)\n3. nano - 0 votes (0%)\nWinner: vim"
    );
}

#[test]
fn poll_results_message_reports_tie() {
    let poll = test_poll(vec![("vim", 2), ("emacs", 2)]);

    assert_eq!(
        format_poll_results_message(&poll),
        "---POLL RESULTS---\nWhich editor wins?\n1. vim - 2 votes (50%)\n2. emacs - 2 votes (50%)\nTie: vim, emacs"
    );
}

#[test]
fn poll_results_message_reports_no_votes() {
    let poll = test_poll(vec![("vim", 0), ("emacs", 0)]);

    assert_eq!(
        format_poll_results_message(&poll),
        "---POLL RESULTS---\nWhich editor wins?\n1. vim - 0 votes (0%)\n2. emacs - 0 votes (0%)\nWinner: no votes cast"
    );
}
