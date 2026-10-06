use std::collections::HashMap;

use chrono::TimeZone;
use uuid::Uuid;

use super::*;

const ALICE: Uuid = Uuid::from_u128(7);

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
}

fn message(seconds_ago: i64, body: &str) -> ChatMessage {
    let created = now() - Duration::seconds(seconds_ago);
    ChatMessage {
        id: Uuid::from_u128(1000 + seconds_ago as u128),
        created,
        updated: created,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(1),
        user_id: ALICE,
        body: body.to_string(),
    }
}

fn names() -> HashMap<Uuid, String> {
    HashMap::from([(ALICE, "alice".to_string())])
}

#[test]
fn the_line_is_the_newest_message_on_one_row() {
    let names = names();
    let usernames = UsernameLookup::new(&names, None);
    let messages = [message(125, "nice\n  sling!"), message(300, "older")];
    assert_eq!(
        latest_line(&messages, &usernames, now()),
        Some(WatchLine {
            author: "alice".to_string(),
            body: "nice sling!".to_string(),
            is_action: false,
            age: "2m".to_string(),
        })
    );
}

#[test]
fn a_me_action_reads_as_an_action() {
    let names = names();
    let usernames = UsernameLookup::new(&names, None);
    let body = crate::app::chat::action::encode_action_body("winces").unwrap();
    let line = latest_line(&[message(5, &body)], &usernames, now()).unwrap();
    assert_eq!(
        (line.body.as_str(), line.is_action, line.age.as_str()),
        ("winces", true, "now")
    );
}

#[test]
fn a_stale_or_empty_room_shows_no_line() {
    let names = names();
    let usernames = UsernameLookup::new(&names, None);
    assert_eq!(latest_line(&[], &usernames, now()), None);
    let stale = [message(LINE_FRESH_MINUTES * 60 + 1, "gg")];
    assert_eq!(latest_line(&stale, &usernames, now()), None);
    let fresh = [message(LINE_FRESH_MINUTES * 60, "gg")];
    assert!(latest_line(&fresh, &usernames, now()).is_some());
}
