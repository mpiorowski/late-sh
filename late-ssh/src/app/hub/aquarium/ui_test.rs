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
    let area = Rect::new(0, 0, 21, MINI_TANK_HEIGHT);
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
