use late_core::test_utils::{TestDb, create_test_user};

use super::input::launch;
use super::state::Mode;
use crate::app::common::primitives::Screen;
use crate::app::door::arcade::{ArcadeHandleService, HandleStatus};
use crate::app::state::App;
use crate::test_helpers::{make_app, new_test_db, render_plain, wait_for_app};

/// A session on the Games hub with the Zork client enabled. Entering the
/// menus looks the account's arcade handle up, so the session runs on a real
/// test database under a real user.
async fn zork_app(name: &str) -> (TestDb, App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let mut app = make_app(test_db.db.clone(), user.id, &format!("{name}-flow-it"));
    app.zork_enabled = true;
    app.set_screen(Screen::Games);
    (test_db, app)
}

/// A returning player: the account holds an arcade handle, and the menus'
/// lookup has found it.
async fn claim_handle(test_db: &TestDb, app: &mut App) {
    ArcadeHandleService::new(test_db.db.clone())
        .claim(app.user_id, "tester")
        .await
        .expect("claim the handle");
    launch(app);
    wait_for_app(app, "the handle lookup found it", |app| {
        app.zork_state
            .as_ref()
            .is_some_and(|state| state.handle_status() == HandleStatus::Claimed("tester".into()))
    })
    .await;
}

/// A disabled client is a banner on the hub, never a menu whose actions do
/// nothing; an enabled one opens the edition menu.
#[tokio::test]
async fn the_games_card_respects_the_enabled_gate() {
    let (_test_db, mut app) = zork_app("zork-gate").await;
    app.zork_enabled = false;

    launch(&mut app);
    assert_eq!(app.screen, Screen::Games);
    assert!(app.zork_state.is_none());
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("Zork Trilogy is currently unavailable.")
    );

    app.zork_enabled = true;
    launch(&mut app);
    assert_eq!(app.screen, Screen::Zork);
    assert_eq!(
        app.zork_state.as_ref().map(|state| state.mode()),
        Some(Mode::Editions)
    );
}

/// A state with no live game does not outlive the visit, like every other
/// door: leaving the menus drops it, so the next visit starts fresh instead
/// of repeating the last visit's closing message, and a detached game that
/// ends off-screen is reaped so the hub card and the backtick cycle stop
/// advertising it.
#[tokio::test]
async fn a_state_without_a_live_game_does_not_outlive_the_visit() {
    let (_test_db, mut app) = zork_app("zork-leave").await;

    launch(&mut app);
    assert_eq!(app.screen, Screen::Zork);
    assert!(app.zork_state.is_some());
    app.set_screen(Screen::Games);
    assert!(
        app.zork_state.is_none(),
        "idle menus must not survive leaving"
    );

    launch(&mut app);
    app.zork_state
        .as_mut()
        .expect("zork state")
        .force_running_for_test();
    app.handle_input(b"`");
    assert_ne!(app.screen, Screen::Zork);
    assert!(
        app.zork_state
            .as_ref()
            .is_some_and(|state| state.is_running()),
        "a detached game stays alive"
    );
    // The fabricated proxy points at a dead address: the game ends off-screen.
    wait_for_app(&mut app, "ended game reaped", |app| {
        app.zork_state.is_none()
    })
    .await;
}

/// A lone Esc on the menus backs out one level per press, edition actions
/// to the edition list to the Games hub, as the menu's own key line says.
#[tokio::test]
async fn esc_backs_out_of_the_menus_one_level_at_a_time() {
    let (test_db, mut app) = zork_app("zork-esc").await;
    claim_handle(&test_db, &mut app).await;
    app.handle_input(b"\r");
    assert_eq!(
        app.zork_state.as_ref().map(|state| state.mode()),
        Some(Mode::Actions)
    );

    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "esc to the edition list", |app| {
        app.zork_state
            .as_ref()
            .is_some_and(|state| state.mode() == Mode::Editions)
    })
    .await;
    assert_eq!(app.screen, Screen::Zork);

    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "esc to the games hub", |app| {
        app.screen == Screen::Games
    })
    .await;
}

/// Inside a game every key is the story's, so the frame title is the only
/// place that says how to get out. The ways out lead the hint, so an 80
/// column title still shows them.
#[tokio::test]
async fn a_running_game_names_its_ways_out_in_an_80_column_title() {
    let (_test_db, mut app) = zork_app("zork-title").await;
    app.resize(80, 24).unwrap();
    launch(&mut app);
    app.zork_state
        .as_mut()
        .expect("zork state")
        .force_running_for_test();

    let frame = render_plain(&mut app);
    let title: String = frame.chars().take(80).collect();
    for hint in ["` step out", "QUIT quit"] {
        assert!(title.contains(hint), "missing {hint:?} in title: {title:?}");
    }
}
