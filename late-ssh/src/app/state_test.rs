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
