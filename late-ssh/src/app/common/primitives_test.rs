use crate::app::common::primitives::*;
use ratatui::layout::Rect;
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

#[test]
fn too_small_text_names_the_screen_the_requirement_and_the_current_size() {
    assert_eq!(
        too_small_text("Rubik's Cube", 42, 18, Rect::new(0, 0, 30, 12)),
        "Rubik's Cube needs at least 42×18, this space is 30×12"
    );
}
