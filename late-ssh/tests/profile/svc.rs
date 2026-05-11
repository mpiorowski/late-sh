//! Service integration tests for profile flows against a real ephemeral DB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::helpers::new_test_db;
use late_core::models::{
    chat_room::ChatRoom,
    profile::{Profile, ProfileParams},
    server_ban::ServerBan,
    user::{RightSidebarMode, User, UserParams},
};
use late_core::test_utils::create_test_user;
use late_ssh::app::profile::svc::{ProfileEvent, ProfileService};
use late_ssh::session::{SessionMessage, SessionRegistry};
use late_ssh::state::{ActiveSession, ActiveUser};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep, timeout};

fn default_active_users() -> late_ssh::state::ActiveUsers {
    Arc::new(Mutex::new(HashMap::new()))
}

async fn wait_for_user_deleted(client: &tokio_postgres::Client, user_id: uuid::Uuid) {
    timeout(Duration::from_secs(2), async {
        loop {
            let deleted = User::get(client, user_id).await.expect("load user");
            if deleted.is_none() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("delete timeout");
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
            bio: String::new(),
            country: None,
            timezone: None,
            ide: None,
            terminal: None,
            os: None,
            langs: Vec::new(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: None,
            enable_background_color: false,
            show_dashboard_header: false,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_screens: vec![1, 2, 3, 4, 5],
            show_arcade_sidebar: true,
            show_settings_on_connect: true,
            favorite_room_ids: Vec::new(),
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
    assert!(!updated.show_dashboard_header);
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
            bio: String::new(),
            country: None,
            timezone: None,
            ide: None,
            terminal: None,
            os: None,
            langs: Vec::new(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: None,
            enable_background_color: false,
            show_dashboard_header: true,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_screens: vec![1, 2, 3, 4, 5],
            show_arcade_sidebar: true,
            show_settings_on_connect: true,
            favorite_room_ids: Vec::new(),
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
            bio: String::new(),
            country: None,
            timezone: None,
            ide: None,
            terminal: None,
            os: None,
            langs: Vec::new(),
            notify_kinds: vec!["dms".to_string()],
            notify_bell: false,
            notify_cooldown_mins: 5,
            notify_format: None,
            theme_id: None,
            enable_background_color: false,
            show_dashboard_header: true,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_screens: vec![1, 2, 3, 4, 5],
            show_arcade_sidebar: true,
            show_settings_on_connect: true,
            favorite_room_ids: Vec::new(),
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

#[tokio::test]
async fn delete_account_preserves_moderation_rows_and_allows_key_reuse() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "delete-actor").await;
    let target = create_test_user(&test_db.db, "delete-target").await;
    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");

    client
        .execute(
            "INSERT INTO moderation_audit_log
             (actor_user_id, action, target_kind, target_id)
             VALUES ($1, 'server_ban', 'user', $2)",
            &[&actor.id, &target.id],
        )
        .await
        .expect("insert audit row");
    client
        .execute(
            "INSERT INTO room_bans
             (room_id, target_user_id, actor_user_id)
             VALUES ($1, $2, $3)",
            &[&room.id, &target.id, &actor.id],
        )
        .await
        .expect("insert room ban");
    client
        .execute(
            "INSERT INTO server_bans
             (target_user_id, actor_user_id)
             VALUES ($1, $2)",
            &[&target.id, &actor.id],
        )
        .await
        .expect("insert server ban");
    client
        .execute(
            "INSERT INTO artboard_bans
             (target_user_id, actor_user_id)
             VALUES ($1, $2)",
            &[&target.id, &actor.id],
        )
        .await
        .expect("insert artboard ban");

    let service = ProfileService::new(test_db.db.clone(), default_active_users());

    service.delete_account(actor.id);
    wait_for_user_deleted(&client, actor.id).await;
    let audit_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM moderation_audit_log WHERE actor_user_id = $1",
            &[&actor.id],
        )
        .await
        .expect("count audit rows")
        .get(0);
    let room_ban_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM room_bans WHERE actor_user_id = $1",
            &[&actor.id],
        )
        .await
        .expect("count room bans")
        .get(0);
    let server_ban_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM server_bans WHERE actor_user_id = $1",
            &[&actor.id],
        )
        .await
        .expect("count server bans")
        .get(0);
    let artboard_ban_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM artboard_bans WHERE actor_user_id = $1",
            &[&actor.id],
        )
        .await
        .expect("count artboard bans")
        .get(0);
    assert_eq!(audit_count, 1);
    assert_eq!(room_ban_count, 1);
    assert_eq!(server_ban_count, 1);
    assert_eq!(artboard_ban_count, 1);

    let recreated = User::create(
        &client,
        UserParams {
            fingerprint: actor.fingerprint.clone(),
            username: "delete-actor-again".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("recreate user with same fingerprint");
    assert_ne!(recreated.id, actor.id);
}

#[tokio::test]
async fn delete_account_preserves_server_ban_against_deleted_target() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "target-delete-ban-actor").await;
    let target = create_test_user(&test_db.db, "target-delete-banned").await;
    let banned_ip = "203.0.113.77";

    client
        .execute(
            "INSERT INTO server_bans
             (target_user_id, fingerprint, ip_address, snapshot_username, actor_user_id)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &target.id,
                &target.fingerprint,
                &banned_ip,
                &target.username,
                &actor.id,
            ],
        )
        .await
        .expect("insert server ban");

    let service = ProfileService::new(test_db.db.clone(), default_active_users());

    service.delete_account(target.id);
    wait_for_user_deleted(&client, target.id).await;

    let ban_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM server_bans WHERE target_user_id = $1",
            &[&target.id],
        )
        .await
        .expect("count server bans")
        .get(0);
    assert_eq!(ban_count, 1);
    assert!(
        ServerBan::find_active_for_fingerprint(&client, &target.fingerprint)
            .await
            .expect("lookup fingerprint ban")
            .is_some()
    );
    assert!(
        ServerBan::find_active_for_ip_address(&client, banned_ip)
            .await
            .expect("lookup ip ban")
            .is_some()
    );
}

#[tokio::test]
async fn delete_account_terminates_active_sessions() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "delete-session-user").await;
    let active_users = default_active_users();
    let registry = SessionRegistry::new();
    let token = "delete-session-token".to_string();
    let (tx, mut rx) = mpsc::channel(1);

    registry.register(token.clone(), tx).await;
    active_users.lock().expect("active users").insert(
        user.id,
        ActiveUser {
            username: user.username.clone(),
            fingerprint: Some(user.fingerprint.clone()),
            peer_ip: None,
            sessions: vec![ActiveSession {
                token,
                fingerprint: Some(user.fingerprint.clone()),
                peer_ip: None,
            }],
            connection_count: 1,
            last_login_at: Instant::now(),
        },
    );

    let service = ProfileService::new(test_db.db.clone(), active_users.clone())
        .with_session_registry(registry);

    service.delete_account(user.id);

    let msg = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("terminate timeout")
        .expect("terminate message");
    assert!(matches!(
        msg,
        SessionMessage::Terminate { reason } if reason == "account deleted"
    ));
    wait_for_user_deleted(&client, user.id).await;
    assert!(
        !active_users
            .lock()
            .expect("active users")
            .contains_key(&user.id)
    );
}
