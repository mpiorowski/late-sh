use late_core::models::bonsai::{Grave, Tree};
use late_ssh::app::bonsai::svc::BonsaiService;
use late_ssh::state::ActivityEvent;
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};

use super::helpers::new_test_db;
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
    client
        .execute(
            "UPDATE bonsai_trees
             SET created = current_timestamp - interval '8 days',
                 updated = current_timestamp - interval '8 days',
                 last_watered = current_date - 8
             WHERE user_id = $1",
            &[&user.id],
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
