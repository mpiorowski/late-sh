use late_core::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        notification::Notification,
    },
    test_utils::create_test_user,
};

use super::NotificationService;
use crate::test_helpers::new_test_db;

/// Each user's list reaches that user even when another user's lands right
/// after it, before either session has ticked. On one shared watch the second
/// publish overwrote the first, and the first session's list was simply lost.
#[tokio::test]
async fn a_list_is_not_lost_to_another_users_list() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "list-race-actor").await;
    let first = create_test_user(&test_db.db, "list-race-first").await;
    let second = create_test_user(&test_db.db, "list-race-second").await;
    for reader in [&first, &second] {
        let message = ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: actor.id,
                body: format!("@{} hello", reader.username),
            },
        )
        .await
        .expect("create message");
        Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
            .await
            .expect("create mention");
    }

    let service = NotificationService::new(test_db.db.clone());
    let mut first_rx = service.subscribe_snapshot(first.id);
    let mut second_rx = service.subscribe_snapshot(second.id);
    service.do_list(first.id).await.expect("list first");
    service.do_list(second.id).await.expect("list second");

    assert!(first_rx.has_changed().expect("first channel open"));
    assert!(second_rx.has_changed().expect("second channel open"));
    let first_rows = first_rx.borrow_and_update().items.clone();
    let second_rows = second_rx.borrow_and_update().items.clone();
    assert_eq!(first_rows.len(), 1);
    assert_eq!(first_rows[0].user_id, first.id);
    assert_eq!(second_rows.len(), 1);
    assert_eq!(second_rows[0].user_id, second.id);
}
