use crate::{
    models::{
        media_history_item::MediaHistoryItem,
        media_queue_item::MediaQueueItem,
        user::{User, UserParams},
    },
    test_utils::test_db,
};
use chrono::{DateTime, Duration, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

async fn submitter(client: &Client, name: &str) -> User {
    User::create(
        client,
        UserParams {
            fingerprint: name.to_string(),
            username: name.to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user")
}

/// Play `video_id` through the same path production uses: a queued row that
/// gets promoted, recorded in history, then retired when the track ends.
/// Retiring matters: only one row per track may be queued or playing at a
/// time, so a play that never ends would block the next play of that track.
async fn play(client: &Client, submitter_id: Uuid, video_id: &str, limit: i64) {
    let item = MediaQueueItem::insert_youtube(
        client,
        submitter_id,
        video_id,
        Some(video_id),
        Some("Channel"),
        Some(120_000),
        false,
    )
    .await
    .expect("queue item");
    let item = MediaQueueItem::mark_playing(client, item.id, Utc::now())
        .await
        .expect("mark playing")
        .expect("queued row promoted to playing");
    MediaHistoryItem::record_play_from_queue_item(client, &item, limit)
        .await
        .expect("record play");
    MediaQueueItem::mark_played(client, item.id, Utc::now())
        .await
        .expect("mark played");
}

/// History ordering is driven by wall-clock `last_played_at`. Pin it explicitly
/// so the assertions can't hinge on two inserts landing in the same microsecond.
async fn set_played_at(client: &Client, video_id: &str, at: DateTime<Utc>) {
    client
        .execute(
            "UPDATE media_history_items SET last_played_at = $2 WHERE external_id = $1",
            &[&video_id, &at],
        )
        .await
        .expect("backdate");
}

async fn video_ids(client: &Client, limit: i64) -> Vec<String> {
    MediaHistoryItem::list_recent(client, limit)
        .await
        .expect("list")
        .into_iter()
        .map(|item| item.external_id)
        .collect()
}

#[tokio::test]
async fn replaying_a_track_moves_it_to_the_front_without_duplicating_it() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = submitter(&client, "history-replay-user").await;
    let now = Utc::now();

    play(&client, user.id, "aaa", 200).await;
    set_played_at(&client, "aaa", now - Duration::minutes(10)).await;
    play(&client, user.id, "bbb", 200).await;
    set_played_at(&client, "bbb", now - Duration::minutes(5)).await;

    assert_eq!(video_ids(&client, 200).await, vec!["bbb", "aaa"]);

    play(&client, user.id, "aaa", 200).await;

    assert_eq!(video_ids(&client, 200).await, vec!["aaa", "bbb"]);
    let replayed = MediaHistoryItem::list_recent(&client, 200)
        .await
        .expect("list")
        .into_iter()
        .find(|item| item.external_id == "aaa")
        .expect("replayed row");
    assert_eq!(replayed.play_count, 2);
}

#[tokio::test]
async fn prune_keeps_exactly_the_most_recently_played_rows() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = submitter(&client, "history-prune-user").await;
    let now = Utc::now();

    for (index, video_id) in ["oldest", "older", "newer", "newest"].iter().enumerate() {
        play(&client, user.id, video_id, 200).await;
        set_played_at(
            &client,
            video_id,
            now - Duration::minutes(10 - index as i64),
        )
        .await;
    }

    let deleted = MediaHistoryItem::prune_to_limit(&client, 2)
        .await
        .expect("prune");

    assert_eq!(deleted, 2);
    assert_eq!(video_ids(&client, 200).await, vec!["newest", "newer"]);
}

#[tokio::test]
async fn recording_a_new_track_at_the_limit_evicts_the_oldest_but_a_replay_evicts_nothing() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = submitter(&client, "history-limit-user").await;
    let now = Utc::now();

    play(&client, user.id, "first", 2).await;
    set_played_at(&client, "first", now - Duration::minutes(10)).await;
    play(&client, user.id, "second", 2).await;
    set_played_at(&client, "second", now - Duration::minutes(5)).await;

    // A brand new track lands on top and pushes the oldest row off the bottom.
    play(&client, user.id, "third", 2).await;
    assert_eq!(video_ids(&client, 200).await, vec!["third", "second"]);

    // A replay of a row already in history evicts nothing: it updates in place.
    play(&client, user.id, "second", 2).await;
    assert_eq!(video_ids(&client, 200).await, vec!["second", "third"]);
}
