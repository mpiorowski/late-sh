use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::guide::state::State as GuideState;
use crate::app::deadchannel::street::state::StreetRunner;
use crate::app::deadchannel::tailor::ui as tailor_ui;

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
                    picker: None,
                    till: None,
                    tailor: tailor_ui::MirrorView {
                        draft: None,
                        word: None,
                        changed: false,
                        saving: false,
                    },
                    guide: &GuideState::new(),
                    own_user_id: Uuid::nil(),
                    street: &StreetView::new(),
                    runner_looks: &HashMap::new(),
                    usernames: &UsernameLookup::new(&HashMap::new(), None),
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
    state.open_panel(Landmark::Board);
    let screen = render(&state, 120, 40);
    assert!(screen.contains("the board"), "{screen}");
    assert!(screen.contains("standing orders"), "{screen}");
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

    let mut sheet = Sheet::fresh(
        uuid::Uuid::nil(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
    );
    sheet.level = 3;
    sheet.bits = 1300;
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
                    picker: None,
                    till: Some("the armorer hands over the box cutter. 506 bits."),
                    tailor: tailor_ui::MirrorView {
                        draft: None,
                        word: None,
                        changed: false,
                        saving: false,
                    },
                    guide: &GuideState::new(),
                    own_user_id: Uuid::nil(),
                    street: &StreetView::new(),
                    runner_looks: &HashMap::new(),
                    usernames: &UsernameLookup::new(&HashMap::new(), None),
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
    assert!(screen.contains("on hand 1300 bits"), "{screen}");
    assert!(screen.contains("weapon box cutter"), "{screen}");
    assert!(screen.contains("armor street clothes"), "{screen}");
    assert!(
        screen.contains("▸    3  tire iron"),
        "the cursor on tier 3\n{screen}"
    );
    assert!(screen.contains("the last broadcast"), "{screen}");
    assert!(screen.contains("23287"), "{screen}");
    // Tier 3 weapon: 1316 less 75% of 506 (379). Tier 3 armor off street clothes: 1316.
    assert!(screen.contains("[w] tire iron for 937 bits"), "{screen}");
    assert!(
        screen.contains("[a] padded jacket for 1316 bits"),
        "{screen}"
    );
    assert!(
        screen.contains("the armorer hands over the box cutter. 506 bits."),
        "the till line\n{screen}"
    );
}

/// The city with `state` over it, `sheet` as the mirror, and `till` as the
/// counter's last word, on a 120 by 40 terminal.
fn render_with_sheet(state: &State, sheet: &Sheet, till: Option<&str>) -> String {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw(
                frame,
                frame.area(),
                CityView {
                    state,
                    own_username: "mira",
                    look: None,
                    sheet: Some(sheet),
                    scene: None,
                    picker: None,
                    till,
                    tailor: tailor_ui::MirrorView {
                        draft: None,
                        word: None,
                        changed: false,
                        saving: false,
                    },
                    guide: &GuideState::new(),
                    own_user_id: Uuid::nil(),
                    street: &StreetView::new(),
                    runner_looks: &HashMap::new(),
                    usernames: &UsernameLookup::new(&HashMap::new(), None),
                },
            )
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
    screen
}

/// Dead Air lists what each glass does at this level, and spells the
/// refusal the row would give ahead of the keys.
#[test]
fn dead_air_prices_the_menu_and_says_why_it_would_not_pour() {
    let mut state = State::new();
    state.open_panel(Landmark::Bar);
    let mut sheet = Sheet::fresh(
        uuid::Uuid::nil(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
    );
    sheet.level = 8;
    sheet.signal = 80;
    sheet.crystals = 2;
    let till = "static on ice. it goes down like a short circuit. +3 attack until the roll.";
    let screen = render_with_sheet(&state, &sheet, Some(till));
    assert!(screen.contains(" dead air "), "{screen}");
    assert!(
        screen.contains("crystals 2      in you nothing"),
        "{screen}"
    );
    assert!(
        screen.contains("[s] static on ice     +3 attack"),
        "{screen}"
    );
    assert!(
        screen.contains("[d] dead air, neat    +3 defense"),
        "{screen}"
    );
    assert!(
        screen.contains("[t] test pattern      +16 signal, and filled"),
        "{screen}"
    );
    assert!(screen.contains(till), "the bartender's last word\n{screen}");

    sheet.crystals = 0;
    let dry = render_with_sheet(&state, &sheet, None);
    assert!(
        dry.contains("a glass is a crystal, and you have none."),
        "{dry}"
    );

    sheet.crystals = 1;
    sheet.drink = Some(crate::app::deadchannel::fight::state::Drink::DeadAirNeat);
    let poured = render_with_sheet(&state, &sheet, None);
    assert!(poured.contains("in you dead air, neat"), "{poured}");
    assert!(
        poured.contains("one glass a day. you still have the dead air, neat in you."),
        "{poured}"
    );
}

/// The blade shop prices the next tier up in each slot in crystals, and
/// says what the wall would ask for it.
#[test]
fn the_blade_cart_prices_the_next_tier_up_in_crystals() {
    let mut state = State::new();
    state.open_panel(Landmark::Blades);
    let mut sheet = Sheet::fresh(
        uuid::Uuid::nil(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
    );
    sheet.weapon_tier = 3;
    sheet.armor_tier = 15;
    sheet.crystals = 3;
    let screen = render_with_sheet(&state, &sheet, None);
    assert!(screen.contains(" the blade shop "), "{screen}");
    assert!(
        screen.contains("crystals 3      weapon tire iron"),
        "{screen}"
    );
    assert!(
        screen.contains("[w] rebar club for 3 crystals   the wall asks"),
        "{screen}"
    );
    assert!(
        screen.contains("[a] nothing is made past the top of the wall"),
        "{screen}"
    );
}

/// Patch prices the gap against the sheet and spells its refusals ahead
/// of the key: a dropped signal is the roll's, a full one buys nothing.
#[test]
fn patch_prices_the_gap_and_says_when_there_is_nothing_to_buy() {
    let mut state = State::new();
    state.open_panel(Landmark::Repairs);
    let render_with = |state: &State, sheet: Option<&Sheet>| {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    frame.area(),
                    CityView {
                        state,
                        own_username: "mira",
                        look: None,
                        sheet,
                        scene: None,
                        picker: None,
                        till: Some("patch works fast. +18 signal, back to full. 27 bits."),
                        tailor: tailor_ui::MirrorView {
                            draft: None,
                            word: None,
                            changed: false,
                            saving: false,
                        },
                        guide: &GuideState::new(),
                        own_user_id: Uuid::nil(),
                        street: &StreetView::new(),
                        runner_looks: &HashMap::new(),
                        usernames: &UsernameLookup::new(&HashMap::new(), None),
                    },
                )
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
        screen
    };

    let screen = render_with(&state, None);
    assert!(screen.contains(" patch "), "{screen}");
    assert!(
        screen.contains("the sheet has not come down the wire yet."),
        "{screen}"
    );

    let mut sheet = Sheet::fresh(
        uuid::Uuid::nil(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
    );
    sheet.level = 3;
    sheet.signal = 12;
    sheet.bits = 100;
    let screen = render_with(&state, Some(&sheet));
    assert!(
        screen.contains("signal 12/30      on hand 100 bits"),
        "{screen}"
    );
    assert!(screen.contains("[p] patch to full for 27 bits"), "{screen}");
    assert!(
        screen.contains("patch works fast. +18 signal, back to full. 27 bits."),
        "{screen}"
    );

    sheet.signal = 30;
    let screen = render_with(&state, Some(&sheet));
    assert!(
        screen.contains("nothing on you needs patching."),
        "{screen}"
    );
    assert!(!screen.contains("[p] patch"), "{screen}");

    sheet.signal = 0;
    let screen = render_with(&state, Some(&sheet));
    assert!(
        screen.contains("your signal is down. nothing here brings it back before the roll."),
        "{screen}"
    );

    // Spent for the day: the roll refills for free, so the key is not
    // offered.
    sheet.signal = 12;
    sheet.rations_left = 0;
    let screen = render_with(&state, Some(&sheet));
    assert!(
        screen.contains("you are spent for today. the roll brings the signal back for nothing."),
        "{screen}"
    );
    assert!(!screen.contains("[p] patch"), "{screen}");
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
    let scene = Scene::build(12_345, map::SPAWN.0, map::SPAWN.1, &[]);
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
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1, &[]);
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
                    picker: None,
                    till: None,
                    tailor: tailor_ui::MirrorView {
                        draft: None,
                        word: None,
                        changed: false,
                        saving: false,
                    },
                    guide: &GuideState::new(),
                    own_user_id: Uuid::nil(),
                    street: &StreetView::new(),
                    runner_looks: &HashMap::new(),
                    usernames: &UsernameLookup::new(&HashMap::new(), None),
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
    let scene = Scene::build(t, map::SPAWN.0, map::SPAWN.1, &[]);
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
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1, &[]);
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
    let scene = Scene::build(0, map::OPEN.0, map::OPEN.1, &[]);
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
    let scene = Scene::build(0, map::SPAWN.0, map::SPAWN.1, &[]);
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

#[test]
fn other_runners_stand_on_the_street_with_their_names_and_you_are_not_twice() {
    let state = State::new();
    let (mira, kade, nox) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    // Open street a few rows up from the stairs, clear of your own label,
    // one spot west of you and one east, far enough apart that no label
    // sits on another.
    let (sx, sy) = map::SPAWN;
    let open = |xs: &mut dyn Iterator<Item = u16>| {
        xs.flat_map(|x| (sy - 6..=sy - 3).map(move |y| (x, y)))
            .find(|&(x, y)| map::walkable(x, y))
            .expect("open street near the stairs")
    };
    let west = open(&mut (sx - 20..=sx - 8).rev());
    let east = open(&mut (sx + 8..=sx + 20));
    let street: StreetView = [
        (kade, west, true),
        (nox, east, false),
        // Your own entry (the street's copy of you) is not drawn: the
        // runner under your keys is.
        (mira, east, true),
    ]
    .into_iter()
    .map(|(user_id, (x, y), present)| (user_id, StreetRunner { x, y, present }))
    .collect();
    let names: HashMap<Uuid, String> = [
        (mira, "mira".to_string()),
        (kade, "kade".to_string()),
        (nox, "nox".to_string()),
    ]
    .into_iter()
    .collect();
    let usernames = UsernameLookup::new(&names, None);

    let backend = TestBackend::new(100, 30);
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
                    sheet: None,
                    scene: None,
                    picker: None,
                    till: None,
                    tailor: tailor_ui::MirrorView {
                        draft: None,
                        word: None,
                        changed: false,
                        saving: false,
                    },
                    guide: &GuideState::new(),
                    own_user_id: mira,
                    street: &street,
                    runner_looks: &HashMap::new(),
                    usernames: &usernames,
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
    assert!(screen.contains("kade"), "a runner who is looking\n{screen}");
    assert!(screen.contains("nox"), "a runner who is away\n{screen}");
    assert_eq!(screen.matches("mira").count(), 1, "you, once\n{screen}");
}

#[test]
fn a_runner_who_is_looking_carries_a_light_and_one_who_is_away_does_not() {
    let (px, py) = map::SPAWN;
    let lonely = Scene::build(0, px, py, &[]);
    let with_company = Scene::build(0, px, py, &[map::OPEN]);
    let (ox, oy) = map::OPEN;
    assert!(
        luma(with_company.light(ox, oy)) > luma(lonely.light(ox, oy)) + 0.3,
        "the other runner's pool of light"
    );

    // Side by side under the same light, the one who is looking is the
    // brighter mark.
    let street: StreetView = [
        (
            Uuid::now_v7(),
            StreetRunner {
                x: px + 1,
                y: py,
                present: true,
            },
        ),
        (
            Uuid::now_v7(),
            StreetRunner {
                x: px + 1,
                y: py,
                present: false,
            },
        ),
    ]
    .into_iter()
    .collect();
    let brightness = |present: bool| {
        let street: StreetView = street
            .iter()
            .filter(|(_, runner)| runner.present == present)
            .map(|(id, runner)| (*id, *runner))
            .collect();
        let state = State::new();
        let guide = GuideState::new();
        let names = HashMap::new();
        let usernames = UsernameLookup::new(&names, None);
        let looks = HashMap::new();
        let view = CityView {
            state: &state,
            own_username: "mira",
            look: None,
            sheet: None,
            scene: None,
            picker: None,
            till: None,
            tailor: tailor_ui::MirrorView {
                draft: None,
                word: None,
                changed: false,
                saving: false,
            },
            guide: &guide,
            own_user_id: Uuid::nil(),
            street: &street,
            runner_looks: &looks,
            usernames: &usernames,
        };
        let mut cells = compose_grid(&lonely);
        draw_others(&mut cells, &lonely, &view);
        let (ch, style) = cells[usize::from(py)][usize::from(px + 1)];
        assert_eq!(ch, '@');
        match style.fg {
            Some(Color::Rgb(r, g, b)) => u32::from(r) + u32::from(g) + u32::from(b),
            other => panic!("the mark has a fixed RGB color, got {other:?}"),
        }
    };
    assert!(brightness(true) > brightness(false));
}
