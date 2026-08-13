use std::time::Duration;

use late_core::{
    models::{
        stream_ban::StreamBan,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

use crate::app::activity::publisher::ActivityPublisher;
use crate::app::stream::registry::EndReason;
use crate::app::stream::svc::{StreamEvent, StreamService};
use crate::app::voice::svc::{VoiceConfig, VoiceService};

/// LiveKit is never called by `go_live` (tickets are minted later, at the
/// page's grant fetch), so an enabled config with fake credentials is enough
/// to exercise the whole command path.
fn test_stream_service(db: late_core::db::Db) -> StreamService {
    let voice = VoiceService::new(
        VoiceConfig::enabled(
            "wss://rtc.test".to_string(),
            "test-key".to_string(),
            "test-secret".to_string(),
            "late-voice".to_string(),
        )
        .expect("voice config"),
    );
    let (activity_tx, _activity_rx) = tokio::sync::broadcast::channel(16);
    StreamService::new(
        db.clone(),
        voice,
        ActivityPublisher::new(db, activity_tx),
        "https://late.test".to_string(),
    )
}

async fn go_live_outcome(service: &StreamService, user: &User) -> StreamEvent {
    let mut events = service.subscribe_events();
    service.go_live_task(user.id, user.username.clone(), Some("demo".to_string()));
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("go live event in time")
        .expect("go live event")
}

/// A stream ban is what makes the kill switch stick: the registry teardown
/// kills the current broadcast, and this refusal is what stops the streamer
/// from simply running `/golive` again a second later.
#[tokio::test]
async fn stream_banned_user_cannot_go_live_until_the_ban_is_lifted() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = User::create(
        &client,
        UserParams {
            fingerprint: "stream-ban-actor-fp".to_string(),
            username: "stream_ban_actor".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create actor");
    let target = User::create(
        &client,
        UserParams {
            fingerprint: "stream-ban-target-fp".to_string(),
            username: "stream_ban_target".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create target");
    let service = test_stream_service(test_db.db.clone());

    // Unbanned, the same call registers a stream.
    match go_live_outcome(&service, &target).await {
        StreamEvent::GoLiveReady { user_id, .. } => assert_eq!(user_id, target.id),
        other => panic!("expected GoLiveReady before the ban, got {other:?}"),
    }
    assert!(
        service.stop(target.id, EndReason::Command),
        "stream should have been live"
    );

    StreamBan::activate(&client, target.id, actor.id, "nsfw", None)
        .await
        .expect("activate stream ban");

    match go_live_outcome(&service, &target).await {
        StreamEvent::GoLiveFailed { user_id, message } => {
            assert_eq!(user_id, target.id);
            assert!(
                message.contains("blocked you from streaming"),
                "banned streamer should be told why: {message}"
            );
        }
        other => panic!("expected GoLiveFailed while banned, got {other:?}"),
    }
    assert!(
        service.watch_url_for_username(&target.username).is_none(),
        "a refused go-live must not leave a watchable stream behind"
    );

    StreamBan::delete_for_user(&client, target.id)
        .await
        .expect("lift stream ban");

    match go_live_outcome(&service, &target).await {
        StreamEvent::GoLiveReady { user_id, .. } => assert_eq!(user_id, target.id),
        other => panic!("expected GoLiveReady after the ban was lifted, got {other:?}"),
    }
}

/// An expired ban is dead on read: no sweeper clears the row, so the expiry
/// has to be part of the lookup itself.
#[tokio::test]
async fn expired_stream_ban_does_not_block_going_live() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = User::create(
        &client,
        UserParams {
            fingerprint: "stream-ban-expiry-actor-fp".to_string(),
            username: "stream_expiry_actor".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create actor");
    let target = User::create(
        &client,
        UserParams {
            fingerprint: "stream-ban-expiry-target-fp".to_string(),
            username: "stream_expiry_target".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create target");
    StreamBan::activate(
        &client,
        target.id,
        actor.id,
        "served",
        Some(chrono::Utc::now() - chrono::Duration::minutes(1)),
    )
    .await
    .expect("activate expired stream ban");

    let service = test_stream_service(test_db.db.clone());
    match go_live_outcome(&service, &target).await {
        StreamEvent::GoLiveReady { user_id, .. } => assert_eq!(user_id, target.id),
        other => panic!("expected GoLiveReady with an expired ban, got {other:?}"),
    }
}
