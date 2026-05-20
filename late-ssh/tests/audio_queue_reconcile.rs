use late_core::{
    models::media_queue_item::MediaQueueItem,
    test_utils::{create_test_user, test_db},
};
use late_ssh::{
    app::audio::{svc::AudioService, youtube::YoutubeVideo},
    paired_clients::PairedClientRegistry,
};

#[tokio::test]
async fn submit_adopts_existing_db_current_instead_of_hitting_singleton() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "audio_submit_reconcile").await;

    let existing_id = {
        let client = test.db.get().await.expect("db client");
        let existing = MediaQueueItem::insert_youtube(
            &client,
            user.id,
            "aaaaaaaaaaa",
            Some("already playing"),
            None,
            Some(60_000),
            false,
        )
        .await
        .expect("insert existing");
        MediaQueueItem::mark_playing(&client, existing.id, chrono::Utc::now())
            .await
            .expect("mark playing")
            .expect("playing row")
            .id
    };

    // New service instance starts with empty in-memory state while DB already
    // has a playing row. This is the prod stuck shape after a stale/draining
    // pod lost current_item_id.
    let service = AudioService::new(
        test.db.clone(),
        None,
        PairedClientRegistry::new(),
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    );
    let response = service
        .submit_validated_video(user.id, test_video("bbbbbbbbbbb", "queued"))
        .await
        .expect("submit should reconcile, not singleton-fail");

    assert_eq!(response.position_in_queue, 1);
    let client = test.db.get().await.expect("db client");
    let current = MediaQueueItem::current_playing(&client)
        .await
        .expect("current")
        .expect("still playing");
    assert_eq!(current.id, existing_id);

    let snapshot = MediaQueueItem::list_snapshot(&client, 10)
        .await
        .expect("snapshot");
    assert!(snapshot.iter().any(|(item, _)| {
        item.external_id == "bbbbbbbbbbb" && item.status == MediaQueueItem::STATUS_QUEUED
    }));
}

#[tokio::test]
async fn force_skip_stale_memory_does_not_mutate_already_played_row() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "audio_skip_reconcile").await;
    let service = AudioService::new(
        test.db.clone(),
        None,
        PairedClientRegistry::new(),
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    );

    let first = service
        .submit_validated_video(user.id, test_video("ccccccccccc", "first"))
        .await
        .expect("queue first");

    let second_id = {
        let client = test.db.get().await.expect("db client");
        MediaQueueItem::mark_played(&client, first.id, chrono::Utc::now())
            .await
            .expect("mark first played");
        let second = MediaQueueItem::insert_youtube(
            &client,
            user.id,
            "ddddddddddd",
            Some("db current"),
            None,
            Some(60_000),
            false,
        )
        .await
        .expect("insert second");
        MediaQueueItem::mark_playing(&client, second.id, chrono::Utc::now())
            .await
            .expect("mark second playing")
            .expect("second playing")
            .id
    };

    // Service memory still points at the first id, but DB says first is played
    // and second is playing. The old update_status foot-gun would flip the
    // first row from played -> skipped here.
    let err = service
        .force_skip()
        .await
        .expect_err("stale skip should ask for retry after reconcile");
    assert!(format!("{err:#}").contains("track changed"));

    let client = test.db.get().await.expect("db client");
    let first_row = MediaQueueItem::find_by_id(&client, first.id)
        .await
        .expect("find first")
        .expect("first exists");
    assert_eq!(first_row.status, MediaQueueItem::STATUS_PLAYED);

    let current = MediaQueueItem::current_playing(&client)
        .await
        .expect("current")
        .expect("still playing");
    assert_eq!(current.id, second_id);
}

fn test_video(video_id: &str, title: &str) -> YoutubeVideo {
    YoutubeVideo {
        video_id: video_id.to_string(),
        title: Some(title.to_string()),
        channel: None,
        duration_ms: Some(60_000),
        is_stream: false,
    }
}
