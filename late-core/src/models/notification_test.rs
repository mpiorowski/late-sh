use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        notification::Notification,
    },
    test_utils::{create_test_user, test_db},
};

/// Rendering the message a mention rides on clears that mention and only that
/// mention. The mention feed's own cursor is a single global watermark that
/// only moves when the Mentions entry is opened, so a mention stayed on the
/// rail badge forever if you read the message where it was actually said. The
/// room's coarse read cursor is deliberately not the fix: it moves whenever
/// the room is merely opened, which would also clear mentions above the
/// loaded tail that were never on screen.
#[tokio::test]
async fn rendering_a_mention_clears_it_and_leaves_unrendered_ones_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "render-read-actor").await;
    let reader = create_test_user(&test_db.db, "render-read-reader").await;

    let old_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@render-read-reader way up in the backlog".to_string(),
        },
    )
    .await
    .expect("create old message");
    let seen_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@render-read-reader over here".to_string(),
        },
    )
    .await
    .expect("create seen message");
    for message in [&old_message, &seen_message] {
        Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
            .await
            .expect("create mention");
    }

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count before"),
        2
    );

    // Only the newer message was rendered; the old one sits above the tail.
    let cleared = Notification::mark_read_for_messages(&client, reader.id, &[seen_message.id])
        .await
        .expect("mark rendered mention read");
    assert_eq!(cleared, 1);

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count after"),
        1,
        "the unrendered mention above the tail must stay unread"
    );

    let listed = Notification::list_for_user(&client, reader.id, 10)
        .await
        .expect("list");
    let read_ids: Vec<_> = listed
        .iter()
        .filter(|view| view.read_at.is_some())
        .map(|view| view.message_id)
        .collect();
    assert_eq!(
        read_ids,
        vec![seen_message.id],
        "only the rendered mention should carry a read stamp"
    );
}

/// The read stamp is owner-scoped in the query itself: another user marking
/// the same message read must not clear this user's mention.
#[tokio::test]
async fn marking_read_is_scoped_to_the_mentioned_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "scope-actor").await;
    let reader = create_test_user(&test_db.db, "scope-reader").await;
    let other = create_test_user(&test_db.db, "scope-other").await;

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@scope-reader hello".to_string(),
        },
    )
    .await
    .expect("create message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
        .await
        .expect("create mention");

    let cleared = Notification::mark_read_for_messages(&client, other.id, &[message.id])
        .await
        .expect("mark as the wrong user");
    assert_eq!(cleared, 0);

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count"),
        1,
        "another user's read must not clear this user's mention"
    );

    // Repeating the stamp as the right user clears it exactly once.
    let cleared = Notification::mark_read_for_messages(&client, reader.id, &[message.id])
        .await
        .expect("mark as the mentioned user");
    assert_eq!(cleared, 1);
    let cleared = Notification::mark_read_for_messages(&client, reader.id, &[message.id])
        .await
        .expect("mark again");
    assert_eq!(cleared, 0, "an already-read mention is a no-op");
}
