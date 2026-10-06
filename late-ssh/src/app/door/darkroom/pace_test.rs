use chrono::{DateTime, TimeZone, Utc};

use super::pace::{DAILY_CREDIT_FLOOR_SECS, DAILY_CREDIT_SECS, Pace, SLOWDOWN, slowed};

fn at(day: i64, hour: u32) -> DateTime<Utc> {
    Utc.timestamp_opt(day * 86_400 + i64::from(hour) * 3600, 0)
        .unwrap()
}

#[test]
fn daily_allowance_truncates_a_long_session() {
    let mut pace = Pace::default();
    let now = at(20_000, 12);

    // An eight hour session on one day still only banks the daily allowance.
    let first = pace.grant(now, 2 * 60 * 60, 0);
    let second = pace.grant(now, 6 * 60 * 60, 0);

    assert_eq!(first, 2 * 60 * 60);
    assert_eq!(second, DAILY_CREDIT_SECS - 2 * 60 * 60);
    assert_eq!(first + second, DAILY_CREDIT_SECS);
    assert!(pace.exhausted(now));
}

#[test]
fn a_new_utc_day_restores_the_allowance() {
    let mut pace = Pace::default();
    let today = at(20_000, 23);
    pace.grant(today, DAILY_CREDIT_SECS, 0);
    assert!(pace.exhausted(today));

    let tomorrow = at(20_001, 1);
    assert_eq!(pace.remaining(tomorrow), DAILY_CREDIT_SECS);
    assert_eq!(pace.grant(tomorrow, 60, 0), 60);
    assert_eq!(pace.remaining(tomorrow), DAILY_CREDIT_SECS - 60);
}

#[test]
fn the_floor_tops_up_a_short_visit_once_per_day() {
    let mut pace = Pace::default();
    let now = at(20_000, 12);

    // A one-minute first visit lands the whole floor...
    assert_eq!(
        pace.grant(now, 60, DAILY_CREDIT_FLOOR_SECS),
        DAILY_CREDIT_FLOOR_SECS
    );
    // ...and later grants the same day are back to elapsed time.
    assert_eq!(pace.grant(now, 60, DAILY_CREDIT_FLOOR_SECS), 60);

    // A new day restores it.
    let tomorrow = at(20_001, 9);
    assert_eq!(
        pace.grant(tomorrow, 60, DAILY_CREDIT_FLOOR_SECS),
        DAILY_CREDIT_FLOOR_SECS
    );
}

#[test]
fn the_floor_counts_against_the_daily_allowance() {
    let mut pace = Pace::default();
    let now = at(20_000, 12);

    let floored = pace.grant(now, 60, DAILY_CREDIT_FLOOR_SECS);
    let rest = pace.grant(now, DAILY_CREDIT_SECS, DAILY_CREDIT_FLOOR_SECS);
    assert_eq!(
        floored + rest,
        DAILY_CREDIT_SECS,
        "the floor is not a bonus on top of the cap"
    );

    // A session already longer than the floor gains nothing from it.
    let mut long = Pace::default();
    assert_eq!(
        long.grant(now, DAILY_CREDIT_FLOOR_SECS + 600, DAILY_CREDIT_FLOOR_SECS),
        DAILY_CREDIT_FLOOR_SECS + 600
    );
}

#[test]
fn the_village_runs_slower_than_upstream() {
    assert_eq!(slowed(10), 10 * SLOWDOWN);
}
