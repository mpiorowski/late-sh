use chrono::Utc;
use late_core::models::bonsai::{Tree, TreeParams};
use late_core::models::chips::UserChips;
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};

use super::BonsaiService;
use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::test_helpers::new_test_db;

fn params_for(tree: &Tree, state_revision: i64) -> TreeParams {
    TreeParams {
        user_id: tree.user_id,
        seed: tree.seed,
        last_watered: Some(Utc::now().date_naive()),
        is_alive: tree.is_alive,
        vigor: tree.vigor,
        water_stress: tree.water_stress,
        last_simulated_date: tree.last_simulated_date,
        branch_graph: tree.branch_graph.clone(),
        selected_branch_id: tree.selected_branch_id,
        mode: tree.mode.clone(),
        badge_glyph: tree.badge_glyph.clone(),
        planted_at: tree.planted_at,
        state_revision,
    }
}

#[tokio::test]
async fn ensure_tree_plants_a_fresh_seed_for_a_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-svc-new").await;
    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let (tree, protection) = svc.ensure_tree(user.id).await.expect("ensure tree");

    assert_eq!(tree.user_id, user.id);
    assert_eq!(tree.seed, user.id.as_u128() as i64);
    assert_eq!(tree.last_watered, None);
    assert!(tree.is_alive);
    assert_eq!(tree.badge_glyph, "·", "a seed has no presence yet");
    assert!(protection.is_none());
    assert!(
        tree.branch_graph.get("branches").is_some(),
        "the planted graph is the seeded root, not an empty document"
    );
}

#[tokio::test]
async fn watering_pays_the_daily_chips_once_and_saves_every_time() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-svc-water").await;
    let (tx, mut rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    let (tree, _) = svc.ensure_tree(user.id).await.expect("ensure tree");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    let mut first = params_for(&tree, 1);
    first.badge_glyph = "⚘".to_string();
    svc.water(first).await.expect("first watering");
    let mut second = params_for(&tree, 2);
    second.badge_glyph = "🌱".to_string();
    svc.water(second).await.expect("second watering");

    let after = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(after - before, super::WATER_CHIP_BONUS);

    let stored = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("tree");
    assert_eq!(stored.last_watered, Some(Utc::now().date_naive()));
    assert_eq!(stored.badge_glyph, "🌱", "the second save still lands");
    assert_eq!(stored.state_revision, 2);

    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert_eq!(event.user_id, Some(user.id));
    assert!(matches!(event.kind, ActivityKind::BonsaiWatered));
    assert!(
        rx.try_recv().is_err(),
        "the same-day second watering announces nothing"
    );
}
