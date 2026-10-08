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

#[tokio::test]
async fn profile_edits_save_the_latest_draft_while_an_earlier_write_is_blocked() {
    use crate::test_helpers::{new_test_db, wait_until};
    use late_core::models::statusline::default_statusline_components;
    use late_core::models::user::{
        ArtSplashMode, LandingPage, RightSidebarMode, RoomListMode, TerminalImagesMode,
        default_right_sidebar_components,
    };
    use late_core::test_utils::create_test_user;

    let draft = |ide: &str| ProfileParams {
        username: "ordered-edits".to_string(),
        bio: String::new(),
        country: None,
        timezone: None,
        ide: Some(ide.to_string()),
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
        statusline_components: default_statusline_components(),
        show_room_list_sidebar: true,
        room_list_mode: RoomListMode::On,
        keep_composer_focused: false,
        start_with_music_muted: false,
        landing_page: LandingPage::Clubhouse,
        paper_at_login: true,
        screensaver: late_core::models::user::Screensaver::DEFAULT,
        show_watch_chat: true,
        art_splash_mode: ArtSplashMode::Sfw,
        terminal_images: TerminalImagesMode::Auto,
        hidden_award_categories: Vec::new(),
        show_flag_fallback: false,
        translate_to: late_core::models::message_translation::TranslateLang::En,
        auto_translate: false,
        translate_mine_to_en: false,
        favorite_room_ids: Vec::new(),
        favorite_theme_ids: Vec::new(),
    };

    let db = new_test_db().await;
    let user = create_test_user(&db.db, "ordered-edits").await;
    let service = ProfileService::new(db.db.clone(), Default::default());
    let mut client = db.db.get().await.unwrap();
    let transaction = client.transaction().await.unwrap();
    transaction
        .query_one("SELECT id FROM users WHERE id = $1 FOR UPDATE", &[&user.id])
        .await
        .unwrap();

    // Every save carries the whole draft, so the newest one must land last.
    for ide in ["vim", "helix", "zed", "emacs"] {
        service.edit_profile(user.id, draft(ide));
    }
    transaction.commit().await.unwrap();
    wait_until(
        || async { service.profile_edit_writes.lock_recover().is_empty() },
        "profile edit writes drained",
    )
    .await;
    let saved = Profile::load(&client, user.id).await.unwrap();
    assert_eq!(saved.ide.as_deref(), Some("emacs"));
}
