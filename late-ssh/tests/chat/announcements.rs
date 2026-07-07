use chrono::{Duration, Utc};
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
async fn login_announcements_return_unread_without_marking_read() {
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
    assert_eq!(announcements.room_id, room.id);
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
    assert!(last_read_at.is_none());

    let again = load_login_announcements(&client, viewer.id)
        .await
        .expect("reload announcements")
        .expect("still unread announcements");
    assert_eq!(again.messages.len(), 1);

    client
        .execute(
            "UPDATE chat_room_members
             SET last_read_at = $3
             WHERE room_id = $1 AND user_id = $2",
            &[
                &announcements.room_id,
                &viewer.id,
                &announcements.latest_displayed_at().expect("display cursor"),
            ],
        )
        .await
        .expect("mark displayed announcements read");

    let after_mark = load_login_announcements(&client, viewer.id)
        .await
        .expect("reload announcements after mark");
    assert!(after_mark.is_none());
}

#[tokio::test]
async fn login_announcements_shows_newest_first_capped_at_ten() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = create_test_user(&test_db.db, "announcements-batch-viewer").await;
    let author = create_test_user(&test_db.db, "announcements-batch-author").await;
    let room = ChatRoom::find_non_dm_by_slug(&client, "announcements")
        .await
        .expect("find announcements room")
        .expect("announcements room");
    let base = Utc::now() - Duration::hours(1);

    for index in 0..12 {
        let message = ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: author.id,
                body: format!("announcement {index}"),
            },
        )
        .await
        .expect("announcement message");
        client
            .execute(
                "UPDATE chat_messages SET created = $2 WHERE id = $1",
                &[&message.id, &(base + Duration::seconds(index as i64))],
            )
            .await
            .expect("set announcement order");
    }

    // The splash shows the newest 10 unread, newest at the top: announcement
    // 11 down to 2. The LIMIT keeps the most recent, not the oldest.
    let first_batch = load_login_announcements(&client, viewer.id)
        .await
        .expect("load first batch")
        .expect("first batch");
    assert_eq!(first_batch.messages.len(), 10);
    assert_eq!(first_batch.messages[0].body, "announcement 11");
    assert_eq!(first_batch.messages[9].body, "announcement 2");

    // Marking read up to the newest displayed catches the viewer up; the two
    // announcements older than the 10-message window sit behind the cursor and
    // are treated as seen (an old-announcement backlog is not paged through).
    client
        .execute(
            "UPDATE chat_room_members
             SET last_read_at = $3
             WHERE room_id = $1 AND user_id = $2",
            &[
                &first_batch.room_id,
                &viewer.id,
                &first_batch.latest_displayed_at().expect("display cursor"),
            ],
        )
        .await
        .expect("mark first batch read");

    let after_mark = load_login_announcements(&client, viewer.id)
        .await
        .expect("reload announcements after mark");
    assert!(after_mark.is_none());
}
