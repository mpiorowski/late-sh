use crate::app::stream::ui::draw_obs_overlay;
use ratatui::{Terminal, backend::TestBackend};

fn render_text(width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw_obs_overlay(
                frame,
                frame.area(),
                "https://whip.late.sh/w",
                "sk-abc123",
                "https://late.sh/live/deadbeef",
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn obs_overlay_shows_every_hand_copied_value() {
    let text = render_text(100, 32);

    // The three values are copied into OBS by hand; a clipped value is a
    // silently broken one.
    assert!(text.contains("https://whip.late.sh/w"), "whip url:\n{text}");
    assert!(text.contains("sk-abc123"), "stream key:\n{text}");
    assert!(
        text.contains("https://late.sh/live/deadbeef"),
        "watch url:\n{text}"
    );
    assert!(text.contains("Service: WHIP"), "obs instructions:\n{text}");
    assert!(
        text.contains("Press any key to close."),
        "dismiss hint:\n{text}"
    );
}

#[test]
fn obs_overlay_survives_a_tiny_terminal() {
    // No panic and the dismiss path still hinted; values wrap instead of
    // clipping where the width allows.
    let text = render_text(30, 12);
    assert!(text.contains("WHIP"), "modal renders at 30x12:\n{text}");
}
