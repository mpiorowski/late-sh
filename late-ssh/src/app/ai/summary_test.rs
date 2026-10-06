use std::collections::HashMap;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use late_core::db::{Db, DbConfig};
use late_core::models::chat_message::ChatMessage;
use uuid::Uuid;

use super::{
    Reservation, SUMMARY_DEFAULT_WINDOW_HOURS, SUMMARY_MAX_WINDOW_HOURS,
    SUMMARY_PROMPT_CHAR_BUDGET, SummaryBasis, SummaryOutcome, SummaryService, SummaryWindow,
    build_transcript, window_start,
};
use crate::app::ai::svc::AiService;

// The inert-Db exception from the root test policy: these tests exercise the
// guardrail paths (AI disabled, cooldown, daily cap), all of which settle
// before any DB access.
fn inert_service(ai_enabled_with_key: bool) -> SummaryService {
    let db = Db::new(&DbConfig::default()).expect("inert pool");
    let ai = if ai_enabled_with_key {
        AiService::new(true, Some("test-key".to_string()))
    } else {
        AiService::new(false, None)
    };
    SummaryService::new(db, ai)
}

fn message(index: u64, author: Uuid, body: &str) -> ChatMessage {
    let created = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
        + chrono::Duration::seconds(index as i64);
    ChatMessage {
        id: Uuid::from_u128(1000 + index as u128),
        created,
        updated: created,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(1),
        user_id: author,
        body: body.to_string(),
    }
}

#[test]
fn a_bare_catch_up_starts_where_the_reader_last_left() {
    let now = chrono::Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
    let max = now - chrono::Duration::hours(SUMMARY_MAX_WINDOW_HOURS);

    // The whole point of the device mark: a recent one means a short
    // catch-up. Nothing widens it, because it is not a guess about what was
    // read, it is when the reader left.
    let ten_minutes_ago = now - chrono::Duration::minutes(10);
    assert_eq!(
        window_start(SummaryWindow::SinceLeftApp(ten_minutes_ago), now),
        (ten_minutes_ago, SummaryBasis::LeftApp, false)
    );
    let yesterday = now - chrono::Duration::hours(30);
    assert_eq!(
        window_start(SummaryWindow::SinceLeftApp(yesterday), now),
        (yesterday, SummaryBasis::LeftApp, false)
    );

    // Past the max, cost policy wins, and the cap is reported so the head
    // does not present the capped stamp as the moment the reader left.
    assert_eq!(
        window_start(
            SummaryWindow::SinceLeftApp(now - chrono::Duration::days(9)),
            now
        ),
        (max, SummaryBasis::LeftApp, true)
    );
    assert_eq!(
        window_start(SummaryWindow::SinceLeftApp(max), now),
        (max, SummaryBasis::LeftApp, false),
        "exactly at the cap is not capped"
    );

    // No mark: the one window that is handed out rather than derived, and
    // it says so.
    assert_eq!(
        window_start(SummaryWindow::Default, now),
        (
            now - chrono::Duration::hours(SUMMARY_DEFAULT_WINDOW_HOURS),
            SummaryBasis::Default,
            false
        )
    );
}

#[test]
fn an_explicit_window_is_taken_at_face_value_up_to_the_max() {
    let now = chrono::Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
    // The window the reader typed is the window they get.
    let explicit = |back| window_start(SummaryWindow::Explicit(back), now);

    assert_eq!(
        explicit(chrono::Duration::hours(6)),
        (
            now - chrono::Duration::hours(6),
            SummaryBasis::Explicit,
            false
        )
    );
    assert_eq!(
        explicit(chrono::Duration::minutes(90)),
        (
            now - chrono::Duration::minutes(90),
            SummaryBasis::Explicit,
            false
        )
    );
    // The max is still the max, and an absurd ask clamps to it rather than
    // overflowing the subtraction. The command layer refuses anything past
    // the max before it gets here, so this is not reported as a cap.
    assert_eq!(
        explicit(chrono::Duration::hours(SUMMARY_MAX_WINDOW_HOURS + 1)),
        (
            now - chrono::Duration::hours(SUMMARY_MAX_WINDOW_HOURS),
            SummaryBasis::Explicit,
            false
        )
    );
    assert_eq!(
        explicit(chrono::Duration::MAX),
        (
            now - chrono::Duration::hours(SUMMARY_MAX_WINDOW_HOURS),
            SummaryBasis::Explicit,
            false
        )
    );
}

#[test]
fn transcript_is_oldest_first_with_author_names() {
    let alice = Uuid::from_u128(7);
    let usernames = HashMap::from([(alice, "alice".to_string())]);
    let messages = vec![
        message(0, alice, "first"),
        message(1, alice, "second"),
        message(2, Uuid::from_u128(8), "third"),
    ];

    let (transcript, count, cut) = build_transcript(&messages, &usernames);

    assert_eq!(count, 3);
    assert!(!cut);
    let lines: Vec<&str> = transcript.lines().collect();
    assert!(lines[0].ends_with("alice: first"), "got {:?}", lines[0]);
    assert!(lines[1].ends_with("alice: second"));
    // Unknown authors render as `?` instead of vanishing.
    assert!(lines[2].ends_with("?: third"), "got {:?}", lines[2]);
}

#[test]
fn transcript_budget_drops_the_oldest_end_and_reports_the_cut() {
    let author = Uuid::from_u128(7);
    let usernames = HashMap::from([(author, "a".to_string())]);
    // Each body is ~2k chars, so ~100 messages fit the budget; build more.
    let big_body = "x".repeat(2_000);
    let messages: Vec<ChatMessage> = (0..150)
        .map(|index| message(index, author, &big_body))
        .collect();

    let (transcript, count, cut) = build_transcript(&messages, &usernames);

    assert!(cut);
    assert!(count < messages.len());
    assert!(transcript.len() <= SUMMARY_PROMPT_CHAR_BUDGET);
    // The newest message survives; the cut came off the oldest end.
    let last_kept = transcript.lines().last().unwrap();
    assert!(last_kept.starts_with("[08-01 12:02"), "got {last_kept:?}");
}

#[tokio::test]
async fn ai_disabled_answers_unavailable_without_touching_the_db() {
    let service = inert_service(false);
    let mut events = service.subscribe();

    service.request(
        Uuid::from_u128(1),
        Uuid::from_u128(2),
        "#lounge".to_string(),
        SummaryWindow::Default,
        Vec::new(),
    );

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event within timeout")
        .expect("channel open");
    assert_eq!(event.user_id, Uuid::from_u128(1));
    assert!(matches!(event.outcome, SummaryOutcome::Unavailable));
}

#[tokio::test]
async fn an_armed_cooldown_refuses_before_any_work() {
    let service = inert_service(true);
    let user = Uuid::from_u128(1);
    let room = Uuid::from_u128(2);
    service.finish_slot(user, room);
    let mut events = service.subscribe();

    service.request(
        user,
        room,
        "#lounge".to_string(),
        SummaryWindow::Default,
        Vec::new(),
    );

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event within timeout")
        .expect("channel open");
    let SummaryOutcome::Cooldown { remaining } = event.outcome else {
        panic!("expected cooldown, got {:?}", event.outcome);
    };
    assert!(remaining <= super::SUMMARY_COOLDOWN);

    // A different room for the same user is not throttled by that cooldown.
    assert!(matches!(
        service.reserve_slot(user, Uuid::from_u128(3)),
        Reservation::Reserved
    ));
}

#[tokio::test]
async fn an_in_flight_request_collapses_duplicates_without_spending() {
    let service = inert_service(true);
    let user = Uuid::from_u128(1);
    let room = Uuid::from_u128(2);
    // The first request holds the slot for the whole fetch-and-call span.
    assert!(matches!(
        service.reserve_slot(user, room),
        Reservation::Reserved
    ));
    let mut events = service.subscribe();

    // A duplicate submitted while it runs answers InFlight before any work:
    // no fetch, no daily-cap spend, no model call.
    service.request(
        user,
        room,
        "#lounge".to_string(),
        SummaryWindow::Default,
        Vec::new(),
    );

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event within timeout")
        .expect("channel open");
    assert!(matches!(event.outcome, SummaryOutcome::InFlight));
}

#[test]
fn a_released_slot_allows_the_retry_a_finished_one_refuses() {
    let service = inert_service(true);
    let user = Uuid::from_u128(1);
    let room = Uuid::from_u128(2);

    // A failed request releases the slot: `/summary` stays its own retry.
    assert!(matches!(
        service.reserve_slot(user, room),
        Reservation::Reserved
    ));
    service.release_slot(user, room);
    assert!(matches!(
        service.reserve_slot(user, room),
        Reservation::Reserved
    ));

    // A delivered one arms the cooldown instead.
    service.finish_slot(user, room);
    assert!(matches!(
        service.reserve_slot(user, room),
        Reservation::Cooldown(remaining) if remaining <= super::SUMMARY_COOLDOWN
    ));
}

#[test]
fn daily_cap_spends_then_refuses_until_rollover() {
    let service = inert_service(true);
    for _ in 0..super::SUMMARY_DAILY_CAP {
        assert!(service.spend_from_daily_cap());
    }
    assert!(!service.spend_from_daily_cap());
}
