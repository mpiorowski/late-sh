use crate::{
    models::{
        bonsai::{Tree, TreeWrite},
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
async fn ensure_plants_once_and_then_returns_the_existing_row() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_user(&client, "bonsai-model-user").await;
    let today = Utc::now().date_naive();

    let tree = Tree::ensure(
        &**client,
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

    let again = Tree::ensure(&**client, user.id, 999, today, serde_json::json!({}), "🌼")
        .await
        .expect("ensure again");
    assert_eq!(again.id, tree.id);
    assert_eq!(
        again.seed, 1234,
        "an existing tree is never replanted by ensure"
    );
    assert_eq!(again.badge_glyph, "·");
}

// Eight writers each read the row under the lock and store one more point
// of vigor. Without the lock they would all read 70 and the last store
// would win; with it every read sees the previous writer's store.
#[tokio::test]
async fn locked_read_modify_writes_never_lose_an_update() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_user(&client, "bonsai-model-lock").await;
    let today = Utc::now().date_naive();

    let planted = Tree::ensure(&**client, user.id, 22, today, serde_json::json!({}), "·")
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
            let mut client = db.get().await.expect("db client");
            barrier.wait().await;
            let tx = client.transaction().await.expect("tx");
            let tree = Tree::lock(&*tx, user_id).await.expect("lock");
            let mut write = write_from(&tree);
            write.vigor += 1;
            Tree::store(&*tx, write).await.expect("store");
            tx.commit().await.expect("commit");
        }));
    }
    for handle in handles {
        handle.await.expect("join writer");
    }

    let client = test_db.db.get().await.expect("db client");
    let tree = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find tree")
        .expect("tree");
    assert_eq!(tree.vigor, planted.vigor + task_count as i32);
    assert_eq!(
        tree.state_revision, task_count as i64,
        "the revision counts stored writes"
    );
}

#[tokio::test]
async fn store_is_scoped_to_the_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_user(&client, "bonsai-scope-owner").await;
    let other = create_user(&client, "bonsai-scope-other").await;
    let today = Utc::now().date_naive();

    let owner_tree = Tree::ensure(&**client, owner.id, 1, today, serde_json::json!({}), "·")
        .await
        .expect("ensure owner tree");
    let other_tree = Tree::ensure(&**client, other.id, 2, today, serde_json::json!({}), "·")
        .await
        .expect("ensure other tree");

    let mut write = write_from(&owner_tree);
    write.last_watered = Some(today);
    write.badge_glyph = "🌳".to_string();
    let stored = Tree::store(&**client, write).await.expect("store");
    assert_eq!(stored.last_watered, Some(today));
    assert_eq!(stored.badge_glyph, "🌳");
    assert_eq!(stored.state_revision, 1);

    let other_after = Tree::find_by_user_id(&client, other.id)
        .await
        .expect("find other tree")
        .expect("other tree exists");
    assert_eq!(other_after.id, other_tree.id);
    assert_eq!(other_after.last_watered, None);
    assert_eq!(other_after.badge_glyph, "·");
    assert_eq!(
        other_after.state_revision, 0,
        "storing one user's tree must not touch another user's row"
    );

    let nobody = Tree::find_by_user_id(&client, Uuid::now_v7())
        .await
        .expect("find missing");
    assert!(nobody.is_none());
}
