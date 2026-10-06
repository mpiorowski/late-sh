use super::*;

#[test]
fn profile_snapshot_default_is_empty() {
    let snapshot = ProfileSnapshot::default();
    assert_eq!(snapshot.user_id, None);
    assert!(snapshot.profile.is_none());
    assert!(snapshot.bonsai.is_none());
}

#[test]
fn should_prune_when_only_one_receiver_remains() {
    let (tx, _rx) = watch::channel(ProfileSnapshot::default());
    assert!(should_prune_snapshot_sender(&tx));
}

#[test]
fn should_not_prune_when_multiple_receivers_exist() {
    let (tx, _rx1) = watch::channel(ProfileSnapshot::default());
    let _rx2 = tx.subscribe();
    assert!(!should_prune_snapshot_sender(&tx));
}

#[test]
fn should_prune_when_channel_is_closed() {
    let (tx, rx) = watch::channel(ProfileSnapshot::default());
    drop(rx);
    assert!(should_prune_snapshot_sender(&tx));
}

#[tokio::test]
async fn interaction_mode_saves_latest_choice_while_an_earlier_write_is_blocked() {
    use crate::test_helpers::{new_test_db, wait_until};
    use late_core::models::user::extract_interaction_mode;
    use late_core::test_utils::create_test_user;

    let db = new_test_db().await;
    let user = create_test_user(&db.db, "blocked-mode").await;
    let other = create_test_user(&db.db, "independent-mode").await;
    let service = ProfileService::new(db.db.clone(), Default::default());
    let mut client = db.db.get().await.unwrap();
    let transaction = client.transaction().await.unwrap();
    transaction
        .query_one("SELECT id FROM users WHERE id = $1 FOR UPDATE", &[&user.id])
        .await
        .unwrap();

    service.set_interaction_mode(user.id, InteractionMode::Keyboard);
    service.set_interaction_mode(other.id, InteractionMode::Mouse);
    wait_until(
        || async {
            let client = db.db.get().await.unwrap();
            let saved = User::get(&client, other.id).await.unwrap().unwrap();
            extract_interaction_mode(&saved.settings) == Some(InteractionMode::Mouse)
        },
        "independent interaction mode write",
    )
    .await;
    for mode in [
        InteractionMode::Mouse,
        InteractionMode::Keyboard,
        InteractionMode::Hybrid,
    ] {
        service.set_interaction_mode(user.id, mode);
    }
    transaction.commit().await.unwrap();
    wait_until(
        || async { service.interaction_mode_writes.lock_recover().is_empty() },
        "interaction mode writes drained",
    )
    .await;
    let saved = User::get(&client, user.id).await.unwrap().unwrap();
    assert_eq!(
        extract_interaction_mode(&saved.settings),
        Some(InteractionMode::Hybrid)
    );
}
