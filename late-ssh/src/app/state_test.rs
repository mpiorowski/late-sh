use crate::test_helpers::{
    SessionWorld, make_app, make_app_in_world, new_test_db, render_plain, wait_for_render_contains,
};
use late_core::models::leaderboard::{LeaderboardData, LeaderboardEntry};
use late_core::test_utils::create_test_user;
use std::sync::Arc;
use tokio::sync::watch;

#[tokio::test]
async fn splash_screen_renders_selected_hint_with_existing_copy_and_dismisses() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tip-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tip-session");
    let existing_copy = "take a break, grab a coffee";

    app.show_splash_for_tests("Type /help in chat for a list of available chat commands");
    // This test covers composition, not the typewriter's wall-clock duration.
    // Put the next tick on the completed-copy frame instead of polling through
    // every intermediate character.
    app.splash_ticks = existing_copy.len().saturating_sub(1);

    wait_for_render_contains(&mut app, existing_copy).await;
    wait_for_render_contains(
        &mut app,
        "Type /help in chat for a list of available chat commands",
    )
    .await;

    app.handle_input(b"\x1b");
    assert!(
        !app.show_splash,
        "Esc should dismiss the splash immediately"
    );
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains("Type /help in chat for a list of available chat commands"),
        "tip should be gone once splash is dismissed"
    );
}

/// A session must start from the snapshot `LeaderboardService` has already
/// published. `watch::Sender::subscribe` marks the current value as seen, so the
/// `has_changed()` gate in `tick.rs` never fires for it: before `App::new`
/// seeded from `borrow()`, every session rendered empty leaderboard panels until
/// the next refresh landed, up to `REFRESH_INTERVAL` (300s) later.
#[tokio::test]
async fn leaderboard_seeds_from_the_already_published_snapshot() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "leaderboard-seed-it").await;

    let published = LeaderboardData {
        today_champions: vec![LeaderboardEntry {
            username: "already-here".to_string(),
            user_id: user.id,
            count: 3,
        }],
        ..LeaderboardData::default()
    };
    let (tx, rx) = watch::channel(Arc::new(published));

    let mut app = make_app_in_world(
        test_db.db.clone(),
        user.id,
        "leaderboard-seed-session",
        SessionWorld {
            leaderboard_rx: Some(rx),
            ..SessionWorld::default()
        },
    );

    assert_eq!(
        app.leaderboard
            .today_champions
            .first()
            .map(|entry| entry.username.as_str()),
        Some("already-here"),
        "session must seed from the published snapshot instead of waiting for the next send"
    );

    // The seed must not cost the tick gate its job: a genuinely new snapshot
    // still lands on top of it.
    tx.send(Arc::new(LeaderboardData::default()))
        .expect("session holds the receiver");
    app.tick();

    assert!(
        app.leaderboard.today_champions.is_empty(),
        "a later refresh must still replace the seeded snapshot"
    );
}

/// Away belongs to a session, but peers read it per user: the user is away
/// only once every session is. A `/brb` on the desktop must not hide a laptop
/// that is still being typed on, and a key on either brings the user back.
#[tokio::test]
async fn a_user_is_away_only_once_every_session_is() {
    use crate::app::common::away::{AWAY_AFTER, away_user_ids};
    use crate::state::{ActiveSession, ActiveUser, ActiveUsers};

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "away-two-sessions").await;
    let session = |token: &str| ActiveSession {
        token: token.to_string(),
        fingerprint: Some(user.fingerprint.clone()),
        peer_ip: None,
        away: false,
    };
    let active_users: ActiveUsers = Arc::new(std::sync::Mutex::new(
        [(
            user.id,
            ActiveUser {
                username: user.username.clone(),
                fingerprint: Some(user.fingerprint.clone()),
                audio_source: late_core::models::user::AudioSource::default(),
                sessions: vec![session("laptop"), session("desktop")],
                connection_count: 2,
                last_login_at: std::time::Instant::now(),
            },
        )]
        .into(),
    ));
    let world = SessionWorld {
        active_users: Some(active_users.clone()),
        ..SessionWorld::default()
    };
    let mut laptop = make_app_in_world(test_db.db.clone(), user.id, "laptop", world.clone());
    let mut desktop = make_app_in_world(test_db.db.clone(), user.id, "desktop", world);
    let is_away = || away_user_ids(&active_users.lock().unwrap()).contains(&user.id);

    desktop.sent_away = true;
    assert!(desktop.sync_away(), "/brb moves the desktop's flag");
    assert!(!laptop.sync_away(), "the laptop was just used");
    assert!(!is_away(), "the laptop keeps the user here");

    laptop.last_active_at = std::time::Instant::now() - AWAY_AFTER;
    assert!(laptop.sync_away());
    assert!(is_away(), "both sessions are away now");
    assert!(!laptop.sync_away(), "an unchanged flag is not rewritten");

    desktop.handle_input(b"j");
    assert!(!desktop.away, "any key brings the desktop back, at once");
    assert!(!desktop.sync_away(), "the key already synced the flag");
    assert!(!is_away());
}

/// `/brb` promises "until your next key". Any-event mouse tracking reports
/// the pointer merely crossing the terminal, and that is not a key: a
/// session sent away stays away through it and comes back on a real one.
#[tokio::test]
async fn brb_holds_through_mouse_motion_until_a_key() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "brb-motion").await;
    let mut app = make_app_in_world(test_db.db.clone(), user.id, "brb", SessionWorld::default());

    app.sent_away = true;
    assert!(app.sync_away(), "/brb sends the session away");

    // SGR any-event motion: button code 35 is the motion bit with no button.
    app.handle_input(b"\x1b[<35;20;5M");
    assert!(!app.sync_away(), "a bare mouse move is not a key");
    assert!(app.away);

    app.handle_input(b"j");
    assert!(!app.away, "a key brings the session back, at once");
    assert!(!app.sync_away(), "the key already synced the flag");
}

/// The 30-minute clock counts from the last thing a person did. A pointer
/// resting on (or drifting over) the terminal reports motion all the time
/// under any-event tracking; it must not keep the session here, or a
/// terminal left open under the mouse never goes away.
#[tokio::test]
async fn a_pointer_over_the_terminal_does_not_hold_off_away() {
    use crate::app::common::away::AWAY_AFTER;
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "away-pointer").await;
    let mut app = make_app_in_world(
        test_db.db.clone(),
        user.id,
        "pointer",
        SessionWorld::default(),
    );

    app.last_active_at = std::time::Instant::now() - AWAY_AFTER;
    app.handle_input(b"\x1b[<35;20;5M");
    app.handle_input(b"\x1b[I");
    assert!(
        app.sync_away(),
        "thirty quiet minutes are away, pointer or not"
    );
    assert!(app.away);
}

/// Away with the Tweak at its default, the aurora covers the screen. The
/// pointer crossing it changes nothing, and the key that drops it is
/// swallowed: `?` over the screensaver opens no guide, the next one does.
#[tokio::test]
async fn the_key_that_drops_the_screensaver_is_swallowed() {
    use crate::app::common::away::AWAY_AFTER;
    use late_core::models::user::{AsciiPiece, Scene, SceneStyle};
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "saver-key").await;
    let mut app = make_app_in_world(
        test_db.db.clone(),
        user.id,
        "saver",
        SessionWorld::default(),
    );
    app.set_screen(crate::app::common::primitives::Screen::Dashboard);
    assert_eq!(app.screensaver(), None, "a session that is here has none");

    app.last_active_at = std::time::Instant::now() - AWAY_AFTER;
    assert!(app.sync_away());
    let aurora = AsciiPiece::Scene(Scene::AuroraFjord, SceneStyle::Dots);
    assert_eq!(app.screensaver(), Some(aurora));

    app.handle_input(b"\x1b[<35;20;5M");
    assert_eq!(app.screensaver(), Some(aurora));

    app.handle_input(b"?");
    assert_eq!(app.screensaver(), None);
    assert!(!app.away);
    assert!(!app.show_help, "the waking key is not the page's");

    app.handle_input(b"?");
    assert!(app.show_help, "the next key is");
}
