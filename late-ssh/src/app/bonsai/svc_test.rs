use chrono::Utc;
use late_core::models::bonsai::{Tree, TreeWrite};
use late_core::models::chips::UserChips;
use late_core::test_utils::create_test_user;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{Duration, timeout};

use super::{BonsaiOutcome, BonsaiService, WATER_CHIP_BONUS};
use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::app::bonsai::state::{Applied, BonsaiCommand};
use crate::test_helpers::new_test_db;

const WATER: BonsaiCommand = BonsaiCommand::Water;

fn write_from(tree: &Tree) -> TreeWrite {
    TreeWrite {
        user_id: tree.user_id,
        seed: tree.seed,
        last_watered: tree.last_watered,
        is_alive: tree.is_alive,
        vigor: tree.vigor,
        water_stress: tree.water_stress,
        last_simulated_date: tree.last_simulated_date,
        branch_graph: tree.branch_graph.clone(),
        selected_branch_id: tree.selected_branch_id,
        mode: tree.mode.clone(),
        badge_glyph: tree.badge_glyph.clone(),
        planted_at: tree.planted_at,
    }
}

#[tokio::test]
async fn ensure_tree_plants_a_fresh_seed_for_a_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-svc-new").await;
    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);

    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    let protection = svc
        .decay_protection(user.id)
        .await
        .expect("decay protection");

    assert_eq!(tree.user_id, user.id);
    assert_eq!(tree.seed, user.id.as_u128() as i64);
    assert_eq!(tree.last_watered, None);
    assert!(tree.is_alive);
    assert_eq!(tree.badge_glyph, "·", "a seed has no presence yet");
    assert_eq!(tree.state_revision, 0, "a fresh tree needs no settling");
    assert!(protection.is_none());
    assert!(
        tree.branch_graph.get("branches").is_some(),
        "the planted graph is the seeded root, not an empty document"
    );
}

// The reported bug: a second session, still holding its login-time copy of
// the tree, waters after the first already did. The stored tree must come
// through the second watering untouched, and the chips are paid once.
#[tokio::test]
async fn a_second_watering_the_same_day_changes_nothing() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-svc-water").await;
    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    svc.ensure_tree(user.id).await.expect("ensure tree");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    let first = svc.act(user.id, WATER).await.expect("first watering");
    assert_eq!(first.applied, Applied::Watered);
    assert_eq!(
        first.message,
        Some(format!("Watered (+{WATER_CHIP_BONUS} chips)"))
    );
    assert_eq!(first.tree.last_watered, Some(Utc::now().date_naive()));

    let second = svc.act(user.id, WATER).await.expect("second watering");
    assert_eq!(second.applied, Applied::Unchanged);
    assert_eq!(second.message.as_deref(), Some("Already watered today"));

    let stored = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("tree");
    assert_eq!(stored.branch_graph, first.tree.branch_graph);
    assert_eq!(stored.vigor, first.tree.vigor);
    assert_eq!(stored.water_stress, first.tree.water_stress);
    assert_eq!(stored.state_revision, first.tree.state_revision);

    let after = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(after - before, WATER_CHIP_BONUS);
}

#[tokio::test]
async fn watering_announces_once_through_the_task() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-svc-announce").await;
    let (tx, mut rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    svc.ensure_tree(user.id).await.expect("ensure tree");
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel();

    svc.act_task(user.id, WATER, reply_tx.clone());
    let first = timeout(Duration::from_secs(5), reply_rx.recv())
        .await
        .expect("first answer in time")
        .expect("first answer");
    svc.act_task(user.id, WATER, reply_tx);
    let second = timeout(Duration::from_secs(5), reply_rx.recv())
        .await
        .expect("second answer in time")
        .expect("second answer");

    assert!(matches!(
        first,
        BonsaiOutcome::Acted { message: Some(ref m), .. } if m.starts_with("Watered (+")
    ));
    assert!(matches!(
        second,
        BonsaiOutcome::Acted { message: Some(ref m), .. } if m == "Already watered today"
    ));
    let event = rx.try_recv().expect("the first watering announced");
    assert_eq!(event.user_id, Some(user.id));
    assert!(matches!(event.kind, ActivityKind::BonsaiWatered));
    assert!(
        rx.try_recv().is_err(),
        "the same-day second watering announces nothing"
    );
}

// Eight sessions press `w` at once. The row lock runs them one after the
// other, so exactly one waters and the rest are refused.
#[tokio::test]
async fn concurrent_waterings_grant_the_day_once() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-svc-concurrent").await;
    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    svc.ensure_tree(user.id).await.expect("ensure tree");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    let mut handles = Vec::new();
    for _ in 0..8 {
        let svc = svc.clone();
        let user_id = user.id;
        handles.push(tokio::spawn(async move {
            svc.act(user_id, WATER).await.expect("watering").applied
        }));
    }
    let mut watered = 0;
    for handle in handles {
        if handle.await.expect("join") == Applied::Watered {
            watered += 1;
        }
    }

    assert_eq!(watered, 1);
    let after = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(after - before, WATER_CHIP_BONUS);
}

// A row last simulated days ago is caught up under the lock at login, and
// a second login the same day finds nothing left to do.
#[tokio::test]
async fn ensure_tree_settles_elapsed_days_once() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "bonsai-svc-settle").await;
    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    let planted = svc.ensure_tree(user.id).await.expect("ensure tree");

    let mut neglected = write_from(&planted);
    neglected.last_simulated_date = Utc::now().date_naive() - chrono::Duration::days(3);
    let neglected = Tree::store(&**client, neglected).await.expect("store");

    let settled = svc.ensure_tree(user.id).await.expect("first login");
    assert_eq!(settled.last_simulated_date, Utc::now().date_naive());
    assert_eq!(settled.water_stress, neglected.water_stress + 33);
    assert_eq!(settled.vigor, neglected.vigor - 21);
    assert_eq!(settled.state_revision, neglected.state_revision + 1);

    let again = svc.ensure_tree(user.id).await.expect("second login");
    assert_eq!(again.state_revision, settled.state_revision);
    assert_eq!(again.water_stress, settled.water_stress);
}

#[tokio::test]
async fn an_unreadable_change_payload_is_dropped() {
    let test_db = new_test_db().await;
    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    let mut changes = svc.subscribe_changes();
    let user_id = uuid::Uuid::now_v7();

    svc.publish_change("not-a-uuid");
    svc.publish_change(&user_id.to_string());

    assert_eq!(changes.try_recv().expect("the readable notice"), user_id);
    assert!(changes.try_recv().is_err());
}
