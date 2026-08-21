use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        notification::Notification,
    },
    test_utils::{create_test_user, test_db},
};

/// Reading the room the mention lives in clears it. The mention feed's own
/// cursor is a single global watermark that only moves when the Mentions entry
/// is opened, so a mention stayed on the rail badge forever if you read the
/// message where it was actually said.
#[tokio::test]
async fn mention_read_in_its_own_room_stops_counting_as_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "room-read-actor").await;
    let reader = create_test_user(&test_db.db, "room-read-reader").await;
    ChatRoomMember::join(&client, room.id, reader.id)
        .await
        .expect("join room");

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@room-read-reader over here".to_string(),
        },
    )
    .await
    .expect("create message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
        .await
        .expect("create mention");

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count before"),
        1
    );

    ChatRoomMember::mark_read_now(&client, room.id, reader.id)
        .await
        .expect("mark room read");

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count after"),
        0,
        "a mention read in its room still showed on the badge"
    );
}

/// The room cursor only ever clears a mention; never having opened the room
/// (no membership row at all) must keep the mention unread rather than swallow
/// it.
#[tokio::test]
async fn mention_in_a_never_opened_room_stays_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "no-member-actor").await;
    let reader = create_test_user(&test_db.db, "no-member-reader").await;
    ChatRoomMember::leave(&client, room.id, reader.id)
        .await
        .expect("leave room");

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@no-member-reader hello".to_string(),
        },
    )
    .await
    .expect("create message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
        .await
        .expect("create mention");

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count"),
        1
    );
}
