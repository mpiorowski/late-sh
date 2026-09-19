use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render(width: u16, height: u16, t: u64) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, frame.area(), t)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn the_ledge_fills_the_screen_with_the_lower_city_and_the_way_back() {
    let buffer = render(120, 40, 77);
    let text: String = (0..40)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(text.contains("the drop"), "{text}");
    assert!(text.contains("[Enter] step back"), "{text}");
    // Something stands below the horizon: the picture is not all sky.
    let colors: std::collections::HashSet<String> = (0..40)
        .flat_map(|y| (0..120).map(move |x| (x, y)))
        .map(|(x, y)| format!("{:?}", buffer[(x, y)].bg))
        .collect();
    assert!(colors.len() > 8, "a picture, not a wash: {}", colors.len());
}

#[test]
fn the_picture_is_the_same_for_the_same_size_and_tick() {
    let a = render(80, 24, 5);
    let b = render(80, 24, 5);
    assert_eq!(a, b);
}

#[test]
fn a_tiny_terminal_still_draws() {
    let buffer = render(6, 4, 0);
    assert_eq!(buffer.area.width, 6);
}
