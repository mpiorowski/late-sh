use super::clock_on_screen;
use crate::{
    app::{
        common::primitives::Screen,
        zen::state::{Dir, Look, Node, RiceLayout, TileKind, ZenState},
    },
    test_helpers::{make_app, new_test_db},
};
use late_core::test_utils::create_test_user;

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
