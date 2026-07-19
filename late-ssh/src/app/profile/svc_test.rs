//! Service integration tests for profile flows against a real ephemeral DB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::app::profile::svc::{ProfileEvent, ProfileService};
use crate::session::{SessionMessage, SessionRegistry};
use crate::state::{ActiveSession, ActiveUser};
use crate::test_helpers::new_test_db;
use late_core::models::{
    artboard_ban::ArtboardBan,
    chat_room::ChatRoom,
    chips::{INITIAL_CHIP_BALANCE, UserChips},
    moderation_audit_log::ModerationAuditLog,
    profile::{Profile, ProfileParams},
    room_ban::RoomBan,
    server_ban::{ServerBan, ServerBanActivation},
    user::{RightSidebarMode, User, UserParams, default_right_sidebar_components},
};
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep, timeout};

fn default_active_users() -> crate::state::ActiveUsers {
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
    assert_eq!(snapshot.chip_balance, Some(INITIAL_CHIP_BALANCE));
    assert_eq!(profile.username, "profile-user");

    let client = test_db.db.get().await.expect("db client");
    let chips = UserChips::find(&client, user.id).await.expect("load chips");
    assert!(chips.is_none(), "profile access must not create a chip row");
}

#[tokio::test]
async fn find_profile_publishes_stored_chip_balance() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "profile-chip-user").await;
    UserChips::ensure(&client, user.id)
        .await
        .expect("ensure chips");
    let chips = UserChips::add_bonus(&client, user.id, 250)
        .await
        .expect("add chips");

    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut snapshot_rx = service.subscribe_snapshot(user.id);

    service.find_profile(user.id);

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();

    assert_eq!(snapshot.user_id, Some(user.id));
    assert_eq!(snapshot.chip_balance, Some(chips.balance));
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
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            favorite_room_ids: Vec::new(),
            birthday: None,
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
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            favorite_room_ids: Vec::new(),
            birthday: None,
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
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            favorite_room_ids: Vec::new(),
            birthday: None,
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
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");

    ModerationAuditLog::record(
        &client,
        actor.id,
        "server_ban",
        "user",
        Some(target.id),
        serde_json::json!({}),
    )
    .await
    .expect("record audit row");
    RoomBan::activate(&client, room.id, target.id, actor.id, "", None)
        .await
        .expect("activate room ban");
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: target.id,
            fingerprint: None,
            ip_address: None,
            snapshot_username: None,
            actor_user_id: actor.id,
            reason: "",
            expires_at: None,
        },
    )
    .await
    .expect("activate server ban");
    ArtboardBan::activate(&client, target.id, actor.id, "", None)
        .await
        .expect("activate artboard ban");

    let service = ProfileService::new(test_db.db.clone(), default_active_users());

    service.delete_account(actor.id);
    wait_for_user_deleted(&client, actor.id).await;

    // Deleting the actor must not cascade away the moderation records they
    // authored: the target stays banned and the actor's audit trail survives.
    assert_eq!(
        ModerationAuditLog::count_for_actor(&client, actor.id)
            .await
            .expect("count audit rows"),
        1
    );
    assert!(
        RoomBan::is_active_for_room_and_user(&client, room.id, target.id)
            .await
            .expect("room ban lookup"),
        "room ban survives actor deletion"
    );
    assert!(
        ServerBan::find_active_for_user_id(&client, target.id)
            .await
            .expect("server ban lookup")
            .is_some(),
        "server ban survives actor deletion"
    );
    assert!(
        ArtboardBan::is_active_for_user(&client, target.id)
            .await
            .expect("artboard ban lookup"),
        "artboard ban survives actor deletion"
    );

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

    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: target.id,
            fingerprint: Some(&target.fingerprint),
            ip_address: Some(banned_ip),
            snapshot_username: Some(&target.username),
            actor_user_id: actor.id,
            reason: "",
            expires_at: None,
        },
    )
    .await
    .expect("activate server ban");

    let service = ProfileService::new(test_db.db.clone(), default_active_users());

    service.delete_account(target.id);
    wait_for_user_deleted(&client, target.id).await;

    assert!(
        ServerBan::find_active_for_user_id(&client, target.id)
            .await
            .expect("server ban lookup")
            .is_some(),
        "server ban row survives target deletion"
    );
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

    registry
        .register(token.clone(), tx, uuid::Uuid::now_v7())
        .await;
    active_users.lock().expect("active users").insert(
        user.id,
        ActiveUser {
            username: user.username.clone(),
            fingerprint: Some(user.fingerprint.clone()),
            peer_ip: None,
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token,
                fingerprint: Some(user.fingerprint.clone()),
                peer_ip: None,
                afk: None,
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

#[tokio::test]
async fn edit_profile_snapshots_stay_per_user() {
    let test_db = new_test_db().await;
    let user_a = create_test_user(&test_db.db, "profile-scope-a").await;
    let user_b = create_test_user(&test_db.db, "profile-scope-b").await;
    let service = ProfileService::new(test_db.db.clone(), default_active_users());
    let mut a_rx = service.subscribe_snapshot(user_a.id);
    let mut b_rx = service.subscribe_snapshot(user_b.id);

    service.find_profile(user_a.id);
    timeout(Duration::from_secs(2), a_rx.changed())
        .await
        .expect("a snapshot timeout")
        .expect("watch changed");
    service.find_profile(user_b.id);
    timeout(Duration::from_secs(2), b_rx.changed())
        .await
        .expect("b snapshot timeout")
        .expect("watch changed");
    let b_username = b_rx
        .borrow_and_update()
        .profile
        .clone()
        .expect("b profile")
        .username;

    service.edit_profile(
        user_a.id,
        ProfileParams {
            username: "scoped-owl".to_string(),
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
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            favorite_room_ids: Vec::new(),
            birthday: None,
        },
    );

    // A's own snapshot refresh marks the save task as fully processed.
    timeout(Duration::from_secs(2), a_rx.changed())
        .await
        .expect("a updated snapshot timeout")
        .expect("watch changed");
    assert_eq!(
        a_rx.borrow_and_update()
            .profile
            .clone()
            .expect("a profile")
            .username,
        "scoped-owl"
    );

    assert!(
        !b_rx.has_changed().expect("b channel alive"),
        "user A's save must not push a snapshot to user B"
    );
    assert_eq!(
        b_rx.borrow().profile.clone().expect("b profile").username,
        b_username,
        "user B's profile must be untouched by user A's save"
    );
}
