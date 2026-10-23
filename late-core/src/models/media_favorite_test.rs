use crate::{
    models::{
        media_favorite::{BOOTH_FAVORITES_LIMIT, FavoriteOutcome, MediaFavorite},
        media_history_item::MediaHistoryItem,
        media_queue_item::MediaQueueItem,
    },
    test_utils::{create_test_user, test_db},
};
use chrono::Utc;
use tokio_postgres::Client;
use uuid::Uuid;

/// Play `video_id` the way production records it: queued, promoted, written
/// to History, retired. Returns its History row.
async fn play(client: &mut Client, submitter_id: Uuid, video_id: &str) -> MediaHistoryItem {
    let (item, _reward) = MediaQueueItem::insert_youtube(
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
    MediaHistoryItem::record_play_from_queue_item(client, &item, 200)
        .await
        .expect("record play");
    MediaQueueItem::mark_played(client, item.id, Utc::now())
        .await
        .expect("mark played");
    MediaHistoryItem::list_recent(client, 200)
        .await
        .expect("history")
        .into_iter()
        .find(|row| row.external_id == video_id)
        .expect("history row")
}

async fn favorites(client: &Client, user_id: Uuid) -> Vec<String> {
    MediaFavorite::list_for_user(client, user_id)
        .await
        .expect("favorites")
        .into_iter()
        .map(|row| row.external_id)
        .collect()
}

/// A favorite is the listener's alone, kept under the id the caller named,
/// adding twice is one favorite, the list is newest first, and a remove by
/// somebody else, or of one already gone, deletes nothing.
#[tokio::test]
async fn favorites_are_per_listener_newest_first_and_removes_are_owner_scoped() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let listener = create_test_user(&test_db.db, "favorite-listener").await;
    let other = create_test_user(&test_db.db, "favorite-other").await;
    let first = play(&mut client, listener.id, "fav-first").await;
    let second = play(&mut client, listener.id, "fav-second").await;
    let second_id = Uuid::now_v7();

    for (track, id, expected) in [
        (&first, Uuid::now_v7(), FavoriteOutcome::Added),
        (&first, Uuid::now_v7(), FavoriteOutcome::AlreadyFavorite),
        (&second, second_id, FavoriteOutcome::Added),
    ] {
        let outcome = MediaFavorite::add_from_history(&mut client, listener.id, id, track)
            .await
            .expect("add favorite");
        assert_eq!(outcome, expected, "{}", track.external_id);
    }
    assert_eq!(
        favorites(&client, listener.id).await,
        vec!["fav-second", "fav-first"]
    );
    assert!(favorites(&client, other.id).await.is_empty());
    let kept = MediaFavorite::list_for_user(&client, listener.id)
        .await
        .expect("favorites");
    assert_eq!(kept[0].id, second_id);

    for (user_id, expected) in [(other.id, 0), (listener.id, 1), (listener.id, 0)] {
        let deleted = MediaFavorite::delete_by_user_id(&client, user_id, second_id)
            .await
            .expect("remove");
        assert_eq!(deleted, expected, "{user_id}");
    }
    assert_eq!(favorites(&client, listener.id).await, vec!["fav-first"]);
}

/// Past the cap an add writes nothing, and a favorite outlives its History
/// row, pruned or deleted.
#[tokio::test]
async fn favorites_cap_at_the_limit_and_outlive_their_history_row() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let listener = create_test_user(&test_db.db, "favorite-cap").await;
    for index in 0..BOOTH_FAVORITES_LIMIT {
        let track = play(&mut client, listener.id, &format!("fav-cap-{index}")).await;
        let outcome =
            MediaFavorite::add_from_history(&mut client, listener.id, Uuid::now_v7(), &track)
                .await
                .expect("add favorite");
        assert_eq!(outcome, FavoriteOutcome::Added);
    }
    let extra = play(&mut client, listener.id, "fav-cap-extra").await;

    let outcome = MediaFavorite::add_from_history(&mut client, listener.id, Uuid::now_v7(), &extra)
        .await
        .expect("add past the cap");

    assert_eq!(outcome, FavoriteOutcome::Full);
    let held = favorites(&client, listener.id).await;
    assert_eq!(held.len() as i64, BOOTH_FAVORITES_LIMIT);
    assert!(!held.contains(&"fav-cap-extra".to_string()));

    MediaHistoryItem::prune_to_limit(&client, 1)
        .await
        .expect("prune");
    let history = MediaHistoryItem::list_recent(&client, 200)
        .await
        .expect("history");
    MediaHistoryItem::delete_by_id(&client, history[0].id)
        .await
        .expect("delete history row");
    assert!(
        MediaHistoryItem::list_recent(&client, 200)
            .await
            .expect("history")
            .is_empty()
    );
    assert_eq!(
        favorites(&client, listener.id).await.len() as i64,
        BOOTH_FAVORITES_LIMIT
    );
}
