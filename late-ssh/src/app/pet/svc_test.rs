use late_core::models::chips::UserChips;
use late_core::models::pet::PetCompanion;
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};

use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::app::pet::svc::{FEED_CHIP_BONUS, PetService};
use crate::test_helpers::new_test_db;

fn service(db: &late_core::db::Db) -> (PetService, broadcast::Receiver<ActivityEvent>) {
    let (tx, rx) = broadcast::channel::<ActivityEvent>(16);
    (PetService::new(db.clone(), tx), rx)
}

#[tokio::test]
async fn ensure_cat_creates_default_companion_for_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cat-svc-new").await;
    let (svc, _rx) = service(&test_db.db);

    let cat = svc.ensure_cat(user.id).await.expect("ensure cat");

    assert_eq!(cat.user_id, user.id);
    assert_eq!(cat.last_fed, None);
}

#[tokio::test]
async fn ensure_cat_is_idempotent_across_reconnects() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cat-svc-reconnect").await;
    let (svc, _rx) = service(&test_db.db);

    let first = svc.ensure_cat(user.id).await.expect("first ensure");
    let second = svc.ensure_cat(user.id).await.expect("second ensure");

    assert_eq!(
        first.id, second.id,
        "reconnecting must return the same cat row, not create a new one"
    );
}

#[tokio::test]
async fn feeding_pays_the_daily_chips_once() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cat-svc-feed").await;
    let (svc, mut rx) = service(&test_db.db);
    svc.ensure_cat(user.id).await.expect("ensure cat");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    svc.feed(user.id).await.expect("first feed");
    svc.feed(user.id).await.expect("second feed");

    let after = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(after - before, FEED_CHIP_BONUS);

    let stored = PetCompanion::ensure(&client, user.id)
        .await
        .expect("companion");
    assert_eq!(
        stored.last_fed.map(|time| time.date_naive()),
        Some(chrono::Utc::now().date_naive())
    );

    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert_eq!(event.user_id, Some(user.id));
    assert!(matches!(event.kind, ActivityKind::PetFed));
    assert!(
        rx.try_recv().is_err(),
        "the same-day second feed announces nothing"
    );
}
