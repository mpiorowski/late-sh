use std::time::Duration;

use late_core::models::cyberspace_account::CyberspaceAccount;
use late_core::test_utils::{create_test_user, test_db};

use crate::app::chat::cyberspace::svc::{CsEvent, CyberspaceService};

/// A base URL nothing listens on: any code path that unexpectedly touches
/// the network fails fast instead of calling the real cyberspace API.
fn dead_service(db: late_core::db::Db) -> CyberspaceService {
    CyberspaceService::new(db, "http://127.0.0.1:1".to_string())
}

async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<CsEvent>) -> CsEvent {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("event within timeout")
        .expect("channel open")
}

#[tokio::test]
async fn session_init_reports_unlinked_users_without_network() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "cs-fresh").await;
    let service = dead_service(test_db.db.clone());
    let mut rx = service.subscribe_events();

    service.session_init_task(user.id);

    match next_event(&mut rx).await {
        CsEvent::LinkStatus { user_id, username } => {
            assert_eq!(user_id, user.id);
            assert_eq!(username, None);
        }
        other => panic!("expected LinkStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn session_init_reports_the_linked_username() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-linked").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    let service = dead_service(test_db.db.clone());
    let mut rx = service.subscribe_events();

    service.session_init_task(user.id);

    match next_event(&mut rx).await {
        CsEvent::LinkStatus { user_id, username } => {
            assert_eq!(user_id, user.id);
            assert_eq!(username.as_deref(), Some("odd"));
        }
        other => panic!("expected LinkStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn unlink_forgets_the_stored_account() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-unlinker").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    let service = dead_service(test_db.db.clone());
    let mut rx = service.subscribe_events();

    service.unlink_task(user.id);

    match next_event(&mut rx).await {
        CsEvent::Unlinked { user_id } => assert_eq!(user_id, user.id),
        other => panic!("expected Unlinked, got {other:?}"),
    }
    assert!(
        CyberspaceAccount::find_by_user_id(&client, user.id)
            .await
            .expect("find")
            .is_none()
    );
}
