use crate::app::activity::event::ActivityEvent;
use crate::app::bonsai::svc::BonsaiService;
use late_core::models::bonsai::{Grave, Tree};
use late_core::models::chips::{ChipMove, UserChips};
use late_core::models::marketplace::{
    BONSAI_DECAY_PROTECTION_KIND, BONSAI_DECAY_SHIELD_SKU, purchase_durable_item_by_sku,
};
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};

use crate::test_helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn ensure_tree_creates_default_tree_for_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-svc-new").await;
    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");

    assert_eq!(tree.user_id, user.id);
    assert_eq!(tree.seed, user.id.as_u128() as i64);
    assert_eq!(tree.growth_points, 0);
    assert_eq!(tree.last_watered, None);
    assert!(tree.is_alive);
}

#[tokio::test]
async fn ensure_tree_kills_stale_tree_records_grave_and_emits_activity() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-withered").await;
    Tree::ensure(&client, user.id, 77).await.expect("ensure");
    Tree::set_recorded_dates(
        &client,
        user.id,
        chrono::Utc::now() - chrono::Duration::days(8),
        Some(chrono::Utc::now().date_naive() - chrono::Duration::days(8)),
    )
    .await
    .expect("age tree");

    let (tx, mut rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    assert!(!tree.is_alive);

    let persisted = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find tree")
        .expect("tree");
    assert!(!persisted.is_alive);

    let graves = Grave::list_by_user(&client, user.id)
        .await
        .expect("list graves");
    assert_eq!(graves.len(), 1);
    assert!(graves[0].survived_days >= 8);

    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("event timeout")
        .expect("event");
    assert_eq!(event.username, "bonsai-withered");
    assert!(
        event.action.starts_with("lost their bonsai"),
        "unexpected action: {}",
        event.action
    );
}

#[tokio::test]
async fn ensure_tree_survives_a_stale_gap_covered_by_a_bonsai_decay_shield() {
    let test_db = new_test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-shielded").await;
    Tree::ensure(&client, user.id, 77).await.expect("ensure");
    Tree::set_recorded_dates(
        &client,
        user.id,
        chrono::Utc::now() - chrono::Duration::days(8),
        Some(chrono::Utc::now().date_naive() - chrono::Duration::days(8)),
    )
    .await
    .expect("age tree");

    UserChips::apply(&**client, user.id, ChipMove::Credit, 2_000, None)
        .await
        .expect("fund chips");
    purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("buy shield")
        .expect("item available");
    // Backdate the shield as if it had been active the whole 8-day gap
    // (a fresh purchase only protects days from now on, so this simulates
    // "bought before the tree went dry" rather than "rescued after the
    // fact". See the companion test below for the partial-coverage case.
    client
        .execute(
            "UPDATE shop_consumable_effects
             SET starts_at = current_timestamp - interval '9 days',
                 ends_at = current_timestamp + interval '5 days'
             WHERE user_id = $1 AND effect_kind = $2",
            &[&user.id, &BONSAI_DECAY_PROTECTION_KIND],
        )
        .await
        .expect("backdate shield");

    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    assert!(tree.is_alive);

    let graves = Grave::list_by_user(&client, user.id)
        .await
        .expect("list graves");
    assert!(graves.is_empty());
}

#[tokio::test]
async fn ensure_tree_survives_a_gap_spanning_two_stacked_shield_purchases() {
    let test_db = new_test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-restacked-shield").await;
    Tree::ensure(&client, user.id, 77).await.expect("ensure");
    Tree::set_recorded_dates(
        &client,
        user.id,
        chrono::Utc::now() - chrono::Duration::days(15),
        Some(chrono::Utc::now().date_naive() - chrono::Duration::days(15)),
    )
    .await
    .expect("age tree");

    UserChips::apply(&**client, user.id, ChipMove::Credit, 4_000, None)
        .await
        .expect("fund chips");

    // First purchase, backdated to look like it was bought before the dry
    // spell began and is still live.
    purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("first buy")
        .expect("item available");
    client
        .execute(
            "UPDATE shop_consumable_effects
             SET starts_at = current_timestamp - interval '9 days',
                 ends_at = current_timestamp + interval '5 days'
             WHERE user_id = $1 AND effect_kind = $2",
            &[&user.id, &BONSAI_DECAY_PROTECTION_KIND],
        )
        .await
        .expect("backdate first purchase");

    // Rebuy while the first window is still live. If the rebuy reset
    // starts_at to now instead of carrying the earlier purchase's starts_at
    // forward, the protection credit for the first 9 days would be lost
    // and this tree would die below.
    purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("second buy")
        .expect("item available");

    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    assert!(tree.is_alive);

    let graves = Grave::list_by_user(&client, user.id)
        .await
        .expect("list graves");
    assert!(graves.is_empty());
}

#[tokio::test]
async fn ensure_tree_still_dies_when_the_shield_only_covers_part_of_the_gap() {
    let test_db = new_test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-partially-shielded").await;
    Tree::ensure(&client, user.id, 77).await.expect("ensure");
    Tree::set_recorded_dates(
        &client,
        user.id,
        chrono::Utc::now() - chrono::Duration::days(8),
        Some(chrono::Utc::now().date_naive() - chrono::Duration::days(8)),
    )
    .await
    .expect("age tree");

    UserChips::apply(&**client, user.id, ChipMove::Credit, 2_000, None)
        .await
        .expect("fund chips");
    // A fresh purchase only protects days from now on: it covers just
    // today out of the 8-day-old gap, leaving 7 unprotected dry days,
    // still enough to kill the tree.
    purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("buy shield")
        .expect("item available");

    let (tx, mut rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    assert!(!tree.is_alive);

    let graves = Grave::list_by_user(&client, user.id)
        .await
        .expect("list graves");
    assert_eq!(graves.len(), 1);
    timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("event timeout")
        .expect("event");
}
