use late_core::{
    db::{Db, DbConfig},
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        notification::Notification,
        user::User,
    },
    test_utils::create_test_user,
};

use uuid::Uuid;

use super::{NotificationEvent, NotificationService};
use crate::test_helpers::new_test_db;

/// One connection, so a step holding its own while the next checks one out
/// waits on itself until checkout times out.
fn pool_of_one(db: &Db) -> Db {
    Db::new(&DbConfig {
        max_pool_size: 1,
        ..db.config().clone()
    })
    .expect("pool of one")
}

/// A mention of `reader`, returning the message it rides on.
async fn mention(db: &Db, actor_id: Uuid, reader: &User) -> Uuid {
    let client = db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor_id,
            body: format!("@{} hello", reader.username),
        },
    )
    .await
    .expect("create message");
    Notification::create_mentions_batch(&client, &[reader.id], actor_id, message.id, room.id)
        .await
        .expect("create mention");
    message.id
}

/// A stamp reaches the list as well as the count. Sessions sample the count once
/// a second to decide when to refetch, so a count that moved and came back
/// inside that second left the read row on the list for good, with the rail
/// saying 0.
#[tokio::test]
async fn stamping_a_mention_delivers_the_read_row_on_a_pool_of_one() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "stamp-svc-actor").await;
    let reader = create_test_user(&test_db.db, "stamp-svc-reader").await;
    let message_id = mention(&test_db.db, actor.id, &reader).await;

    let service = NotificationService::new(pool_of_one(&test_db.db));
    let mut rows_rx = service.subscribe_snapshot(reader.id);
    service
        .do_mark_read_for_messages(reader.id, &[message_id])
        .await
        .expect("stamp the mention");

    assert!(rows_rx.has_changed().expect("channel open"));
    let rows = rows_rx.borrow_and_update().items.clone();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].read_at.is_some());

    service
        .do_mark_read_for_messages(reader.id, &[message_id])
        .await
        .expect("stamp it again");
    assert!(
        !rows_rx.has_changed().expect("channel open"),
        "a stamp that moved nothing lists nothing"
    );
}

/// Opening the feed reaches the session in full: the rows it stamped, then the
/// new count, then the list, on a pool of one so no step holds its connection
/// across the next.
#[tokio::test]
async fn opening_the_feed_delivers_the_read_rows_on_a_pool_of_one() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "open-feed-svc-actor").await;
    let reader = create_test_user(&test_db.db, "open-feed-svc-reader").await;
    mention(&test_db.db, actor.id, &reader).await;

    let service = NotificationService::new(pool_of_one(&test_db.db));
    let mut rows_rx = service.subscribe_snapshot(reader.id);
    let mut events = service.subscribe_events();

    service
        .do_open_feed(reader.id)
        .await
        .expect("open the feed");

    let rows = rows_rx.borrow_and_update().items.clone();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].read_at.is_some(), "the list never brings it back");
    match events.try_recv().expect("the stamp is announced") {
        NotificationEvent::FeedOpened { user_id, read } => {
            assert_eq!(user_id, reader.id);
            assert_eq!(read, vec![rows[0].id]);
        }
        other => panic!("expected FeedOpened first, got {other:?}"),
    }
    match events.try_recv().expect("the count is announced") {
        NotificationEvent::UnreadCountUpdated {
            user_id,
            unread_count,
        } => {
            assert_eq!(user_id, reader.id);
            assert_eq!(unread_count, 0);
        }
        other => panic!("expected UnreadCountUpdated next, got {other:?}"),
    }
}

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
