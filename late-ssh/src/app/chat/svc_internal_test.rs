use super::*;
use chrono::Duration as ChronoDuration;
use late_core::models::chat_poll::{ChatPoll, ChatPollOptionSummary};

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

#[test]
fn send_error_message_explains_report_only_rooms() {
    let bugs = send_error_message(&anyhow::anyhow!("report-only:bugs"));
    assert!(bugs.contains("#bugs"), "{bugs}");
    assert!(bugs.contains("/bug"), "{bugs}");
    let suggestions = send_error_message(&anyhow::anyhow!("report-only:suggestions"));
    assert!(suggestions.contains("#suggestions"), "{suggestions}");
    assert!(suggestions.contains("/suggest"), "{suggestions}");
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
