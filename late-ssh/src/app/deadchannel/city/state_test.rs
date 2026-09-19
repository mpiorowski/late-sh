use super::*;

#[test]
fn walking_stops_at_the_railing_and_moves_on_the_street() {
    let mut state = State::new();
    let (x, y) = (state.player_x, state.player_y);
    // The stairwell to the wire hangs below the spawn: solid.
    assert!(!state.walk(0, 1));
    assert_eq!((state.player_x, state.player_y), (x, y));
    // North is the street.
    assert!(state.walk(0, -1));
    assert_eq!((state.player_x, state.player_y), (x, y - 1));
}

#[test]
fn spawn_stands_at_the_wire_and_the_street_has_no_landmark() {
    let mut state = State::new();
    assert_eq!(state.nearby(), Some(Landmark::Wire));
    state.player_x = map::OPEN.0;
    state.player_y = map::OPEN.1;
    assert_eq!(state.nearby(), None);
}

#[test]
fn panels_open_and_close() {
    let mut state = State::new();
    assert_eq!(state.panel(), None);
    state.open_panel(Landmark::Armorer);
    assert_eq!(state.panel(), Some(Landmark::Armorer));
    state.close_panel();
    assert_eq!(state.panel(), None);
}

#[test]
fn a_street_line_expires_on_its_own() {
    let mut state = State::new();
    state.tick(100);
    state.say(Landmark::Noodles, 1);
    assert_eq!(state.line(), Some((Landmark::Noodles, 1)));
    state.tick(100 + LINE_TICKS - 1);
    assert_eq!(state.line(), Some((Landmark::Noodles, 1)));
    state.tick(100 + LINE_TICKS);
    assert_eq!(state.line(), None);
}

#[test]
fn enter_routes_shops_to_panels_and_the_wire_out() {
    assert_eq!(
        Landmark::Armorer.on_enter(),
        Enter::Panel(Landmark::Armorer)
    );
    assert_eq!(Landmark::Noodles.on_enter(), Enter::Line(Landmark::Noodles));
    assert_eq!(Landmark::Wire.on_enter(), Enter::Leave);
}
