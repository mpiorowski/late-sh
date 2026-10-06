use ratatui::{
    Terminal,
    backend::TestBackend,
    text::Line,
    widgets::{Paragraph, Wrap},
};

use super::render_scrolled;

/// Draw `paragraph` into a 20-column, `height`-row terminal scrolled by
/// `scroll`, and return the reported scroll range plus the top visible row.
fn draw(paragraph: Paragraph<'static>, height: u16, scroll: u16) -> (u16, String) {
    let mut terminal = Terminal::new(TestBackend::new(20, height)).expect("terminal");
    let mut max_scroll = 0;
    terminal
        .draw(|frame| {
            max_scroll = render_scrolled(frame, frame.area(), paragraph, scroll);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let top: String = (0..buffer.area.width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect();
    (max_scroll, top.trim_end().to_string())
}

fn rows(count: usize) -> Paragraph<'static> {
    Paragraph::new(
        (0..count)
            .map(|i| Line::from(format!("row {i}")))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn a_landing_that_fits_does_not_scroll() {
    assert_eq!(draw(rows(5), 10, 3), (0, "row 0".to_string()));
}

#[test]
fn scrolling_moves_the_top_row_and_stops_at_the_last_page() {
    assert_eq!(draw(rows(30), 10, 4), (20, "row 4".to_string()));
    assert_eq!(draw(rows(30), 10, 99), (20, "row 20".to_string()));
}

/// Wrapped rows count: four 45-character lines are twelve rows at width 20.
#[test]
fn wrapped_lines_count_toward_the_range() {
    let long = "x".repeat(45);
    let paragraph = Paragraph::new(vec![Line::from(long); 4]).wrap(Wrap { trim: false });
    assert_eq!(draw(paragraph, 10, 0).0, 2);
}
