use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        room_ban::RoomBan,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_chat_room_member() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "member-user-1".to_string(),
            username: "m1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    // auto join public
    ChatRoomMember::auto_join_public_rooms(&client, user.id)
        .await
        .unwrap();

    assert!(
        ChatRoomMember::is_member(&client, room.id, user.id)
            .await
            .unwrap()
    );

    let ids = ChatRoomMember::list_user_ids(&client, room.id)
        .await
        .unwrap();
    assert!(ids.contains(&user.id));

    ChatRoomMember::mark_read_now(&client, room.id, user.id)
        .await
        .unwrap();
    let counts = ChatRoomMember::unread_counts_for_user(&client, user.id, None)
        .await
        .unwrap();
    assert_eq!(counts.get(&room.id), Some(&0));
}

#[tokio::test]
async fn room_bans_block_join_and_auto_join() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "member-banned-user".to_string(),
            username: "banned_member".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user");

    RoomBan::activate(&client, room.id, user.id, user.id, "test ban", None)
        .await
        .expect("activate ban");

    assert!(
        ChatRoomMember::join(&client, room.id, user.id)
            .await
            .is_err()
    );
    let _joined = ChatRoomMember::auto_join_public_rooms(&client, user.id)
        .await
        .expect("auto join public rooms");
    assert!(
        !ChatRoomMember::is_member(&client, room.id, user.id)
            .await
            .expect("membership lookup")
    );
}

#[tokio::test]
async fn unread_count_stops_at_the_cap() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let reader = User::create(
        &client,
        UserParams {
            fingerprint: "unread-cap-reader".to_string(),
            username: "cap_reader".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create reader");
    let author = User::create(
        &client,
        UserParams {
            fingerprint: "unread-cap-author".to_string(),
            username: "cap_author".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create author");
    ChatRoomMember::join(&client, room.id, reader.id)
        .await
        .expect("join reader");

    let over_cap = ChatRoomMember::UNREAD_COUNT_CAP + 20;
    for n in 0..over_cap {
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: author.id,
                body: format!("message {n}"),
            },
        )
        .await
        .expect("create message");
    }

    let counts = ChatRoomMember::unread_counts_for_user(&client, reader.id, None)
        .await
        .expect("unread counts");

    assert_eq!(
        counts.get(&room.id),
        Some(&ChatRoomMember::UNREAD_COUNT_CAP),
        "a room with more unread than the cap reports exactly the cap, not the true total"
    );
}

#[tokio::test]
async fn system_activity_lines_do_not_count_as_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let reader = User::create(
        &client,
        UserParams {
            fingerprint: "unread-system-reader".to_string(),
            username: "sys_reader".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create reader");
    let system = User::create(
        &client,
        UserParams {
            fingerprint: "unread-system-bot".to_string(),
            username: "sys_bot".to_string(),
            settings: serde_json::json!({ "bot": true, "system": true }),
        },
    )
    .await
    .expect("create system user");
    ChatRoomMember::join(&client, room.id, reader.id)
        .await
        .expect("join reader");

    // An ambient activity line, and a real message from the same author.
    for body in ["· someone joined", "heads up: new public room opened"] {
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: system.id,
                body: body.to_string(),
            },
        )
        .await
        .expect("create message");
    }

    let counts = ChatRoomMember::unread_counts_for_user(&client, reader.id, Some(system.id))
        .await
        .expect("unread counts");
    assert_eq!(
        counts.get(&room.id),
        Some(&1),
        "the `· ` activity line is excluded, the bot's real message still counts"
    );

    // Without the system id the bot is just another author, so both count.
    let counts = ChatRoomMember::unread_counts_for_user(&client, reader.id, None)
        .await
        .expect("unread counts");
    assert_eq!(
        counts.get(&room.id),
        Some(&2),
        "with no system user known, nothing is excluded"
    );
}
