use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::draw;
use crate::app::deadchannel::guide::data::SECTIONS;
use crate::app::deadchannel::guide::state::State;

fn render(state: &State, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), state))
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
fn the_guide_shows_every_section_between_the_top_and_the_end() {
    let mut state = State::new();
    state.open();

    // The box holds at most forty rows, so the guide is read by scrolling:
    // every heading and every line shows in some frame between the top
    // and the end. A line wraps to the box, so its opening words are the
    // witness (the box is 84 wide with the frame; forty fits one row).
    let mut seen = String::new();
    let mut last_scroll = None;
    while last_scroll != Some(state.scroll()) {
        last_scroll = Some(state.scroll());
        seen.push_str(&render(&state, 100, 60));
        state.scroll_by(10);
    }
    assert!(seen.contains("the street, explained"));
    for section in SECTIONS {
        assert!(seen.contains(section.title), "missing {}", section.title);
        for line in section.lines {
            let opening: String = line.chars().take(40).collect();
            assert!(seen.contains(&opening), "missing {line:?}");
        }
    }
    assert!(seen.contains("Esc close"));
    state.open();

    // A short frame shows the top first and the end after a long scroll.
    let short = render(&state, 100, 14);
    assert!(short.contains("arrows or hjkl walk"));
    assert!(!short.contains("opens it again."));
    state.scroll_by(100);
    let scrolled = render(&state, 100, 14);
    assert!(scrolled.contains("opens it again."));
    assert!(!scrolled.contains("arrows or hjkl walk"));
}
