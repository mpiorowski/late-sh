//! Service integration tests for profile flows against a real ephemeral DB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::helpers::new_test_db;
use late_core::models::profile::{Profile, ProfileParams};
use late_core::test_utils::create_test_user;
use late_ssh::app::profile::svc::{ProfileEvent, ProfileService};
use tokio::time::{Duration, timeout};

fn default_active_users() -> late_ssh::state::ActiveUsers {
    Arc::new(Mutex::new(HashMap::new()))
}

#[tokio::test]
async fn find_profile_creates_profile_and_publishes_snapshot() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "profile-user").await;
    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut snapshot_rx = service.subscribe_snapshot(user.id);

    service.find_profile(user.id);

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();
    let profile = snapshot.profile.expect("profile in snapshot");

    assert_eq!(snapshot.user_id, Some(user.id));
    assert_eq!(profile.user_id, user.id);
    assert_eq!(profile.username, "profile-user");
    assert!(profile.enable_ghost);
}

#[tokio::test]
async fn edit_profile_emits_saved_event_and_refreshes_snapshot() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "profile-edit-user").await;
    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut snapshot_rx = service.subscribe_snapshot(user.id);
    let mut events = service.subscribe_events();

    service.find_profile(user.id);
    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("initial snapshot timeout")
        .expect("watch changed");
    let current = snapshot_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("initial profile");

    service.edit_profile(
        user.id,
        current.id,
        ProfileParams {
            user_id: user.id,
            username: "night-owl".to_string(),
            enable_ghost: true,
            notify_kinds: Vec::new(),
            notify_cooldown_mins: 0,
        },
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ProfileEvent::Saved { user_id } => assert_eq!(user_id, user.id),
        _ => panic!("expected saved event"),
    }

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("updated snapshot timeout")
        .expect("watch changed");
    let updated = snapshot_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("updated profile");

    assert_eq!(updated.username, "night-owl");
    assert!(updated.enable_ghost);
}

#[tokio::test]
async fn edit_profile_does_not_modify_another_users_profile() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "profile-owner").await;
    let intruder = create_test_user(&test_db.db, "profile-intruder").await;
    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut owner_snapshot_rx = service.subscribe_snapshot(owner.id);

    service.find_profile(owner.id);
    timeout(Duration::from_secs(2), owner_snapshot_rx.changed())
        .await
        .expect("owner snapshot timeout")
        .expect("watch changed");
    let owner_profile = owner_snapshot_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("owner profile");

    service.edit_profile(
        intruder.id,
        owner_profile.id,
        ProfileParams {
            user_id: intruder.id,
            username: "hijack".to_string(),
            enable_ghost: true,
            notify_kinds: Vec::new(),
            notify_cooldown_mins: 0,
        },
    );

    tokio::time::sleep(Duration::from_millis(200)).await;

    let stored = Profile::find_or_create_by_user(&client, owner.id)
        .await
        .expect("owner profile from db");
    assert_eq!(stored.id, owner_profile.id);
    assert_eq!(stored.username, owner_profile.username);
    assert_eq!(stored.enable_ghost, owner_profile.enable_ghost);
}

#[tokio::test]
async fn creating_profiles_for_same_ssh_username_assigns_unique_handles() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let first = create_test_user(&test_db.db, "alice").await;
    let second = create_test_user(&test_db.db, "alice").await;

    let first_profile = Profile::find_or_create_by_user(&client, first.id)
        .await
        .expect("first profile");
    let second_profile = Profile::find_or_create_by_user(&client, second.id)
        .await
        .expect("second profile");

    assert_eq!(first_profile.username, "alice");
    assert_eq!(second_profile.username, "alice-2");
}
