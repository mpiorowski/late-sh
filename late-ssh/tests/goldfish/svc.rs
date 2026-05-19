use late_ssh::app::goldfish::svc::GoldfishService;

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn ensure_bowl_creates_default_bowl_for_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "fish-svc-new").await;
    let svc = GoldfishService::new(test_db.db.clone());

    let bowl = svc.ensure_bowl(user.id).await.expect("ensure bowl");

    assert_eq!(bowl.user_id, user.id);
    assert_eq!(bowl.friend_count, 0);
    assert_eq!(bowl.last_fed, None);
}

#[tokio::test]
async fn ensure_bowl_is_idempotent_across_reconnects() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "fish-svc-reconnect").await;
    let svc = GoldfishService::new(test_db.db.clone());

    let first = svc.ensure_bowl(user.id).await.expect("first ensure");
    let second = svc.ensure_bowl(user.id).await.expect("second ensure");

    assert_eq!(
        first.id, second.id,
        "reconnecting must return the same bowl row, not create a new one"
    );
}
