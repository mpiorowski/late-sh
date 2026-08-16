use crate::app::common::primitives::*;
use ratatui::{layout::Rect, text::Span};
use std::time::{Duration, Instant};

#[test]
fn screen_next_cycles_top_level_screens() {
    assert_eq!(Screen::Clubhouse.next(), Screen::Dashboard);
    assert_eq!(Screen::Dashboard.next(), Screen::Arcade);
    assert_eq!(Screen::Arcade.next(), Screen::Games);
    assert_eq!(Screen::Games.next(), Screen::Artboard);
    assert_eq!(Screen::Artboard.next(), Screen::Profiles);
    assert_eq!(Screen::Profiles.next(), Screen::Leaderboard);
    assert_eq!(Screen::Leaderboard.next(), Screen::Clubhouse);
}

#[test]
fn screen_prev_cycles_top_level_screens() {
    assert_eq!(Screen::Clubhouse.prev(), Screen::Leaderboard);
    assert_eq!(Screen::Leaderboard.prev(), Screen::Profiles);
    assert_eq!(Screen::Dashboard.prev(), Screen::Clubhouse);
    assert_eq!(Screen::Arcade.prev(), Screen::Dashboard);
    assert_eq!(Screen::Games.prev(), Screen::Arcade);
    assert_eq!(Screen::Artboard.prev(), Screen::Games);
    assert_eq!(Screen::Profiles.prev(), Screen::Artboard);
}

#[test]
fn door_games_are_outside_the_tab_cycle_and_fall_back_to_the_hub() {
    for door in [
        Screen::Lateania,
        Screen::Rebels,
        Screen::Nethack,
        Screen::Dcss,
        Screen::Brogue,
        Screen::Dopewars,
        Screen::Codekeep,
        Screen::Usurper,
        Screen::GreenDragon,
    ] {
        assert_eq!(door.next(), Screen::Games);
        assert_eq!(door.prev(), Screen::Games);
    }
}

#[test]
fn daily_match_board_is_outside_the_tab_cycle_and_falls_back_home() {
    assert_eq!(Screen::DailyMatch.next(), Screen::Dashboard);
    assert_eq!(Screen::DailyMatch.prev(), Screen::Dashboard);
}

#[test]
fn format_duration_mmss_formats_minutes_and_seconds() {
    assert_eq!(format_duration_mmss(Duration::from_secs(0)), "0:00");
    assert_eq!(format_duration_mmss(Duration::from_secs(65)), "1:05");
    assert_eq!(format_duration_mmss(Duration::from_secs(3599)), "59:59");
}

#[test]
fn banner_is_active_for_recent_messages() {
    let fresh = Banner::success("ok");
    assert!(fresh.is_active());

    let stale = Banner {
        message: "old".to_string(),
        kind: BannerKind::Error,
        created_at: Instant::now() - Duration::from_secs(6),
    };
    assert!(!stale.is_active());
}

#[test]
fn thousands_groups_digits() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(10_000), "10,000");
    assert_eq!(thousands(1_234_567), "1,234,567");
    assert_eq!(thousands(-10_000), "-10,000");
}

/// The hint is what carries a URL on a header row (the room header's
/// `watch: …` nudge), so it must stop short of the frame border: a terminal
/// that linkifies by scanning the row would otherwise swallow `│` into the
/// link.
#[test]
fn row_with_hint_stops_a_cell_short_of_the_right_edge() {
    let line = row_with_hint(
        vec![Span::raw("● LIVE")],
        vec![Span::raw("watch: https://late.sh/live/abc")],
        60,
    );
    assert_eq!(line.width(), 60 - EDGE_GAP);
    assert!(line.to_string().ends_with("/abc"), "{line:?}");
}

#[test]
fn row_with_hint_drops_the_hint_when_the_reserved_cell_would_not_fit() {
    let left = vec![Span::raw("0123456789")];
    let right = vec![Span::raw("hint")];
    // 10 + 4 + 2 separating cells + the reserved cell needs 17.
    assert_eq!(row_with_hint(left.clone(), right.clone(), 17).width(), 16);
    assert_eq!(row_with_hint(left, right, 16).width(), 10);
}

#[test]
fn horizontal_inset_narrows_both_sides_and_never_underflows() {
    let inset = horizontal_inset(Rect::new(4, 2, 20, 3), 1);
    assert_eq!((inset.x, inset.width), (5, 18));
    assert_eq!((inset.y, inset.height), (2, 3));

    let squeezed = horizontal_inset(Rect::new(0, 0, 1, 1), 1);
    assert_eq!((squeezed.x, squeezed.width), (0, 1));
}
