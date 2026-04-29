use late_core::{
    models::{
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

    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");

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
    let counts = ChatRoomMember::unread_counts_for_user(&client, user.id)
        .await
        .unwrap();
    assert_eq!(counts.get(&room.id), Some(&0));
}

#[tokio::test]
async fn room_bans_block_join_and_auto_join() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");
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
