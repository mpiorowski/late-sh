use late_core::models::chips::UserChips;
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};

use super::{AquariumService, FEED_CHIP_BONUS};
use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::test_helpers::new_test_db;

#[tokio::test]
async fn feeding_pays_the_daily_chips_once() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "aquarium-svc-feed").await;
    let (tx, mut rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = AquariumService::new(test_db.db.clone(), tx);
    assert_eq!(
        svc.last_fed(user.id).await.expect("last fed"),
        None,
        "a tank nobody fed has no care row"
    );
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
    assert_eq!(
        svc.last_fed(user.id)
            .await
            .expect("last fed")
            .map(|time| time.date_naive()),
        Some(chrono::Utc::now().date_naive())
    );

    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert_eq!(event.user_id, Some(user.id));
    assert!(matches!(event.kind, ActivityKind::AquariumFed));
    assert!(
        rx.try_recv().is_err(),
        "the same-day second feed announces nothing"
    );
}
