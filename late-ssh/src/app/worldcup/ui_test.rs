use super::*;
use crate::app::worldcup::model::{Match, MatchStatus};

#[test]
fn score_str_shows_score_or_placeholder() {
    let finished = Match {
        home_score: Some(2),
        away_score: Some(1),
        status: MatchStatus::Finished,
        ..Default::default()
    };
    assert_eq!(score_str(&finished), "2-1");

    let upcoming = Match {
        status: MatchStatus::Upcoming,
        ..Default::default()
    };
    assert_eq!(score_str(&upcoming), "v");
}

#[test]
fn clip_name_keeps_short_and_ellipsizes_long() {
    assert_eq!(clip_name("Spain", 12), "Spain");
    // Exactly 12 chars is kept whole; a 13th forces the ellipsis.
    assert_eq!(clip_name("Saudi Arabia", 12), "Saudi Arabia");
    assert_eq!(clip_name("Bosnia and Herzegovina", 12), "Bosnia and H…");
}

#[test]
fn flag_prefix_honours_tweak() {
    assert_eq!(
        flag_prefix("Spain", true),
        format!("{} ", flag_emoji("Spain"))
    );
    assert_eq!(flag_prefix("Spain", false), "");
}

#[test]
fn code_or_falls_back_to_tbd() {
    assert_eq!(code_or("GER"), "GER");
    assert_eq!(code_or("   "), "TBD");
}

#[test]
fn kickoff_splits_into_day_and_time() {
    use chrono::TimeZone;
    let m = Match {
        kickoff: Some(Utc.with_ymd_and_hms(2026, 6, 30, 21, 0, 0).unwrap()),
        status: MatchStatus::Upcoming,
        ..Default::default()
    };
    // No timezone set -> UTC, the snapshot's native zone.
    assert_eq!(kickoff_day(&m, None), Some("Jun 30".to_string()));
    assert_eq!(kickoff_time(&m, None), "21:00");

    let tbd = Match::default();
    assert_eq!(kickoff_day(&tbd, None), None);
    assert_eq!(kickoff_time(&tbd, None), "");
}

#[test]
fn kickoff_uses_account_timezone_when_set() {
    use chrono::TimeZone;
    // 21:00 UTC is 23:00 the same day in Europe/Warsaw (UTC+2 in summer).
    let m = Match {
        kickoff: Some(Utc.with_ymd_and_hms(2026, 6, 30, 21, 0, 0).unwrap()),
        status: MatchStatus::Upcoming,
        ..Default::default()
    };
    let tz: Tz = "Europe/Warsaw".parse().unwrap();
    assert_eq!(kickoff_day(&m, Some(tz)), Some("Jun 30".to_string()));
    assert_eq!(kickoff_time(&m, Some(tz)), "23:00");

    // 2026-06-30 22:30 UTC is 2026-07-01 00:30 in Warsaw -> day rolls over.
    let late = Match {
        kickoff: Some(Utc.with_ymd_and_hms(2026, 6, 30, 22, 30, 0).unwrap()),
        status: MatchStatus::Upcoming,
        ..Default::default()
    };
    assert_eq!(kickoff_day(&late, Some(tz)), Some("Jul 01".to_string()));
    assert_eq!(kickoff_time(&late, Some(tz)), "00:30");
}

#[test]
fn banner_fills_to_width() {
    let line = banner_line("Jun 30", 20);
    assert_eq!(line.width(), 20);
}
