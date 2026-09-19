use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

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
    state.open_panel(Landmark::Armorer);
    let screen = render(&state, 120, 40);
    assert!(screen.contains("the armorer"), "{screen}");
    assert!(screen.contains("bent antenna"), "{screen}");
    assert!(screen.contains("the last broadcast"), "{screen}");
    assert!(screen.contains("10350"), "{screen}");
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
fn billboards_write_the_citys_script_on_blank_cells() {
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1);
    let mut cells = compose_grid(&scene);
    let mut written = 0;
    for t in 0..(BILLBOARD_CYCLE * 2) {
        animate(&mut cells, t * SLOW, &scene);
        for sign in map::BILLBOARDS.iter() {
            for y in sign.zone.y0..=sign.zone.y1 {
                for x in sign.zone.x0..=sign.zone.x1 {
                    assert_eq!(
                        map::char_at(x, y),
                        ' ',
                        "billboard cell is blank on the map"
                    );
                    if GLYPH_ALPHABET.contains(&cells[usize::from(y)][usize::from(x)].0) {
                        written += 1;
                    }
                }
            }
        }
    }
    assert!(written > 0, "the boards showed a text");
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
