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
fn a_run_covers_open_street_and_stops_at_a_wall() {
    let mut state = State::new();
    state.player_x = map::OPEN.0;
    state.player_y = map::OPEN.1;
    // Open street ahead: the full run.
    let steps = state.run(1, 0);
    assert_eq!(steps, RUN_STEPS);
    assert_eq!(state.player_x, map::OPEN.0 + RUN_STEPS);
    // Into a wall: no step, no move.
    state.player_x = map::OPEN.0;
    state.player_y = map::OPEN.1;
    let mut blocked = 0;
    while state.walk(0, -1) {
        blocked += 1;
        assert!(blocked < 50, "never hit a wall");
    }
    let (x, y) = (state.player_x, state.player_y);
    assert_eq!(state.run(0, -1), 0);
    assert_eq!((state.player_x, state.player_y), (x, y));
}

#[test]
fn panels_open_and_close() {
    let mut state = State::new();
    assert_eq!(state.panel(), None);
    state.open_panel(Landmark::Armorer);
    assert_eq!(state.panel(), Some(Landmark::Armorer));
    state.dismiss();
    assert_eq!(state.panel(), None);
}

#[test]
fn the_armorer_cursor_walks_the_wall_and_holds_at_the_ends() {
    let mut state = State::new();
    assert_eq!(state.picked_tier(), 1);
    state.pick_up();
    assert_eq!(state.picked_tier(), 1, "the top holds");
    for _ in 0..40 {
        state.pick_down();
    }
    assert_eq!(state.picked_tier(), 15, "the bottom holds");
    state.pick_up();
    assert_eq!(state.picked_tier(), 14);
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

#[test]
fn a_run_along_the_railing_does_not_stop_at_every_step() {
    let mut state = State::new();
    // One row north of the wire stairs and a few cells west, clear of
    // the wire's reach, at the railing; running west stays at the railing
    // the whole way.
    state.player_x = map::SPAWN.0 - 5;
    state.player_y = map::RAIL_Y - 1;
    assert_eq!(state.nearby(), Some(Landmark::Ledge));
    assert_eq!(state.run(-1, 0), RUN_STEPS);
    assert_eq!(state.player_x, map::SPAWN.0 - 5 - RUN_STEPS);
    assert_eq!(state.nearby(), Some(Landmark::Ledge));
}

#[test]
fn enter_at_the_railing_looks_over_the_ledge_and_enter_again_steps_back() {
    let mut state = State::new();
    // One row north of the wire stairs: at the railing, not on the wire.
    state.player_y = map::RAIL_Y - 1;
    assert_eq!(state.nearby(), Some(Landmark::Ledge));
    assert_eq!(Landmark::Ledge.on_enter(), Enter::Ledge);
    assert!(!state.at_ledge());
    state.look_over();
    assert!(state.at_ledge());
    state.dismiss();
    assert!(!state.at_ledge());
}
