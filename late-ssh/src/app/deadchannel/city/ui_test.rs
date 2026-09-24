use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::deadchannel::fight::state::Sheet;

fn render(state: &State, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            draw(
                frame,
                area,
                CityView {
                    state,
                    own_username: "mira",
                    look: None,
                    sheet: None,
                    scene: None,
                    till: None,
                },
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_runner_and_the_wire_popover_render_at_the_spawn() {
    let state = State::new();
    let screen = render(&state, 100, 30);
    // The camera follows the runner: their name is on screen, and the
    // way up to the wire is within reach.
    assert!(screen.contains("mira"), "name label\n{screen}");
    assert!(screen.contains("the wire"), "the popover title\n{screen}");
    assert!(
        screen.contains("back up the wire"),
        "the popover verb\n{screen}"
    );
}

#[test]
fn a_shop_panel_lists_its_catalog() {
    let mut state = State::new();
    state.open_panel(Landmark::Tailor);
    let screen = render(&state, 120, 40);
    assert!(screen.contains("the tailor"), "{screen}");
    assert!(screen.contains("the rack (starter set, free)"), "{screen}");
}

#[test]
fn the_armorer_prices_the_picked_row_against_the_sheet() {
    let mut state = State::new();
    state.open_panel(Landmark::Armorer);
    // Nothing to trade with until the sheet comes down the wire.
    let screen = render(&state, 120, 40);
    assert!(screen.contains("the armorer"), "{screen}");
    assert!(
        screen.contains("the sheet has not come down the wire yet."),
        "{screen}"
    );

    let mut sheet = Sheet::fresh(uuid::Uuid::nil(), chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap());
    sheet.bits = 300;
    sheet.weapon_tier = 2;
    state.pick_down();
    state.pick_down();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw(
                frame,
                frame.area(),
                CityView {
                    state: &state,
                    own_username: "mira",
                    look: None,
                    sheet: Some(&sheet),
                    scene: None,
                    till: Some("the armorer hands over the box cutter. 225 bits."),
                },
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut screen = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            screen.push_str(buffer[(x, y)].symbol());
        }
        screen.push('\n');
    }
    assert!(screen.contains("on hand 300 bits"), "{screen}");
    assert!(screen.contains("weapon box cutter"), "{screen}");
    assert!(screen.contains("armor street clothes"), "{screen}");
    assert!(screen.contains("▸    3  tire iron"), "the cursor on tier 3\n{screen}");
    assert!(screen.contains("the last broadcast"), "{screen}");
    assert!(screen.contains("10350"), "{screen}");
    // Tier 3 weapon: 585 less 75% of 225. Tier 3 armor off street clothes: 585.
    assert!(screen.contains("[w] tire iron for 417 bits"), "{screen}");
    assert!(screen.contains("[a] padded jacket for 585 bits"), "{screen}");
    assert!(
        screen.contains("the armorer hands over the box cutter. 225 bits."),
        "the till line\n{screen}"
    );
}

#[test]
fn the_street_line_pins_what_the_cart_said() {
    let mut state = State::new();
    state.say(Landmark::Noodles, 0);
    let screen = render(&state, 100, 30);
    assert!(screen.contains("two bowls or none"), "{screen}");
}

#[test]
fn animation_never_paints_over_a_prop() {
    let scene = Scene::build(12_345, map::SPAWN.0, map::SPAWN.1);
    let mut cells = compose_grid(&scene);
    animate(&mut cells, 12_345, &scene);
    for sign in map::SIGNS.iter().chain(map::CART_SIGNS.iter()) {
        for x in sign.zone.x0..=sign.zone.x1 {
            let (ch, _) = cells[usize::from(sign.zone.y0)][usize::from(x)];
            assert_eq!(ch, map::char_at(x, sign.zone.y0), "sign letters stay put");
        }
    }
    let (bx, by) = (map::BOARD.x0, map::BOARD.y0);
    assert_eq!(
        cells[usize::from(by)][usize::from(bx)].0,
        map::char_at(bx, by)
    );
}

#[test]
fn light_falls_on_the_street_and_stops_at_walls() {
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1);
    // The cell next to a street lamp is lit; deep inside a tenement's
    // back rooms, away from every window and door, it is not.
    let lamp = map::LIGHTS
        .iter()
        .find(|l| l.kind == LightKind::Lamp)
        .expect("a lamp");
    assert!(
        luma(scene.light(lamp.x + 1, lamp.y)) > 0.3,
        "beside the lamp"
    );
    // Far down the street from the runner the ground is at the floor:
    // dim, still readable.
    assert!(
        (scene.vis(map::OPEN.0, map::OPEN.1) - SEE_FLOOR).abs() < 0.01,
        "the far end fades"
    );
    assert_eq!(scene.vis(map::SPAWN.0, map::SPAWN.1), 1.0, "here is bright");
}

#[test]
fn camera_centers_small_maps_and_clamps_large_ones() {
    assert_eq!(camera_origin(10, 300, 200), 0);
    assert_eq!(camera_origin(2, 40, 200), 0);
    assert_eq!(camera_origin(100, 40, 200), 80);
    assert_eq!(camera_origin(199, 40, 200), 160);
}

#[test]
fn every_cell_sits_on_the_city_night_not_the_theme_canvas() {
    // The street, the padding around a small map, and an open panel all
    // paint the same fixed background, so a light theme never bleeds in.
    let mut state = State::new();
    state.open_panel(Landmark::Armorer);
    let backend = TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            draw(
                frame,
                area,
                CityView {
                    state: &state,
                    own_username: "mira",
                    look: None,
                    sheet: None,
                    scene: None,
                    till: None,
                },
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            assert_eq!(buffer[(x, y)].bg, night(), "cell ({x}, {y})");
        }
    }
}

#[test]
fn the_car_route_is_open_street_end_to_end() {
    for &(x, y) in car_route() {
        assert!(map::walkable(x, y), "car route blocked at ({x}, {y})");
    }
    // With the car and the train on the street the props still stand.
    let t = 50;
    assert!(car_at(t).is_some(), "the car is out at tick {t}");
    let scene = Scene::build(t, map::SPAWN.0, map::SPAWN.1);
    let mut cells = compose_grid(&scene);
    animate(&mut cells, t, &scene);
    for sign in map::SIGNS.iter().chain(map::CART_SIGNS.iter()) {
        for x in sign.zone.x0..=sign.zone.x1 {
            let (ch, _) = cells[usize::from(sign.zone.y0)][usize::from(x)];
            assert_eq!(ch, map::char_at(x, sign.zone.y0), "sign letters stay put");
        }
    }
    for lamp in map::LIGHTS.iter().filter(|l| l.kind == LightKind::Lamp) {
        assert_eq!(cells[usize::from(lamp.y)][usize::from(lamp.x)].0, '*');
    }
}

#[test]
fn billboards_scroll_street_copy_across_blank_cells() {
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1);
    let mut cells = compose_grid(&scene);
    let mut seen: Vec<String> = Vec::new();
    for t in 0..BILLBOARD_CYCLE {
        animate(&mut cells, t * SLOW, &scene);
        for sign in map::BILLBOARDS.iter() {
            let y = sign.zone.y0;
            let strip: String = (sign.zone.x0..=sign.zone.x1)
                .map(|x| {
                    assert_eq!(
                        map::char_at(x, y),
                        ' ',
                        "billboard cell is blank on the map"
                    );
                    cells[usize::from(y)][usize::from(x)].0
                })
                .collect();
            assert!(strip.starts_with('▌') && strip.ends_with('▐'), "{strip}");
            seen.push(strip);
        }
    }
    assert!(
        seen.iter()
            .any(|s| s.contains("DEAD AIR") || s.contains("NOODLES") || s.contains("VIDS")),
        "a line scrolled by: {seen:?}"
    );
}

#[test]
fn where_the_runner_stands_is_lit_even_with_no_lamp_near() {
    let scene = Scene::build(0, map::OPEN.0, map::OPEN.1);
    assert!(
        luma(scene.light(map::OPEN.0, map::OPEN.1)) > 0.3,
        "the carried light"
    );
}

#[test]
fn looking_over_the_ledge_swaps_the_street_for_the_lower_city() {
    let mut state = State::new();
    state.look_over();
    let screen = render(&state, 100, 30);
    assert!(screen.contains("[Enter] step back"), "{screen}");
    assert!(
        !screen.contains("the wire"),
        "the street's popover is gone: {screen}"
    );
}

#[test]
fn rain_falls_on_the_street_and_the_drop_but_never_in_a_room() {
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1);
    let mut cells = compose_grid(&scene);
    let mut outside = 0;
    for t in 0..8u64 {
        animate(&mut cells, t * SLOW, &scene);
        for y in 1..map::MAP_H - 1 {
            for x in 1..map::MAP_W - 1 {
                let ch = cells[usize::from(y)][usize::from(x)].0;
                let drop = matches!(ch, '\'' | '|') && is_floor(map::char_at(x, y));
                if !drop {
                    continue;
                }
                assert!(
                    inside_map()[index(x, y)].is_none(),
                    "rain in a room at ({x}, {y})"
                );
                outside += 1;
            }
        }
    }
    assert!(outside > 100, "the city rains: {outside}");
}
