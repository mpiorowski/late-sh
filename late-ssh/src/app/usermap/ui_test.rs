use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};

use super::state::{HEAT, UserMapState, heat_floors, heat_index_for, territory_for_code};
use super::ui::draw;

/// Render into a terminal and hand back every colour that reached the buffer.
fn render(state: &mut UserMapState, w: u16, h: u16) -> (String, Vec<Color>) {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
    terminal
        .draw(|frame| {
            let area = frame.area();
            draw(frame, area, state);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    let colors = buffer.content().iter().flat_map(|c| [c.fg, c.bg]).collect();
    (text, colors)
}

/// The map draws before the numbers arrive — a modal that shows nothing until
/// a round trip finishes reads as broken.
#[test]
fn it_draws_while_it_is_still_counting() {
    let mut state = UserMapState::default();
    state.open_without_loading();
    let (text, colors) = render(&mut state, 120, 40);
    assert!(text.contains("where everyone is"), "{text:.200}");
    assert!(text.contains("counting"), "it should say what it is doing");
    // The world is on screen: half-blocks, and sea somewhere.
    assert!(text.contains('▀'));
    let (wr, wg, wb) = crate::app::common::worldmap::view::WATER_COLOR;
    assert!(colors.contains(&Color::Rgb(wr, wg, wb)), "no sea drawn");
}

/// The shading is the whole feature: a country with people in it has to come
/// out a different colour from one without.
#[test]
fn a_country_with_people_in_it_is_shaded() {
    let mut state = UserMapState::default();
    state.open_without_loading();
    state.set_tallies_for_test(&[("PL", 3), ("NZ", 1)]);
    let (_, colors) = render(&mut state, 160, 48);
    let floors = heat_floors(3);
    for (code, count) in [("PL", 3usize), ("NZ", 1)] {
        assert!(
            territory_for_code(code).is_some(),
            "{code} has to be on the map for this test to mean anything"
        );
        let (r, g, b) = HEAT[heat_index_for(&floors, count)];
        assert!(
            colors.contains(&Color::Rgb(r, g, b)),
            "{code}'s rung never reached the screen"
        );
    }
    // And the two counts are on different rungs, so the map distinguishes
    // busy from quiet rather than colouring everything the same.
    assert_ne!(
        HEAT[heat_index_for(&floors, 3)],
        HEAT[heat_index_for(&floors, 1)]
    );
}

/// Everybody online with no country set is the ordinary state of a new
/// server, and it has to say so rather than showing an empty world with no
/// explanation.
#[test]
fn an_empty_tally_explains_itself() {
    let mut state = UserMapState::default();
    state.open_without_loading();
    state.set_tallies_for_test(&[]);
    let (text, _) = render(&mut state, 120, 40);
    assert!(text.contains("nobody online has said"), "{text:.400}");
}

/// A narrow terminal drops the list rather than the map, and neither draws
/// past its box.
#[test]
fn it_survives_a_small_terminal() {
    let mut state = UserMapState::default();
    state.open_without_loading();
    state.set_tallies_for_test(&[("PL", 2)]);
    for (w, h) in [(40u16, 12u16), (80, 24), (200, 60)] {
        let (text, _) = render(&mut state, w, h);
        assert!(!text.is_empty());
    }
}

/// A wheel, a drag and a click all have to land somewhere, and only the draw
/// knows where anything is — so the geometry it records is what the mouse
/// reads. These go through `render` first for exactly that reason.
#[test]
fn the_mouse_zooms_drags_and_picks() {
    use super::state::MousePress;

    let mut state = UserMapState::default();
    state.open_without_loading();
    state.set_tallies_for_test(&[("PL", 3), ("NZ", 1)]);
    render(&mut state, 160, 48);

    // Wheel over the map zooms; the map is what the pointer is over.
    let before = state.view().expect("a view once drawn").scale;
    assert!(state.handle_mouse(MousePress::ScrollUp, 20, 10));
    assert!(
        state.view().expect("view").scale < before,
        "the wheel should zoom in"
    );
    assert!(state.handle_mouse(MousePress::ScrollDown, 20, 10));

    // Zoom in far enough that there is somewhere to drag to.
    for _ in 0..8 {
        state.handle_mouse(MousePress::ScrollUp, 20, 10);
    }
    render(&mut state, 160, 48);
    let anchored = state.view().expect("view");
    assert!(state.handle_mouse(MousePress::Down, 40, 20));
    assert!(state.handle_mouse(MousePress::Drag, 30, 20));
    let dragged = state.view().expect("view");
    assert!(
        dragged.vx > anchored.vx,
        "dragging left pulls the world left, so the view moves east"
    );
    // The release after a drag is not a pick: the selection stays put.
    let selected = state.cursor();
    assert!(state.handle_mouse(MousePress::Up, 30, 20));
    assert_eq!(state.cursor(), selected);
}

/// Clicking a country in the list picks it, the same as walking there with
/// j/k — including framing it on the map.
#[test]
fn a_click_in_the_list_picks_that_country() {
    use super::state::MousePress;

    let mut state = UserMapState::default();
    state.open_without_loading();
    state.set_tallies_for_test(&[("PL", 3), ("NZ", 1), ("JP", 1)]);
    render(&mut state, 160, 48);
    assert_eq!(state.cursor(), 0);

    // Ask the draw where the list ended up rather than recomputing the
    // modal's insets here — that arithmetic belongs in one place, and a test
    // that duplicates it tests itself.
    let list = state.list_area_for_test().expect("a list was drawn");
    // Mouse coordinates are 1-based, which is what the handler undoes.
    let second_row_y = list.y + 1 + 1;
    assert!(state.handle_mouse(MousePress::Down, list.x + 2, second_row_y));
    assert_eq!(state.cursor(), 1, "the second row is the second country");
    assert_eq!(
        state.selected(),
        territory_for_code("NZ"),
        "and the map rings it"
    );
}

/// A press outside both boxes is not ours, so the modal does not swallow it.
#[test]
fn a_press_outside_the_boxes_is_not_taken() {
    use super::state::MousePress;

    let mut state = UserMapState::default();
    state.open_without_loading();
    state.set_tallies_for_test(&[("PL", 1)]);
    render(&mut state, 160, 48);
    assert!(!state.handle_mouse(MousePress::Down, 1, 1));
}

/// The three slices have to be discoverable and distinguishable: the title
/// says which one you are looking at, and the tab strip says the others
/// exist.
#[test]
fn every_mode_names_itself_and_shows_the_others() {
    use super::state::MapMode;

    for mode in MapMode::ALL {
        let mut state = UserMapState::default();
        state.open_without_loading();
        state.set_mode_for_test(mode);
        state.set_tallies_for_test(&[("PL", 3), ("NZ", 1)]);
        let (text, _) = render(&mut state, 160, 48);
        assert!(
            text.contains(mode.title()),
            "{mode:?} should name itself: {text:.300}"
        );
        for other in MapMode::ALL {
            assert!(
                text.contains(other.tab_label()),
                "{mode:?} should still offer {other:?}"
            );
        }
        assert!(text.contains("4 people"), "the headline counts the slice");
    }
}

/// The legend describes the data in front of it, not a fixed ladder. Four
/// people on the server and four thousand are both legible.
#[test]
fn the_legend_follows_the_numbers() {
    let mut small = UserMapState::default();
    small.open_without_loading();
    small.set_tallies_for_test(&[("PL", 2), ("NZ", 1)]);
    let (text, _) = render(&mut small, 160, 48);
    assert!(
        text.contains("2+"),
        "a two-person peak tops out at 2: {text:.400}"
    );
    assert!(
        !text.contains("16+"),
        "no rungs nobody is standing on: {text:.400}"
    );

    let mut big = UserMapState::default();
    big.open_without_loading();
    big.set_tallies_for_test(&[("PL", 900), ("NZ", 40), ("JP", 3)]);
    let (text, _) = render(&mut big, 160, 48);
    assert!(
        text.contains("500+"),
        "a 900-person peak needs a rung up there"
    );
}
