use late_ssh::app::cat::svc::CatService;

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn ensure_cat_creates_default_companion_for_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cat-svc-new").await;
    let svc = CatService::new(test_db.db.clone());

    let cat = svc.ensure_cat(user.id).await.expect("ensure cat");

    assert_eq!(cat.user_id, user.id);
    assert_eq!(cat.last_fed, None);
    assert_eq!(cat.last_watered, None);
    assert_eq!(cat.last_played, None);
    assert_eq!(cat.last_groomed, None);
    assert_eq!(cat.last_treated, None);
}

#[tokio::test]
async fn ensure_cat_is_idempotent_across_reconnects() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cat-svc-reconnect").await;
    let svc = CatService::new(test_db.db.clone());

    let first = svc.ensure_cat(user.id).await.expect("first ensure");
    let second = svc.ensure_cat(user.id).await.expect("second ensure");

    assert_eq!(
        first.id, second.id,
        "reconnecting must return the same cat row, not create a new one"
    );
}
