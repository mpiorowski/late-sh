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
    let mut cells = styled_base_grid();
    animate(&mut cells, 12_345);
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
fn camera_centers_small_maps_and_clamps_large_ones() {
    assert_eq!(camera_origin(10, 300, 200), 0);
    assert_eq!(camera_origin(2, 40, 200), 0);
    assert_eq!(camera_origin(100, 40, 200), 80);
    assert_eq!(camera_origin(199, 40, 200), 160);
}
