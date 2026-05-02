use late_core::{
    models::{
        work_feed_read::WorkFeedRead,
        work_profile::{WorkProfile, WorkProfileParams},
    },
    test_utils::create_test_user,
};
use late_ssh::app::chat::work::svc::{WorkEvent, WorkService};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::helpers::new_test_db;

fn params(user_id: Uuid, headline: &str, summary: &str, slug: &str) -> WorkProfileParams {
    WorkProfileParams {
        user_id,
        slug: slug.to_string(),
        headline: headline.to_string(),
        status: "open".to_string(),
        work_type: "full-time".to_string(),
        location: "remote".to_string(),
        links: vec!["https://github.com/late-sh".to_string()],
        skills: vec!["rust".to_string(), "postgres".to_string()],
        summary: summary.to_string(),
        include_bio: true,
        include_late_fetch: true,
        include_showcases: true,
    }
}

async fn recv_work_event(events: &mut tokio::sync::broadcast::Receiver<WorkEvent>) -> WorkEvent {
    timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("work event timeout")
        .expect("work event")
}

#[tokio::test]
async fn create_work_profile_publishes_event_and_snapshot_with_author() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "work-author").await;
    let service = WorkService::new(test_db.db.clone());
    let mut events = service.subscribe_events();
    let mut snapshot_rx = service.subscribe_snapshot();

    service.create_task(
        user.id,
        params(
            user.id,
            "Rust backend engineer",
            "Building terminal software and database-backed systems.",
            "w_abcdefghijkl",
        ),
    );

    match recv_work_event(&mut events).await {
        WorkEvent::Created { user_id } => assert_eq!(user_id, user.id),
        other => panic!("expected Created event, got {other:?}"),
    }

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].profile.headline, "Rust backend engineer");
    assert_eq!(snapshot.items[0].profile.slug, "w_abcdefghijkl");
    assert_eq!(snapshot.items[0].author_username, "work-author");
}

#[tokio::test]
async fn saving_existing_profile_updates_and_preserves_public_slug() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "work-upsert").await;
    let service = WorkService::new(test_db.db.clone());
    let mut events = service.subscribe_events();

    service.create_task(
        user.id,
        params(
            user.id,
            "Original headline",
            "Original summary.",
            "w_original1234",
        ),
    );
    match recv_work_event(&mut events).await {
        WorkEvent::Created { user_id } => assert_eq!(user_id, user.id),
        other => panic!("expected Created event, got {other:?}"),
    }

    service.create_task(
        user.id,
        params(
            user.id,
            "Updated headline",
            "Updated summary.",
            "w_ignoredslug1",
        ),
    );
    match recv_work_event(&mut events).await {
        WorkEvent::Updated { user_id } => assert_eq!(user_id, user.id),
        other => panic!("expected Updated event, got {other:?}"),
    }

    let profile = WorkProfile::find_by_user_id(&client, user.id)
        .await
        .expect("load profile")
        .expect("profile exists");
    assert_eq!(profile.headline, "Updated headline");
    assert_eq!(profile.summary, "Updated summary.");
    assert_eq!(profile.slug, "w_original1234");
}

#[tokio::test]
async fn non_owner_update_fails_and_leaves_work_profile_unchanged() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "work-owner").await;
    let intruder = create_test_user(&test_db.db, "work-intruder").await;
    let original = WorkProfile::create_by_user_id(
        &client,
        owner.id,
        params(
            owner.id,
            "Owned profile",
            "Original summary.",
            "w_owned0000000",
        ),
    )
    .await
    .expect("seed work profile");

    let service = WorkService::new(test_db.db.clone());
    let mut events = service.subscribe_events();

    service.update_task(
        intruder.id,
        original.id,
        params(
            intruder.id,
            "Hijacked profile",
            "Should not persist.",
            "w_hijack000000",
        ),
        false,
    );

    match recv_work_event(&mut events).await {
        WorkEvent::Failed { user_id, error } => {
            assert_eq!(user_id, intruder.id);
            assert!(
                error.contains("not your work profile"),
                "unexpected error: {error}"
            );
        }
        other => panic!("expected Failed event, got {other:?}"),
    }

    let reloaded = WorkProfile::get(&client, original.id)
        .await
        .expect("reload work profile")
        .expect("work profile still exists");
    assert_eq!(reloaded.user_id, owner.id);
    assert_eq!(reloaded.headline, "Owned profile");
    assert_eq!(reloaded.slug, "w_owned0000000");
}

#[tokio::test]
async fn admin_delete_removes_other_users_work_profile_and_refreshes_snapshot() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "work-delete-owner").await;
    let admin = create_test_user(&test_db.db, "work-delete-admin").await;
    let profile = WorkProfile::create_by_user_id(
        &client,
        owner.id,
        params(
            owner.id,
            "Delete me",
            "Admin should remove this.",
            "w_delete000000",
        ),
    )
    .await
    .expect("seed work profile");

    let service = WorkService::new(test_db.db.clone());
    let mut events = service.subscribe_events();
    let mut snapshot_rx = service.subscribe_snapshot();

    service.delete_task(admin.id, profile.id, true);

    match recv_work_event(&mut events).await {
        WorkEvent::Deleted { user_id } => assert_eq!(user_id, admin.id),
        other => panic!("expected Deleted event, got {other:?}"),
    }

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();
    assert!(snapshot.items.is_empty());

    let deleted = WorkProfile::get(&client, profile.id)
        .await
        .expect("reload deleted work profile");
    assert!(deleted.is_none());
}

#[tokio::test]
async fn unread_count_uses_work_read_cursor() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let author = create_test_user(&test_db.db, "work-unread-author").await;
    let later_author = create_test_user(&test_db.db, "work-unread-later").await;
    let reader = create_test_user(&test_db.db, "work-unread-reader").await;

    WorkProfile::create_by_user_id(
        &client,
        author.id,
        params(
            author.id,
            "Unread profile",
            "Visible before opening Work.",
            "w_unread000000",
        ),
    )
    .await
    .expect("seed unread work profile");

    let unread_before = WorkFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread before");
    assert_eq!(unread_before, 1);

    WorkFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("mark read");
    let read_cursor = WorkFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("read cursor");
    assert!(read_cursor.is_some());

    let unread_after = WorkFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after mark read");
    assert_eq!(unread_after, 0);

    tokio::time::sleep(Duration::from_millis(5)).await;
    WorkProfile::create_by_user_id(
        &client,
        later_author.id,
        params(
            later_author.id,
            "Later profile",
            "Created after opening Work.",
            "w_later0000000",
        ),
    )
    .await
    .expect("seed later work profile");

    let unread_after_new = WorkFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after new profile");
    assert_eq!(unread_after_new, 1);
}
