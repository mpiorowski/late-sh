//! Service integration tests for vote flows against a real ephemeral DB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;
use late_ssh::app::vote::svc::{Genre, VoteEvent, VoteService};
use late_ssh::state::ActivityEvent;
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

fn test_vote_service(db: late_core::db::Db) -> VoteService {
    VoteService::new(
        db,
        "127.0.0.1:0".to_string(),
        Duration::from_secs(30 * 60),
        Arc::new(Mutex::new(HashMap::new())),
        broadcast::channel::<ActivityEvent>(64).0,
    )
}

#[tokio::test]
async fn persists_user_vote_and_emits_success_when_cast_vote_called() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "vote-user").await;
    let user_id = user.id;
    let service = test_vote_service(test_db.db.clone());
    let mut events = service.subscribe_events();

    let snapshot = service
        .cast_vote(user_id, Genre::Jazz)
        .await
        .expect("cast vote");

    assert_eq!(snapshot.counts.jazz, 1);
    assert_eq!(
        service.get_user_vote(user_id).await.expect("get vote"),
        Some(Genre::Jazz)
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        VoteEvent::Success {
            user_id: got_user,
            genre,
        } => {
            assert_eq!(got_user, user_id);
            assert_eq!(genre, Genre::Jazz);
        }
        _ => panic!("expected success event"),
    }
}

#[tokio::test]
async fn emits_success_and_updates_snapshot_when_cast_vote_task_called() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "vote-task-user").await;
    let user_id = user.id;
    let service = test_vote_service(test_db.db.clone());
    let mut events = service.subscribe_events();
    let mut state_rx = service.subscribe_state();

    service.cast_vote_task(user_id, Genre::Classic);

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        VoteEvent::Success {
            user_id: got_user,
            genre,
        } => {
            assert_eq!(got_user, user_id);
            assert_eq!(genre, Genre::Classic);
        }
        _ => panic!("expected success event"),
    }

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();
    assert_eq!(snapshot.counts.classic, 1);
    assert_eq!(
        service.get_user_vote(user_id).await.expect("get vote"),
        Some(Genre::Classic)
    );
}

#[tokio::test]
async fn emits_error_event_when_cast_vote_called_for_unknown_user() {
    let test_db = new_test_db().await;
    let unknown_user = Uuid::now_v7();
    let service = test_vote_service(test_db.db.clone());
    let mut events = service.subscribe_events();

    let result = service.cast_vote(unknown_user, Genre::Lofi).await;
    assert!(result.is_err(), "expected db FK error for unknown user");

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        VoteEvent::Error { user_id, message } => {
            assert_eq!(user_id, unknown_user);
            assert_eq!(message, "Vote failed. Please try again.");
        }
        _ => panic!("expected error event"),
    }
}

#[tokio::test]
async fn does_not_emit_activity_when_revoting_same_genre() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "revote-user").await;
    let user_id = user.id;

    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(64);
    let mut active_users = HashMap::new();
    active_users.insert(
        user_id,
        late_ssh::state::ActiveUser {
            username: user.username.clone(),
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    );

    let service = VoteService::new(
        test_db.db.clone(),
        "127.0.0.1:0".to_string(),
        Duration::from_secs(30 * 60),
        Arc::new(Mutex::new(active_users)),
        activity_tx,
    );

    // First vote - should emit activity
    service
        .cast_vote(user_id, Genre::Ambient)
        .await
        .expect("first vote");

    let activity = timeout(Duration::from_millis(100), activity_rx.recv())
        .await
        .expect("activity timeout")
        .expect("activity event");
    assert_eq!(activity.username, user.username);
    assert!(activity.action.contains("ambient"));

    // Revote same genre - should NOT emit activity
    service
        .cast_vote(user_id, Genre::Ambient)
        .await
        .expect("revote same");

    // Try to receive activity - should timeout since none was sent
    let no_activity = timeout(Duration::from_millis(100), activity_rx.recv()).await;
    assert!(
        no_activity.is_err(),
        "expected no activity event for revoting same genre"
    );

    // Vote different genre - should emit activity again
    service
        .cast_vote(user_id, Genre::Jazz)
        .await
        .expect("vote different");

    let activity = timeout(Duration::from_millis(100), activity_rx.recv())
        .await
        .expect("activity timeout")
        .expect("activity event");
    assert_eq!(activity.username, user.username);
    assert!(activity.action.contains("jazz"));
}
