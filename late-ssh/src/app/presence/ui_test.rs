use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn pulse_rows(width: u16, height: u16, online: usize) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| draw_pulse_tile(frame, Rect::new(0, 0, width, height), online, &[]))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// A tile with no samples yet: the header counts the live people and says
/// the day is still sampling, the sparkline is a row of gaps with this
/// minute's live count standing at its end, and the axis marks the day.
#[test]
fn a_three_row_pulse_tile_is_header_bars_and_axis() {
    let rows = pulse_rows(24, 3, 3);
    assert_eq!(rows[0], "online 3        sampling");
    assert_eq!(rows[1], format!("{}█", "·".repeat(23)));
    assert_eq!(rows[2], "24h        12h       now");

    // One row is the sparkline alone; two rows add the header.
    assert_eq!(pulse_rows(24, 1, 3)[0], format!("{}█", "·".repeat(23)));
    assert_eq!(pulse_rows(24, 2, 3)[0], "online 3        sampling");
}
