use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, backend::TestBackend};

use super::state::{HEAT, UserMapState, heat_floors, heat_index_for, territory_for_code};
use super::ui::{draw, hints};
use crate::app::common::primitives::hint_line;

/// Render into a terminal and hand back every colour that reached the buffer.
/// Laid out as the page lays it out: the map gets every row but the last,
/// which the page fills from the map's footer hints.
fn render(state: &mut UserMapState, w: u16, h: u16) -> (String, Vec<Color>) {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
    terminal
        .draw(|frame| {
            let area = frame.area();
            let body = Rect::new(area.x, area.y, area.width, area.height - 1);
            draw(frame, body, state);
            frame.render_widget(
                Paragraph::new(hint_line(hints(state))),
                Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    let colors = buffer.content().iter().flat_map(|c| [c.fg, c.bg]).collect();
    (text, colors)
}

/// The map draws before the numbers arrive — a shelf that shows nothing until
/// a round trip finishes reads as broken.
#[test]
fn it_draws_while_it_is_still_counting() {
    let mut state = UserMapState::default();
    state.show_without_loading();
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
    state.show_without_loading();
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
    state.show_without_loading();
    state.set_tallies_for_test(&[]);
    let (text, _) = render(&mut state, 120, 40);
    assert!(text.contains("nobody online has said"), "{text:.400}");
}

/// The online slice is everyone connected minus those with no country, so
/// its headline says how many of the connected it shows; otherwise it reads
/// as a different "online" number from the sidebar's.
#[test]
fn the_online_headline_says_how_many_set_a_country() {
    let mut state = UserMapState::default();
    state.show_without_loading();
    state.set_tallies_for_test(&[("PL", 14), ("NZ", 1)]);
    state.set_online_for_test(Some(17));
    let (text, _) = render(&mut state, 160, 48);
    assert!(
        text.contains("online now · 15 of 17 have set a country"),
        "{text:.300}"
    );

    state.set_tallies_for_test(&[("PL", 1)]);
    let (text, _) = render(&mut state, 160, 48);
    assert!(
        text.contains("online now · 1 of 17 has set a country"),
        "{text:.300}"
    );
}

/// A narrow terminal drops the list rather than the map, and neither draws
/// past its box.
#[test]
fn it_survives_a_small_terminal() {
    let mut state = UserMapState::default();
    state.show_without_loading();
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
    state.show_without_loading();
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
    state.show_without_loading();
    state.set_tallies_for_test(&[("PL", 3), ("NZ", 1), ("JP", 1)]);
    render(&mut state, 160, 48);
    assert_eq!(state.cursor(), 0);

    // Ask the draw where the list ended up rather than recomputing the
    // layout here: that arithmetic belongs in one place, and a test that
    // duplicates it tests itself.
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

/// Online, each country lists who is in it, right under its row.
#[test]
fn online_countries_name_their_people() {
    let mut state = UserMapState::default();
    state.show_without_loading();
    state.set_people_for_test(&[("US", &["cws", "Renu"]), ("PL", &["mat"])]);
    let (text, _) = render(&mut state, 160, 48);
    let list = state.list_area_for_test().expect("a list was drawn");
    let row = |y: u16| -> String {
        (list.x..list.x + list.width)
            .map(|x| text.chars().nth((y * 160 + x) as usize).unwrap_or(' '))
            .collect()
    };
    let rows: Vec<String> = (list.y..list.y + 5).map(row).collect();
    assert!(rows[0].contains("United States"), "{rows:?}");
    assert!(rows[1].contains("@cws"), "{rows:?}");
    assert!(rows[2].contains("@Renu"), "{rows:?}");
    assert!(rows[3].contains("Poland"), "{rows:?}");
    assert!(rows[4].contains("@mat"), "{rows:?}");
}

/// A country with more people than the list has rows is still read to the
/// end: `j` reveals the block a line at a time before it moves on to the
/// next country, and the wheel scrolls lines without moving the cursor.
#[test]
fn a_country_taller_than_the_list_can_be_read_to_the_end() {
    use super::state::MousePress;

    let names: Vec<String> = (0..60).map(|n| format!("p{n:02}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut state = UserMapState::default();
    state.show_without_loading();
    state.set_people_for_test(&[("US", &names), ("PL", &["mat"])]);
    let list_text = |state: &mut UserMapState| -> String {
        let (text, _) = render(state, 160, 48);
        let list = state.list_area_for_test().expect("a list was drawn");
        (list.y..list.y + list.height)
            .map(|y| -> String {
                (list.x..list.x + list.width)
                    .map(|x| text.chars().nth((y * 160 + x) as usize).unwrap_or(' '))
                    .collect()
            })
            .collect::<Vec<String>>()
            .join("\n")
    };
    assert!(
        !list_text(&mut state).contains("@p59"),
        "the list is shorter than the block"
    );

    // `j` walks the block to its last name before it leaves the country.
    let mut steps = 0;
    while !list_text(&mut state).contains("@p59") {
        state.walk(1);
        steps += 1;
        assert_eq!(
            state.cursor(),
            0,
            "still on the United States after {steps} steps"
        );
        assert!(steps < 100, "the block never scrolled to its end");
    }
    assert!(steps > 0);
    state.walk(1);
    assert_eq!(
        state.cursor(),
        1,
        "the step after the last name is the next country"
    );
    assert!(list_text(&mut state).contains("Poland"));

    // The wheel over the list scrolls a line and leaves the cursor alone.
    let mut state = UserMapState::default();
    state.show_without_loading();
    state.set_people_for_test(&[("US", &names), ("PL", &["mat"])]);
    render(&mut state, 160, 48);
    let list = state.list_area_for_test().expect("a list was drawn");
    assert!(state.handle_mouse(MousePress::ScrollDown, list.x + 2, list.y + 2));
    assert_eq!(state.first_visible_line(), 1);
    assert_eq!(state.cursor(), 0);
    let text = list_text(&mut state);
    assert!(text.starts_with("     @p00"), "{text:.60}");
}

/// A click on a person, or on the country after them, lands on the right
/// country: the names push every country below them down the list.
#[test]
fn a_click_below_named_people_picks_the_right_country() {
    use super::state::MousePress;

    let mut state = UserMapState::default();
    state.show_without_loading();
    state.set_people_for_test(&[("US", &["cws", "Renu"]), ("PL", &["mat"])]);
    render(&mut state, 160, 48);
    let list = state.list_area_for_test().expect("a list was drawn");
    // Mouse coordinates are 1-based, which is what the handler undoes.
    let line_y = |line: u16| list.y + line + 1;

    assert!(state.handle_mouse(MousePress::Down, list.x + 2, line_y(3)));
    assert_eq!(
        state.selected(),
        territory_for_code("PL"),
        "the fourth line is Poland"
    );

    render(&mut state, 160, 48);
    assert!(state.handle_mouse(MousePress::Down, list.x + 2, line_y(2)));
    assert_eq!(
        state.selected(),
        territory_for_code("US"),
        "a name belongs to the country above it"
    );
}

/// A press outside both boxes is not ours, so the modal does not swallow it.
#[test]
fn a_press_outside_the_boxes_is_not_taken() {
    use super::state::MousePress;

    let mut state = UserMapState::default();
    state.show_without_loading();
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
        state.show_without_loading();
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
    small.show_without_loading();
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
    big.show_without_loading();
    big.set_tallies_for_test(&[("PL", 900), ("NZ", 40), ("JP", 3)]);
    let (text, _) = render(&mut big, 160, 48);
    assert!(
        text.contains("500+"),
        "a 900-person peak needs a rung up there"
    );
}
