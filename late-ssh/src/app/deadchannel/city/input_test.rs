use late_core::test_utils::create_test_user;

use super::handle_event;
use crate::app::common::primitives::Screen;
use crate::app::deadchannel::city::map::Landmark;
use crate::app::input::ParsedInput;
use crate::test_helpers::{make_app, new_test_db};

#[tokio::test]
async fn r_steps_up_to_the_ring_from_the_street_and_nowhere_else() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "city-ring-key-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "city-ring-key-token");
    app.skip_splash_for_tests();
    app.set_screen(Screen::City);

    // On the bare street the key opens the ring's panel, wherever the
    // runner stands, and the runner has not moved.
    let stood = (app.city.player_x, app.city.player_y);
    assert_ne!(app.city.nearby(), Some(Landmark::Ring), "not at the sign");
    assert!(handle_event(&mut app, &ParsedInput::Char('r')));
    assert_eq!(app.city.panel(), Some(Landmark::Ring));
    assert_eq!((app.city.player_x, app.city.player_y), stood);

    // Over another shop's counter it is that panel's key, or nothing.
    app.city.dismiss();
    app.city.open_panel(Landmark::Board);
    assert!(handle_event(&mut app, &ParsedInput::Char('r')));
    assert_eq!(app.city.panel(), Some(Landmark::Board));
}
