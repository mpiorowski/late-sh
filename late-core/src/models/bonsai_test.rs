use crate::{
    models::{
        bonsai::{Tree, TreeParams},
        user::{User, UserParams},
    },
    test_utils::test_db,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Barrier;
use uuid::Uuid;

async fn create_user(client: &tokio_postgres::Client, name: &str) -> User {
    User::create(
        client,
        UserParams {
            fingerprint: name.to_string(),
            username: name.to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user")
}

fn params_from(tree: &Tree, state_revision: i64) -> TreeParams {
    TreeParams {
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
        state_revision,
    }
}

#[tokio::test]
async fn ensure_plants_once_and_then_returns_the_existing_row() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_user(&client, "bonsai-model-user").await;
    let today = Utc::now().date_naive();

    let tree = Tree::ensure(
        &client,
        user.id,
        1234,
        today,
        serde_json::json!({"version": 1, "next_id": 2, "branches": []}),
        "·",
    )
    .await
    .expect("ensure");
    assert_eq!(tree.user_id, user.id);
    assert_eq!(tree.seed, 1234);
    assert_eq!(tree.last_watered, None);
    assert!(tree.is_alive);
    assert_eq!(tree.vigor, 70);
    assert_eq!(tree.state_revision, 0);

    let again = Tree::ensure(&client, user.id, 999, today, serde_json::json!({}), "🌼")
        .await
        .expect("ensure again");
    assert_eq!(again.id, tree.id);
    assert_eq!(
        again.seed, 1234,
        "an existing tree is never replanted by ensure"
    );
    assert_eq!(again.badge_glyph, "·");
}

#[tokio::test]
async fn concurrent_water_days_grant_the_day_once() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_user(&client, "bonsai-model-concurrent-water").await;
    let today = Utc::now().date_naive();

    Tree::ensure(&client, user.id, 22, today, serde_json::json!({}), "·")
        .await
        .expect("ensure");
    drop(client);

    let task_count = 8;
    let barrier = Arc::new(Barrier::new(task_count));
    let mut handles = Vec::new();
    for _ in 0..task_count {
        let db = test_db.db.clone();
        let barrier = Arc::clone(&barrier);
        let user_id = user.id;
        handles.push(tokio::spawn(async move {
            let client = db.get().await.expect("db client");
            barrier.wait().await;
            Tree::water_day(&client, user_id, today)
                .await
                .expect("water")
        }));
    }

    let mut granted = 0;
    for handle in handles {
        if handle.await.expect("join water task") {
            granted += 1;
        }
    }

    let client = test_db.db.get().await.expect("db client");
    let tree = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find tree")
        .expect("tree");
    assert_eq!(granted, 1);
    assert_eq!(tree.last_watered, Some(today));
    assert_eq!(tree.state_revision, 0, "the gate never bumps the revision");
}

#[tokio::test]
async fn a_dead_tree_does_not_take_the_water_day() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_user(&client, "bonsai-model-dead-water").await;
    let today = Utc::now().date_naive();

    let tree = Tree::ensure(&client, user.id, 5, today, serde_json::json!({}), "·")
        .await
        .expect("ensure");
    let mut dead = params_from(&tree, 1);
    dead.is_alive = false;
    Tree::save(&client, dead).await.expect("save dead");

    let granted = Tree::water_day(&client, user.id, today)
        .await
        .expect("water");
    assert!(!granted);
}

#[tokio::test]
async fn save_ignores_a_stale_revision() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_user(&client, "bonsai-model-revision").await;
    let today = Utc::now().date_naive();

    let tree = Tree::ensure(&client, user.id, 7, today, serde_json::json!({}), "·")
        .await
        .expect("ensure");

    let mut newer = params_from(&tree, 3);
    newer.badge_glyph = "🌳".to_string();
    Tree::save(&client, newer).await.expect("save newer");

    let mut stale = params_from(&tree, 2);
    stale.badge_glyph = "⚘".to_string();
    Tree::save(&client, stale).await.expect("save stale");

    let stored = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("tree");
    assert_eq!(stored.badge_glyph, "🌳");
    assert_eq!(stored.state_revision, 3);
}

#[tokio::test]
async fn watering_is_scoped_to_the_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_user(&client, "bonsai-scope-owner").await;
    let other = create_user(&client, "bonsai-scope-other").await;
    let today = Utc::now().date_naive();

    let owner_tree = Tree::ensure(&client, owner.id, 1, today, serde_json::json!({}), "·")
        .await
        .expect("ensure owner tree");
    let other_tree = Tree::ensure(&client, other.id, 2, today, serde_json::json!({}), "·")
        .await
        .expect("ensure other tree");
    assert_ne!(owner_tree.id, other_tree.id);

    assert!(
        Tree::water_day(&client, owner.id, today)
            .await
            .expect("water owner tree")
    );

    let other_after = Tree::find_by_user_id(&client, other.id)
        .await
        .expect("find other tree")
        .expect("other tree exists");
    assert_eq!(other_after.id, other_tree.id);
    assert_eq!(
        other_after.last_watered, None,
        "watering one user's tree must not touch another user's row"
    );

    let nobody = Tree::find_by_user_id(&client, Uuid::now_v7())
        .await
        .expect("find missing");
    assert!(nobody.is_none());
}
