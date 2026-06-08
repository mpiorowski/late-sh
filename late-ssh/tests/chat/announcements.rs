use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::ChatRoom,
};
use late_core::test_utils::create_test_user;
use late_ssh::app::announcements::load_login_announcements;

use super::helpers::new_test_db;

#[tokio::test]
async fn login_announcements_missing_room_is_noop() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "announcements-none").await;

    client
        .execute(
            "DELETE FROM chat_rooms
             WHERE slug = 'announcements'
               AND kind = 'topic'
               AND visibility = 'public'",
            &[],
        )
        .await
        .expect("delete announcements room");

    let announcements = load_login_announcements(&client, user.id)
        .await
        .expect("load announcements");

    assert!(announcements.is_none());
}

#[tokio::test]
async fn login_announcements_return_unread_once_and_mark_read() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = create_test_user(&test_db.db, "announcements-viewer").await;
    let author = create_test_user(&test_db.db, "announcements-author").await;
    let room = ChatRoom::find_non_dm_by_slug(&client, "announcements")
        .await
        .expect("find announcements room")
        .expect("announcements room");

    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: viewer.id,
            body: "my own announcement draft".to_string(),
        },
    )
    .await
    .expect("self message");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "server update tonight".to_string(),
        },
    )
    .await
    .expect("announcement message");

    let announcements = load_login_announcements(&client, viewer.id)
        .await
        .expect("load announcements")
        .expect("unread announcements");
    assert_eq!(announcements.messages.len(), 1);
    assert_eq!(announcements.messages[0].author, author.username);
    assert_eq!(announcements.messages[0].body, "server update tonight");

    let last_read_at: Option<chrono::DateTime<chrono::Utc>> = client
        .query_one(
            "SELECT last_read_at
             FROM chat_room_members
             WHERE room_id = $1 AND user_id = $2",
            &[&room.id, &viewer.id],
        )
        .await
        .expect("membership")
        .get("last_read_at");
    assert!(last_read_at.is_some());

    let again = load_login_announcements(&client, viewer.id)
        .await
        .expect("reload announcements");
    assert!(again.is_none());
}
