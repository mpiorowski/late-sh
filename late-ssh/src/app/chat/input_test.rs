use super::{is_next_room_key, is_prev_room_key, leader_reaction_emoji};
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
