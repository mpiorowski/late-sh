use std::time::{Duration, Instant};

use chrono::NaiveDate;
use late_core::models::daily_rung::rung_for;

use super::super::svc::QuestSnapshot;
use super::{PUSH_AFTER, PUSH_FOR, ReturnPush, RungStanding};

fn epoch_rung() -> late_core::models::daily_rung::Rung {
    // The rotation's first day: medium Sudoku.
    rung_for(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap())
}

fn with_streak(days: i32) -> QuestSnapshot {
    let mut snapshot = QuestSnapshot::default();
    snapshot.daily_streak.consecutive_days = days;
    snapshot
}

#[test]
fn a_short_quiet_arms_nothing_and_a_long_one_holds_for_the_window() {
    let now = Instant::now();
    let mut push = ReturnPush::default();
    assert!(!push.note_return(now, PUSH_AFTER - Duration::from_secs(1)));
    assert_eq!(
        push.line(now, &with_streak(4), epoch_rung(), RungStanding::Open),
        None
    );

    assert!(push.note_return(now, Duration::from_secs(9 * 3600 + 7 * 60)));
    assert_eq!(
        push.line(now, &with_streak(4), epoch_rung(), RungStanding::Open),
        Some("back after 9 h · streak day 4 · the rung: medium Sudoku, open".to_string())
    );
    assert!(
        push.line(
            now + PUSH_FOR - Duration::from_secs(1),
            &with_streak(4),
            epoch_rung(),
            RungStanding::Open,
        )
        .is_some()
    );
    assert_eq!(
        push.line(
            now + PUSH_FOR,
            &with_streak(4),
            epoch_rung(),
            RungStanding::Open
        ),
        None
    );
}

#[test]
fn the_line_drops_a_zero_streak_marks_a_won_rung_and_counts_days_past_two() {
    let now = Instant::now();
    let mut push = ReturnPush::default();
    push.note_return(now, Duration::from_secs(3 * 24 * 3600 + 3600));
    assert_eq!(
        push.line(now, &with_streak(0), epoch_rung(), RungStanding::Done),
        Some("back after 3 d · the rung: medium Sudoku, done ✦".to_string())
    );
}
