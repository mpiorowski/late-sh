use late_core::{
    models::{
        showcase::{Showcase, ShowcaseParams},
        showcase_feed_read::ShowcaseFeedRead,
    },
    test_utils::create_test_user,
};
use late_ssh::app::chat::showcase::svc::{ShowcaseEvent, ShowcaseService};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::helpers::new_test_db;

fn params(user_id: Uuid, title: &str, url: &str, description: &str) -> ShowcaseParams {
    ShowcaseParams {
        user_id,
        title: title.to_string(),
        url: url.to_string(),
        description: description.to_string(),
        tags: vec!["rust".to_string(), "ssh".to_string()],
    }
}

async fn recv_showcase_event(
    events: &mut tokio::sync::broadcast::Receiver<ShowcaseEvent>,
) -> ShowcaseEvent {
    timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("showcase event timeout")
        .expect("showcase event")
}

#[tokio::test]
async fn create_showcase_publishes_event_and_snapshot_with_author() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "showcase-author").await;
    let service = ShowcaseService::new(test_db.db.clone());
    let mut events = service.subscribe_events();
    let mut snapshot_rx = service.subscribe_snapshot();

    service.create_task(
        user.id,
        params(
            user.id,
            "Late SSH",
            "https://late.sh",
            "A terminal clubhouse for developers.",
        ),
    );

    match recv_showcase_event(&mut events).await {
        ShowcaseEvent::Created { user_id } => assert_eq!(user_id, user.id),
        other => panic!("expected Created event, got {other:?}"),
    }

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].showcase.title, "Late SSH");
    assert_eq!(snapshot.items[0].showcase.url, "https://late.sh");
    assert_eq!(snapshot.items[0].author_username, "showcase-author");
}

#[tokio::test]
async fn non_owner_update_fails_and_leaves_showcase_unchanged() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "showcase-owner").await;
    let intruder = create_test_user(&test_db.db, "showcase-intruder").await;
    let original = Showcase::create_by_user_id(
        &client,
        owner.id,
        params(
            owner.id,
            "Owned Project",
            "https://example.com/original",
            "Original description.",
        ),
    )
    .await
    .expect("seed showcase");

    let service = ShowcaseService::new(test_db.db.clone());
    let mut events = service.subscribe_events();

    service.update_task(
        intruder.id,
        original.id,
        params(
            intruder.id,
            "Hijacked Project",
            "https://example.com/hijacked",
            "Should not persist.",
        ),
        false,
    );

    match recv_showcase_event(&mut events).await {
        ShowcaseEvent::Failed { user_id, error } => {
            assert_eq!(user_id, intruder.id);
            assert!(
                error.contains("not your showcase"),
                "unexpected error: {error}"
            );
        }
        other => panic!("expected Failed event, got {other:?}"),
    }

    let reloaded = Showcase::get(&client, original.id)
        .await
        .expect("reload showcase")
        .expect("showcase still exists");
    assert_eq!(reloaded.user_id, owner.id);
    assert_eq!(reloaded.title, "Owned Project");
    assert_eq!(reloaded.url, "https://example.com/original");
}

#[tokio::test]
async fn admin_delete_removes_other_users_showcase_and_refreshes_snapshot() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "showcase-delete-owner").await;
    let admin = create_test_user(&test_db.db, "showcase-delete-admin").await;
    let showcase = Showcase::create_by_user_id(
        &client,
        owner.id,
        params(
            owner.id,
            "Delete Me",
            "https://example.com/delete-me",
            "Admin should be able to remove this.",
        ),
    )
    .await
    .expect("seed showcase");

    let service = ShowcaseService::new(test_db.db.clone());
    let mut events = service.subscribe_events();
    let mut snapshot_rx = service.subscribe_snapshot();

    service.delete_task(admin.id, showcase.id, true);

    match recv_showcase_event(&mut events).await {
        ShowcaseEvent::Deleted { user_id } => assert_eq!(user_id, admin.id),
        other => panic!("expected Deleted event, got {other:?}"),
    }

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();
    assert!(snapshot.items.is_empty());

    let deleted = Showcase::get(&client, showcase.id)
        .await
        .expect("reload deleted showcase");
    assert!(deleted.is_none());
}

#[tokio::test]
async fn unread_count_uses_showcase_read_cursor() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let author = create_test_user(&test_db.db, "showcase-unread-author").await;
    let reader = create_test_user(&test_db.db, "showcase-unread-reader").await;

    Showcase::create_by_user_id(
        &client,
        author.id,
        params(
            author.id,
            "Unread Project",
            "https://example.com/unread",
            "Visible before the reader opens showcase.",
        ),
    )
    .await
    .expect("seed unread showcase");

    let unread_before = ShowcaseFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread before");
    assert_eq!(unread_before, 1);

    ShowcaseFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("mark read");
    let read_cursor = ShowcaseFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("read cursor");
    assert!(read_cursor.is_some());

    let unread_after = ShowcaseFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after mark read");
    assert_eq!(unread_after, 0);

    Showcase::create_by_user_id(
        &client,
        author.id,
        params(
            author.id,
            "Later Project",
            "https://example.com/later",
            "Created after the reader opened showcase.",
        ),
    )
    .await
    .expect("seed later showcase");

    let unread_after_new = ShowcaseFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after new showcase");
    assert_eq!(unread_after_new, 1);
}
