use late_core::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        mention_feed_read::MentionFeedRead,
        notification::Notification,
    },
    test_utils::{create_test_user, test_db},
};
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn mention_feed_unread_uses_timestamp_cursor() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");
    let actor = create_test_user(&test_db.db, "mention-actor").await;
    let reader = create_test_user(&test_db.db, "mention-reader").await;

    let first = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@mention-reader one".to_string(),
        },
    )
    .await
    .expect("create first message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, first.id, room.id)
        .await
        .expect("create first mention");

    let second = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@mention-reader two".to_string(),
        },
    )
    .await
    .expect("create second message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, second.id, room.id)
        .await
        .expect("create second mention");

    let unread_before = MentionFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread before");
    assert_eq!(unread_before, 2);

    MentionFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("mark read");
    let read_cursor = MentionFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("read cursor");
    assert!(read_cursor.is_some());

    let unread_after = MentionFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after mark read");
    assert_eq!(unread_after, 0);

    sleep(Duration::from_millis(5)).await;

    let third = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@mention-reader three".to_string(),
        },
    )
    .await
    .expect("create third message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, third.id, room.id)
        .await
        .expect("create third mention");

    let unread_after_new = MentionFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after new mention");
    assert_eq!(unread_after_new, 1);
}
