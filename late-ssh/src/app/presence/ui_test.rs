use super::*;
use late_core::models::user::AudioSource;
use ratatui::{Terminal, backend::TestBackend};

fn friend(username: &str, away: bool) -> ActiveFriend {
    ActiveFriend {
        user_id: uuid::Uuid::now_v7(),
        username: username.to_string(),
        audio_source: AudioSource::default(),
        online_since: std::time::Instant::now(),
        away,
    }
}

#[test]
fn friend_names_text_keeps_every_name_when_the_row_is_wide() {
    let friends = vec![friend("ada", false), friend("bob", false)];
    assert_eq!(friend_names_text(&friends, 40, 0), "@ada @bob");
}

#[test]
fn friend_names_text_marks_an_away_friend_with_the_glyph_alone() {
    let friends = vec![friend("ada", false), friend("bob", true)];
    assert_eq!(friend_names_text(&friends, 40, 0), "@ada @bob 💤");
}

#[test]
fn friend_names_text_scrolls_past_the_names_that_do_not_fit() {
    let friends = vec![
        friend("ada", false),
        friend("bob", false),
        friend("cyd", false),
    ];
    // The full 12-wide rail shows names; the marker was retired so no
    // column is reserved.
    assert_eq!(friend_names_text(&friends, 12, 0), "@ada @bob @c");
    // Held at the start, then scrolled to the end: the tail is readable.
    // With hold 45 and step 15, the far extreme spans ticks 75..120.
    assert_eq!(friend_names_text(&friends, 12, 90), "da @bob @cyd");
}

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
