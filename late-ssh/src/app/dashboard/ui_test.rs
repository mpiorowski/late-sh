use super::*;
use chrono::Utc;
use late_core::models::chat_message::ChatMessage;
use uuid::Uuid;

const TEST_WIDTH: u16 = 80;

fn pin(body: &str) -> ChatMessage {
    let now = Utc::now();
    ChatMessage {
        id: Uuid::nil(),
        created: now,
        updated: now,
        pinned: true,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::nil(),
        user_id: Uuid::nil(),
        body: body.to_string(),
    }
}

#[test]
fn dashboard_pinned_height_zero_without_pins() {
    assert_eq!(dashboard_pinned_height(40, TEST_WIDTH, &[]), 0);
}

#[test]
fn dashboard_pinned_height_present_when_space_allows() {
    let pins = [pin("hello")];
    let height = dashboard_pinned_height(40, TEST_WIDTH, &pins);
    assert!(height > 0);
}

#[test]
fn dashboard_pinned_height_yields_to_minimum_chat() {
    let pins = [pin("hello")];
    assert_eq!(
        dashboard_pinned_height(MIN_CHAT_HEIGHT_WITH_LOUNGE, TEST_WIDTH, &pins),
        0
    );
}

#[test]
fn pinned_natural_height_wraps_and_sums() {
    let pins = [
        pin("short"),
        pin(&"word ".repeat(40)), // forces multi-line wrap at width 80
    ];
    let height = pinned_natural_height(&pins, TEST_WIDTH);
    assert!(height >= 2, "expected wrapping to add rows, got {height}");
    assert!(height <= MAX_PINNED_HEIGHT);
}

#[test]
fn pinned_natural_height_caps_at_max() {
    let pins: Vec<ChatMessage> = (0..20).map(|i| pin(&format!("pin {i}"))).collect();
    let height = pinned_natural_height(&pins, TEST_WIDTH);
    assert_eq!(height, MAX_PINNED_HEIGHT);
}
