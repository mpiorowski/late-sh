use super::clock_on_screen;
use crate::{
    app::{
        common::primitives::Screen,
        state::App,
        zen::state::{Dir, Guest, Look, Node, RiceLayout, TileKind, ZenState},
    },
    test_helpers::{make_app, new_test_db},
};
use late_core::{
    models::user::Screensaver,
    test_utils::{TestDb, create_test_user},
};

#[tokio::test]
async fn the_glitch_runs_on_a_home_showing_the_presence_clock_and_nowhere_else() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "haunt-clock-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "haunt-clock-token");
    app.skip_splash_for_tests();

    // The stock Home carries the clock and animated tiles that repaint it.
    app.set_screen(Screen::Dashboard);
    assert!(clock_on_screen(&app));

    // No other page draws a clock for a burst to land on.
    app.set_screen(Screen::Arcade);
    assert!(!clock_on_screen(&app));

    // A layout without the presence tile has nowhere to spend one either.
    app.set_screen(Screen::Dashboard);
    let no_clock = RiceLayout {
        root: Node::split_first(
            Dir::Row,
            23,
            Node::leaf(TileKind::Rooms),
            Node::leaf(TileKind::Chat),
        ),
        look: Look::default(),
    };
    app.home = ZenState::new(no_clock.clone(), Some(no_clock));
    assert!(!clock_on_screen(&app));
}

/// The stock Home on an ordinary terminal, where a burst would land: each
/// case below starts from this, so its own condition is what defers the burst.
async fn app_showing_the_clock(name: &str) -> (TestDb, App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let mut app = make_app(test_db.db.clone(), user.id, name);
    app.skip_splash_for_tests();
    app.set_screen(Screen::Dashboard);
    assert!(clock_on_screen(&app));
    (test_db, app)
}

#[tokio::test]
async fn a_burst_waits_for_an_away_session_to_come_back() {
    let (_db, mut app) = app_showing_the_clock("haunt-clock-away").await;
    app.sent_away = true;
    assert!(app.sync_away());
    assert!(app.screensaver().is_some(), "the default piece covers Home");
    assert!(!clock_on_screen(&app));

    // With the Tweak off the clock still draws, with nobody there to see it.
    app.profile_state.profile.screensaver = Screensaver::Off;
    assert!(app.screensaver().is_none());
    assert!(!clock_on_screen(&app));

    app.sent_away = false;
    assert!(app.sync_away());
    assert!(clock_on_screen(&app));
}

/// A hosted guest takes the whole tile while it has something to say.
#[tokio::test]
async fn a_burst_skips_a_presence_tile_hosting_a_guest() {
    let (_db, mut app) = app_showing_the_clock("haunt-clock-guest").await;
    let presence = app
        .home
        .rice
        .root
        .leaf_kinds()
        .iter()
        .position(|kind| *kind == TileKind::Presence)
        .expect("the stock Home has a presence tile");
    assert!(app.home.rice.root.toggle_guest(presence, Guest::Attention));
    assert!(!clock_on_screen(&app));
}

#[tokio::test]
async fn a_burst_waits_for_a_terminal_big_enough_for_home() {
    let (_db, mut app) = app_showing_the_clock("haunt-clock-small").await;
    app.resize(30, 10).expect("resize test terminal");
    assert!(
        !clock_on_screen(&app),
        "Home draws only the too-small notice"
    );
}

#[tokio::test]
async fn a_burst_waits_for_a_modal_to_close() {
    let (_db, mut app) = app_showing_the_clock("haunt-clock-modal").await;
    app.show_settings = true;
    assert!(!clock_on_screen(&app));
    app.show_settings = false;
    assert!(clock_on_screen(&app));

    app.show_lobby_modal = true;
    assert!(!clock_on_screen(&app));
    app.show_lobby_modal = false;

    app.show_sheet_modal = true;
    assert!(!clock_on_screen(&app));
    app.show_sheet_modal = false;
    assert!(clock_on_screen(&app));
}
