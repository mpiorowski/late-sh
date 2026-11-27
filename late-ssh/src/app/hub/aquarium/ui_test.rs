use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn tank_with(creatures: &[(&str, usize)]) -> AquariumState {
    let mut app = AquariumState::default_for_area(Rect::new(0, 0, 60, 20)).expect("default reef");
    let active: Vec<(String, usize)> = creatures
        .iter()
        .map(|(name, count)| (name.to_string(), *count))
        .collect();
    app.set_active_creatures(&active, None, false);
    app
}

fn mini_rows(app: &AquariumState, hungry: bool, tick: usize) -> Vec<String> {
    mini_rows_tall(app, hungry, tick, MINI_TANK_HEIGHT)
}

fn mini_rows_tall(app: &AquariumState, hungry: bool, tick: usize, height: u16) -> Vec<String> {
    let area = Rect::new(0, 0, 21, height);
    let mut buf = Buffer::empty(area);
    draw_mini_tank(&mut buf, area, app, hungry, tick);
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

#[test]
fn mini_tank_lays_swimmers_on_their_rows_and_plants_on_the_floor() {
    let app = tank_with(&[("anchovy", 1), ("boxfish", 1), ("seatuft", 1)]);
    assert_eq!(
        mini_rows(&app, false, 0),
        vec![
            "~~~^--^^~~~^--^^~~~^-",
            "-'>                  ",
            "     )o>             ",
            "                     ",
            r"   \|/               ",
            "._.-^-.__-._._.-^-.__",
        ]
    );
}

/// A tank tile taller than the small band fills its height: the
/// surface on the top row, the floor on the bottom one, and the four
/// lanes spread over the water between, so a ten-row tank swims on rows
/// one, three, five and seven.
#[test]
fn a_taller_tank_spreads_the_lanes_over_its_water() {
    let app = tank_with(&[("anchovy", 2), ("boxfish", 2)]);
    let rows = mini_rows_tall(&app, false, 0, 10);
    assert_eq!(rows[0], "~~~^--^^~~~^--^^~~~^-");
    assert_eq!(rows[9], "._.-^-.__-._._.-^-.__");
    for row in [1, 3, 5, 7] {
        assert!(!rows[row].trim().is_empty(), "lane on row {row}: {rows:?}");
    }
    for row in [2, 4, 6, 8] {
        assert!(
            rows[row].trim().is_empty(),
            "no lane on row {row}: {rows:?}"
        );
    }
    // Hungry, every fish rests on the last water row.
    let rows = mini_rows_tall(&app, true, 0, 10);
    assert!(!rows[8].trim().is_empty(), "{rows:?}");
    assert!(
        rows[1..8].iter().all(|row| row.trim().is_empty()),
        "{rows:?}"
    );
}

#[test]
fn mini_tank_swimmers_turn_at_the_wall() {
    let app = tank_with(&[("anchovy", 1)]);
    // Travel is 18 cells at 4 ticks a cell: one step short of the far wall
    // at tick 68, turned around on it at tick 72.
    assert_eq!(mini_rows(&app, false, 68)[1], "                 -'> ");
    assert_eq!(mini_rows(&app, false, 72)[1], "                  <'-");
}

#[test]
fn mini_tank_hungry_fish_rest_on_the_bottom_row() {
    let app = tank_with(&[("anchovy", 1), ("boxfish", 1)]);
    let rows = mini_rows(&app, true, 0);
    assert_eq!(rows[1].trim(), "");
    assert_eq!(rows[2].trim(), "");
    assert_eq!(rows[4], "-'>  )o>             ");
}

#[test]
fn mini_tank_draws_at_most_two_of_a_kind() {
    let app = tank_with(&[("mj", 10)]);
    let glyphs: usize = mini_rows(&app, false, 0)
        .iter()
        .map(|row| row.matches('ଳ').count())
        .sum();
    assert_eq!(glyphs, 2);
}

#[test]
fn mini_tank_surface_keeps_rolling_past_the_u16_wall_tick() {
    // The wave shifts by half the wall tick: 65_535 here, about 2h24m into
    // a session, where a u16 sum with the column would overflow.
    let app = tank_with(&[]);
    assert_eq!(mini_rows(&app, false, 131_070)[0], "^~~~^--^^~~~^--^^~~~^");
}

fn care_glyphs(bar: CareBar) -> String {
    care_bar_spans(bar)
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

#[test]
fn the_care_bar_is_always_fourteen_dots() {
    // Five fed days: five full, nine empty.
    assert_eq!(care_glyphs(CareBar::Streak(5)), "●●●●●○○○○○○○○○");
    // Two unfed days: two full (red), the rest empty.
    assert_eq!(care_glyphs(CareBar::Dry(2)), "●●○○○○○○○○○○○○");
    // A minded tank shows nothing filled.
    assert_eq!(care_glyphs(CareBar::Minded), "○".repeat(CARE_DAYS as usize));
    // Nothing past fourteen ever widens the title.
    assert_eq!(
        care_glyphs(CareBar::Dry(40)),
        "●".repeat(CARE_DAYS as usize)
    );
}

/// Everyone has a tank, so neither of its tiles points anyone at the shop,
/// even before a fish is in the water: both draw the water and its care bar.
#[test]
fn the_tank_tiles_never_send_anyone_to_the_shop() {
    let area = Rect::new(0, 0, 60, 16);
    let state = AquariumState::default_for_area(area).expect("aquarium");
    let care = AquariumCare::new(None, Vec::new());
    let view = TankView {
        state: &state,
        care: &care,
    };
    for reef in [true, false] {
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
        terminal
            .draw(|frame| match reef {
                true => draw_reef_tile(frame, area, &view),
                false => draw_tank_tile(frame, area, &view, 0),
            })
            .expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!text.contains("/shop"), "reef {reef}: {text}");
        assert!(!text.contains("no tank"), "reef {reef}: {text}");
    }
}

/// The full view is the reef bound to the water inside its border, the
/// care bar on the surface, and under the water the day's word and the
/// keys that answer.
#[test]
fn the_full_view_draws_the_reef_over_its_footer() {
    let area = Rect::new(0, 0, 90, 26);
    let mut state = tank_with(&[("anchovy", 2), ("boxfish", 2), ("seatuft", 1)]);
    for _ in 0..8 {
        state.tick();
    }
    let water = full_view_water(area);
    assert_eq!(
        water,
        Rect::new(1, 1, 88, 23),
        "inside the border, over the footer"
    );
    state.handle_resize(water.width, water.height);
    let care = AquariumCare::new(None, Vec::new());
    let today = chrono::Utc::now().date_naive();
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
    terminal
        .draw(|frame| {
            draw_full_view(
                frame,
                area,
                &TankView {
                    state: &state,
                    care: &care,
                },
                today,
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect();
    let plain = rows.join("\n");
    assert!(rows[0].starts_with("┌ reef "), "{plain}");
    assert!(
        rows[1].contains("○○○○○○○○○○○○○○"),
        "the bar on the surface:\n{plain}"
    );
    assert!(
        rows[24].contains("not fed today · f feed · s more fish · q close"),
        "the footer under the water:\n{plain}"
    );
    assert!(rows[25].starts_with('└'), "{plain}");
}

/// A reef or tank tile names Home's key for the full view beside its care
/// bar when the surface has room for both, and keeps the bar alone when
/// it has not.
#[test]
fn a_tile_names_its_key_beside_the_care_bar_when_it_fits() {
    let state = AquariumState::default_for_area(Rect::new(0, 0, 60, 16)).expect("aquarium");
    let care = AquariumCare::new(None, Vec::new());
    let view = TankView {
        state: &state,
        care: &care,
    };
    for (width, named) in [(60, true), (22, false)] {
        let area = Rect::new(0, 0, width, 16);
        let mut terminal = Terminal::new(TestBackend::new(width, 16)).expect("terminal");
        terminal
            .draw(|frame| draw_tank_tile(frame, area, &view, 0))
            .expect("draw");
        let surface: String = (0..width)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
            .collect();
        assert!(surface.contains("○○○○○○○○○○○○○○"), "{width}: {surface}");
        // The bar, then the key, a blank cell parting each end from the
        // water.
        assert_eq!(
            surface.contains(" ○○○○○○○○○○○○○○ a reef "),
            named,
            "{width} wide: {surface}"
        );
    }
}
