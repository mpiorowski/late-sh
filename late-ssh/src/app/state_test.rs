use crate::test_helpers::{
    SessionWorld, assert_render_not_contains_for, make_app, make_app_in_world, new_test_db,
    render_plain, wait_for_render_contains,
};
use late_core::models::leaderboard::{LeaderboardData, LeaderboardEntry};
use late_core::test_utils::create_test_user;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Duration;

#[tokio::test]
async fn splash_screen_renders_selected_hint_with_existing_copy() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tip-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tip-session");

    app.show_splash_for_tests("Type /help in chat for a list of available chat commands");

    wait_for_render_contains(&mut app, "take a break, grab a coffee").await;
    wait_for_render_contains(
        &mut app,
        "Type /help in chat for a list of available chat commands",
    )
    .await;
}

#[tokio::test]
async fn splash_hint_disappears_after_splash_is_skipped() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tip-dismiss-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tip-dismiss-session");
    let tip = "Use m, - and = to mute, quiet, or louden the music";

    app.show_splash_for_tests(tip);
    wait_for_render_contains(&mut app, tip).await;

    app.handle_input(b"\x1b");

    assert_render_not_contains_for(&mut app, tip, Duration::from_millis(150)).await;
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains(tip),
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
