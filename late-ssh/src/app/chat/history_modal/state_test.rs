use std::collections::HashMap;

use chrono::{Duration, Utc};
use late_core::models::chat_message::{ChatMessage, HistoryDirection};
use uuid::Uuid;

use super::{ChatHistoryModalState, HistoryStatus};

const ROOM: u128 = 1;
/// The session's own user; distinct from `message`'s author (7) so every
/// generated message counts as "authored by someone else".
const VIEWER: Uuid = Uuid::from_u128(42);

/// `index` doubles as the ordering key, so a run built with ascending indices
/// is already chronological.
fn message(index: u64) -> ChatMessage {
    let created = Utc::now() + Duration::seconds(index as i64);
    ChatMessage {
        id: Uuid::from_u128(1000 + index as u128),
        created,
        updated: created,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(ROOM),
        user_id: Uuid::from_u128(7),
        body: format!("message {index}"),
    }
}

fn run(range: std::ops::Range<u64>) -> Vec<ChatMessage> {
    range.map(message).collect()
}

/// Opens a tail modal already showing `count` messages, viewport parked at the
/// top so the older edge is live.
fn opened_at_tail(count: u64) -> ChatHistoryModalState {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    state.open_at_tail(
        Uuid::from_u128(ROOM),
        "#lounge".to_string(),
        request_id,
        VIEWER,
        None,
    );
    state.set_visible_rows(10);
    state.apply_page(
        request_id,
        HistoryDirection::Older,
        run(0..count),
        HashMap::new(),
    );
    state
}

#[test]
fn older_page_splices_on_top_and_holds_the_viewport() {
    let mut state = opened_at_tail(30);
    state.scroll(i32::MIN / 2);
    assert_eq!(state.scroll_index(), 0);
    let anchored_on = state.messages()[0].id;

    let request_id = Uuid::now_v7();
    state.begin_page(HistoryDirection::Older, request_id);
    state.apply_page(
        request_id,
        HistoryDirection::Older,
        run(100..112),
        HashMap::new(),
    );

    // The 12 older messages land in front, and the row the user was looking
    // at stays under their eyes rather than being shoved off-screen.
    assert_eq!(state.messages().len(), 42);
    assert_eq!(state.scroll_index(), 12);
    assert_eq!(state.messages()[state.scroll_index()].id, anchored_on);
}

#[test]
fn an_empty_page_retires_that_edge() {
    let mut state = opened_at_tail(30);
    state.scroll(i32::MIN / 2);
    assert!(state.wants_page(HistoryDirection::Older));

    let request_id = Uuid::now_v7();
    state.begin_page(HistoryDirection::Older, request_id);
    state.apply_page(
        request_id,
        HistoryDirection::Older,
        Vec::new(),
        HashMap::new(),
    );

    // Nothing more behind it, so sitting at the top must stop asking rather
    // than refetching an empty page on every keystroke.
    assert!(!state.wants_page(HistoryDirection::Older));
    assert_eq!(state.messages().len(), 30);
}

#[test]
fn a_tail_open_never_asks_for_newer_messages() {
    let mut state = opened_at_tail(30);
    state.scroll_to_bottom();

    // Opening at the tail means there is nothing newer by definition; asking
    // would be a guaranteed-empty round trip on every scroll to the bottom.
    assert!(!state.wants_page(HistoryDirection::Newer));
}

#[test]
fn a_stale_page_is_dropped() {
    let mut state = opened_at_tail(30);
    state.scroll(i32::MIN / 2);
    let request_id = Uuid::now_v7();
    state.begin_page(HistoryDirection::Older, request_id);

    // A page from a request this edge is no longer waiting on: the modal was
    // reopened elsewhere while it was in flight.
    state.apply_page(
        Uuid::now_v7(),
        HistoryDirection::Older,
        run(100..112),
        HashMap::new(),
    );

    assert_eq!(state.messages().len(), 30);
    assert_eq!(state.scroll_index(), 0);
}

#[test]
fn an_anchored_open_becomes_ready_and_marks_the_anchor() {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    let anchor_id = message(20).id;
    state.open_at_message(
        Uuid::from_u128(ROOM),
        "#lounge".to_string(),
        anchor_id,
        request_id,
        VIEWER,
        None,
    );
    state.apply_anchor(request_id, anchor_id, run(0..41), HashMap::new());

    assert_eq!(state.status(), HistoryStatus::Ready);
    assert_eq!(state.anchor_id(), Some(anchor_id));
    // Where the viewport lands is the renderer's job (the anchor centers by
    // wrapped lines); the ui tests pin that.
}

#[test]
fn a_missing_anchor_is_a_settled_state() {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    state.open_at_message(
        Uuid::from_u128(ROOM),
        "#lounge".to_string(),
        Uuid::from_u128(999),
        request_id,
        VIEWER,
        None,
    );
    state.apply_anchor_missing(request_id);

    assert_eq!(state.status(), HistoryStatus::AnchorMissing);
    // Settled, not transient: no edge should keep trying to page around a
    // message that does not exist.
    assert!(!state.wants_page(HistoryDirection::Older));
    assert!(!state.wants_page(HistoryDirection::Newer));
}

#[test]
fn an_unread_open_lands_on_the_anchor_when_the_service_finds_one() {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    let cutoff = Utc::now() - Duration::hours(1);
    state.open_at_unread(
        Uuid::from_u128(ROOM),
        "#lounge".to_string(),
        request_id,
        VIEWER,
        Some(cutoff),
    );
    let anchor_id = message(20).id;
    state.apply_anchor(request_id, anchor_id, run(0..41), HashMap::new());

    assert_eq!(state.status(), HistoryStatus::Ready);
    assert_eq!(state.anchor_id(), Some(anchor_id));
    // The anchor sits mid-history, so newer messages must remain reachable.
    state.set_visible_rows(10);
    state.scroll(i32::MAX / 2);
    assert!(state.wants_page(HistoryDirection::Newer));
}

#[test]
fn an_unread_open_falls_back_to_a_tail_page() {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    state.open_at_unread(
        Uuid::from_u128(ROOM),
        "#lounge".to_string(),
        request_id,
        VIEWER,
        Some(Utc::now()),
    );
    state.apply_page(
        request_id,
        HistoryDirection::Older,
        run(0..30),
        HashMap::new(),
    );

    // Nothing unread survived server-side: the open settles exactly like a
    // tail open, no anchor to highlight.
    assert_eq!(state.status(), HistoryStatus::Ready);
    assert_eq!(state.anchor_id(), None);
    assert_eq!(state.messages().len(), 30);
}

#[test]
fn unread_divider_targets_the_first_foreign_message_past_the_cutoff() {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    let mut messages = run(0..20);
    // The viewer's own message right past the cutoff must not take the
    // divider; the first message by someone else does.
    messages[10].user_id = VIEWER;
    // Cutoff at message 9: messages 10..20 count as unread.
    let cutoff = messages[9].created;
    state.open_at_tail(
        Uuid::from_u128(ROOM),
        "#lounge".to_string(),
        request_id,
        VIEWER,
        Some(cutoff),
    );
    state.apply_page(
        request_id,
        HistoryDirection::Older,
        messages,
        HashMap::new(),
    );

    assert_eq!(state.unread_divider_target(), Some(message(11).id));
}

#[test]
fn no_cutoff_means_no_divider() {
    let state = opened_at_tail(20);
    assert_eq!(state.unread_divider_target(), None);
}
