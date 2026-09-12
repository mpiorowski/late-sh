use super::{is_next_room_key, is_prev_room_key, leader_reaction_emoji, resolve_status_change};
use crate::app::chat::state::StatusChange;
use crate::app::common::status::Status;
use chrono::Utc;

#[test]
fn next_room_keys_include_ctrl_n() {
    assert!(is_next_room_key(b'l'));
    assert!(is_next_room_key(b'L'));
    assert!(is_next_room_key(0x0E));
    assert!(!is_next_room_key(b'h'));
}

#[test]
fn prev_room_keys_include_ctrl_p() {
    assert!(is_prev_room_key(b'h'));
    assert!(is_prev_room_key(b'H'));
    assert!(is_prev_room_key(0x10));
    assert!(!is_prev_room_key(b'l'));
}

#[test]
fn leader_reaction_keys_are_plain_digits_except_custom_zero() {
    assert_eq!(leader_reaction_emoji(b'0'), None);
    assert_eq!(leader_reaction_emoji(b'1'), Some("👍"));
    assert_eq!(leader_reaction_emoji(b'5'), Some("🔥"));
    assert_eq!(leader_reaction_emoji(b'6'), Some("🙌"));
    assert_eq!(leader_reaction_emoji(b'7'), Some("🚀"));
    assert_eq!(leader_reaction_emoji(b'8'), Some("🤔"));
    assert_eq!(leader_reaction_emoji(b'9'), Some("💩"));
    assert_eq!(leader_reaction_emoji(b'!'), None);
}

fn set(status: Status, minutes: Option<u32>) -> StatusChange {
    StatusChange::Set { status, minutes }
}

#[test]
fn a_timed_status_ends_at_the_callers_clock_and_says_the_rule() {
    let now = Utc::now();

    let (status, banner) = resolve_status_change(None, set(Status::Focus, Some(50)), now);
    let armed = status.expect("set should arm a status");
    assert_eq!(armed.status, Status::Focus);
    // The duration is measured from the caller's clock, not re-read inside.
    assert_eq!(armed.ends_at, Some(now + chrono::Duration::minutes(50)));
    assert!(!armed.clears_on_post());
    assert_eq!(banner.message, "🍅 focus, clears in 50m, stays while you chat");
}

/// The other half of the rule, and the half nobody can infer from the badge:
/// no minutes means the next message clears it, so the banner has to say so.
#[test]
fn an_open_ended_status_carries_no_deadline_and_says_the_rule() {
    let now = Utc::now();

    let (status, banner) = resolve_status_change(None, set(Status::Away, None), now);
    let armed = status.expect("set should arm a status");
    assert_eq!(armed.ends_at, None);
    assert!(armed.clears_on_post());
    assert_eq!(banner.message, "💤 away, clears when you next post");
}

/// A second set replaces the running one instead of being refused, and the
/// banner has to say what the new one does: silently restarting a 50 minute
/// countdown as a 25 minute one is the kind of thing you only notice at the
/// wrong moment.
#[test]
fn setting_a_status_replaces_the_running_one() {
    let now = Utc::now();
    let (first, _) = resolve_status_change(None, set(Status::Focus, Some(50)), now);

    let (second, banner) = resolve_status_change(first, set(Status::Gaming, Some(5)), now);
    let armed = second.expect("a replacement should stay armed");
    assert_eq!(armed.status, Status::Gaming);
    assert_eq!(armed.ends_at, Some(now + chrono::Duration::minutes(5)));
    assert_eq!(banner.message, "👾 gaming, clears in 5m, stays while you chat");
}

#[test]
fn clearing_reports_what_was_cleared() {
    let now = Utc::now();
    let (running, _) = resolve_status_change(None, set(Status::Focus, Some(25)), now);

    let (cleared, banner) = resolve_status_change(running, StatusChange::Clear, now);
    assert!(cleared.is_none(), "clear should empty the status");
    assert_eq!(banner.message, "cleared focus");
}

/// Clearing nothing is a user error, not a silent no-op: without the banner
/// there is no feedback at all, because the HUD badge was already absent.
#[test]
fn clearing_with_no_status_set_reports_it() {
    let (status, banner) = resolve_status_change(None, StatusChange::Clear, Utc::now());
    assert!(
        banner.message.contains("no status set"),
        "expected a usage banner, got: {}",
        banner.message
    );
    assert!(status.is_none());
}

/// `g` on a message in a DM, a private room, or a game/stream chat refuses
/// before the picker opens: the service would refuse anyway, but a purchase
/// modal that can never complete must not open at all. A public topic room
/// still gets the picker.
#[tokio::test]
async fn gild_key_refuses_before_opening_the_picker_outside_public_rooms() {
    use late_core::models::{chat_message::ChatMessage, chat_room::ChatRoom};
    use uuid::Uuid;

    let db = crate::test_helpers::new_test_db().await;
    let mut app = crate::test_helpers::make_app(db.db.clone(), Uuid::now_v7(), "gild-preflight");

    for (kind, visibility, refusal) in [
        ("dm", "dm", Some("Gilds only work in public rooms")),
        ("topic", "private", Some("Gilds only work in public rooms")),
        (
            "game",
            "public",
            Some("Gilds do not work in game or stream chats"),
        ),
        ("topic", "public", None),
    ] {
        let opens = refusal.is_none();
        let room_id = Uuid::now_v7();
        let message_id = Uuid::now_v7();
        let room = ChatRoom {
            id: room_id,
            created: Utc::now(),
            updated: Utc::now(),
            kind: kind.to_string(),
            visibility: visibility.to_string(),
            auto_join: false,
            permanent: false,
            slug: None,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
            topic: None,
            rules: None,
            created_by: None,
        };
        let message = ChatMessage {
            id: message_id,
            created: Utc::now(),
            updated: Utc::now(),
            reply_to_message_id: None,
            reply_to_user_id: None,
            room_id,
            user_id: Uuid::now_v7(),
            body: "worth paying for".to_string(),
        };
        app.chat.rooms.push((room, vec![message]));
        assert!(app.chat.select_message_by_id_in_room(room_id, message_id));
        app.banner = None;

        assert!(
            super::handle_message_action_in_room(&mut app, room_id, b'g'),
            "`g` is consumed whenever a message is selected ({kind}/{visibility})"
        );
        assert_eq!(app.show_gild_modal, opens, "{kind}/{visibility}");
        assert_eq!(
            app.banner.as_ref().map(|banner| banner.message.as_str()),
            refusal,
            "{kind}/{visibility}"
        );
        crate::app::chat::gild::input::close(&mut app);
    }
}
