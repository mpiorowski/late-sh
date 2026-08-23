use std::collections::HashMap;

use chrono::{Duration, TimeZone, Utc};
use late_core::models::chat_message::{ChatMessage, HistoryDirection};
use uuid::Uuid;

use super::{fill_start, resolve_viewport, wrapped_body};
use crate::app::chat::history_modal::state::ChatHistoryModalState;

/// Fixed base time so every message lands on the same date: a `Utc::now`
/// base would flake across midnight by adding or removing day separators.
fn message(index: u64, body: &str) -> ChatMessage {
    let created =
        Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap() + Duration::seconds(index as i64);
    ChatMessage {
        id: Uuid::from_u128(1000 + index as u128),
        created,
        updated: created,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(1),
        user_id: Uuid::from_u128(7),
        body: body.to_string(),
    }
}

/// A tail-opened modal holding `bodies` in order, in the running app's
/// order: open, then the page lands, and no frame has reported a pane size
/// yet. No usernames are loaded, so every author renders as `?` and line
/// math is width-stable.
fn opened_with(bodies: &[&str]) -> ChatHistoryModalState {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    state.open_at_tail(
        Uuid::from_u128(1),
        "#lounge".to_string(),
        request_id,
        Uuid::from_u128(42),
        None,
    );
    let messages = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| message(index as u64, body))
        .collect();
    state.apply_page(
        request_id,
        HistoryDirection::Older,
        messages,
        HashMap::new(),
    );
    state
}

#[test]
fn a_tail_open_lands_on_a_full_pane_with_the_scroll_index_aligned() {
    let bodies: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let state = opened_with(&refs);

    // A 10-row pane spends 1 row on the day separator, leaving 9 one-line
    // messages: the first frame walks back from the tail to fill exactly
    // that, and the scroll index lands on the first drawn row so the very
    // first Up keypress moves the view instead of dying in the clamp.
    assert_eq!(resolve_viewport(&state, 10, 40), 11);
    assert_eq!(state.scroll_index(), 11);
}

#[test]
fn an_anchored_open_centers_the_anchor_by_wrapped_lines() {
    let mut state = ChatHistoryModalState::default();
    let request_id = Uuid::now_v7();
    let anchor_id = Uuid::from_u128(1000 + 20);
    state.open_at_message(
        Uuid::from_u128(1),
        "#lounge".to_string(),
        anchor_id,
        request_id,
        Uuid::from_u128(42),
        None,
    );
    let messages = (0..41).map(|i| message(i, &format!("m{i}"))).collect();
    state.apply_anchor(request_id, anchor_id, messages, HashMap::new());

    // Half a 10-row pane is 5 lines of one-line messages above the anchor,
    // so the window opens at message 15 and the anchor sits mid-pane with
    // its preceding context on screen.
    assert_eq!(resolve_viewport(&state, 10, 40), 15);
    assert_eq!(state.scroll_index(), 15);
}

#[test]
fn back_fill_counts_wrapped_heights_not_messages() {
    let bodies: Vec<&str> = (0..20).map(|_| "aaaa bbbb cccc dddd").collect();
    let state = opened_with(&bodies);
    // Sanity: at this width each `?: ...` body wraps to 3 lines, so the fit
    // below is measured in lines, not messages.
    assert_eq!(wrapped_body(&state, &state.messages()[0], 10).len(), 3);

    // 10 rows minus the separator fits three 3-line messages; a fourth would
    // overflow.
    assert_eq!(resolve_viewport(&state, 10, 10), 17);
}

#[test]
fn a_scrolled_up_viewport_is_not_back_filled() {
    let bodies: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let mut state = opened_with(&refs);
    state.scroll(i32::MIN / 2);

    // Mid-history the window below the scroll index already overflows the
    // pane, so the top row stays exactly where the user scrolled it.
    assert_eq!(fill_start(&state, 10, 40), 0);
    assert_eq!(resolve_viewport(&state, 10, 40), 0);
}

#[test]
fn a_whitespace_only_body_still_renders_one_row() {
    let state = opened_with(&["   "]);
    // `wrap_plain_line` drops blank text; without the guard the message
    // would vanish from the run entirely.
    assert_eq!(wrapped_body(&state, &state.messages()[0], 40).len(), 1);
}
