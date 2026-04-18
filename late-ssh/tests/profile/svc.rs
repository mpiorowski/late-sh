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
    assert_eq!(profile.username, "profile-user");
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
    let _ = snapshot_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("initial profile");

    service.edit_profile(
        user.id,
        ProfileParams {
            username: "night-owl".to_string(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            enable_background_color: false,
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
}

#[tokio::test]
async fn edit_profile_normalizes_username_before_persisting() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "profile-normalize-user").await;
    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut snapshot_rx = service.subscribe_snapshot(user.id);

    service.find_profile(user.id);
    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("initial snapshot timeout")
        .expect("watch changed");
    let _ = snapshot_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("initial profile");

    service.edit_profile(
        user.id,
        ProfileParams {
            username: "  late night!!!  ".to_string(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            enable_background_color: false,
        },
    );

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("updated snapshot timeout")
        .expect("watch changed");
    let updated = snapshot_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("updated profile");

    assert_eq!(updated.username, "late_night");
}

#[tokio::test]
async fn edit_profile_preserves_unrelated_settings_keys() {
    // Concurrent write paths (theme_id, ignored_user_ids) must survive a
    // profile save. The atomic `settings || jsonb_build_object(...)` merge
    // in Profile::update is what guarantees this.
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "profile-merge-user").await;

    late_core::models::user::User::set_theme_id(&client, user.id, "purple")
        .await
        .expect("set theme");

    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut snapshot_rx = service.subscribe_snapshot(user.id);

    service.find_profile(user.id);
    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("initial snapshot timeout")
        .expect("watch changed");

    service.edit_profile(
        user.id,
        ProfileParams {
            username: "merge-user".to_string(),
            notify_kinds: vec!["dms".to_string()],
            notify_bell: false,
            notify_cooldown_mins: 5,
            enable_background_color: false,
        },
    );

    // Wait for the DB write to land.
    let mut events = service.subscribe_events();
    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    assert!(matches!(event, ProfileEvent::Saved { .. }));

    let theme = late_core::models::user::User::theme_id(&client, user.id)
        .await
        .expect("load theme");
    assert_eq!(theme.as_deref(), Some("purple"));
}

#[tokio::test]
async fn creating_profiles_for_same_ssh_username_assigns_unique_handles() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let first = create_test_user(&test_db.db, "alice").await;
    let second = create_test_user(&test_db.db, "alice").await;

    let first_profile = Profile::load(&client, first.id)
        .await
        .expect("first profile");
    let second_profile = Profile::load(&client, second.id)
        .await
        .expect("second profile");

    assert_eq!(first_profile.username, "alice");
    assert_eq!(second_profile.username, "alice-2");
}
