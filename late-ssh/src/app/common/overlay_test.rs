use super::overlay::{Overlay, OverlayInk, OverlaySpan, draw_overlay};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn overlay_height_fits_wrapped_unicode_in_plain_and_styled_rows() {
    let body = "界".repeat(70);
    for overlay in [
        Overlay::new("Widths", vec![body.clone()]),
        Overlay::styled(
            "Widths",
            vec![vec![OverlaySpan {
                text: body.clone(),
                ink: OverlayInk::Body,
            }]],
        ),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(20, 30)).unwrap();
        terminal
            .draw(|frame| draw_overlay(frame, frame.area(), &overlay))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer
                .content
                .iter()
                .filter(|cell| cell.symbol() == "界")
                .count(),
            70
        );
    }
}
