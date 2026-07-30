use super::BonsaiDecayProtection;
use chrono::{Duration, NaiveDate, Utc};

fn protection(days_from_now_start: i64, days_from_now_end: i64) -> BonsaiDecayProtection {
    let now = Utc::now();
    BonsaiDecayProtection {
        starts_at: now + Duration::days(days_from_now_start),
        ends_at: now + Duration::days(days_from_now_end),
    }
}

fn date(offset_days: i64) -> NaiveDate {
    Utc::now().date_naive() + Duration::days(offset_days)
}

#[test]
fn covers_day_is_true_inside_the_window_and_false_outside() {
    let shield = protection(0, 14);
    assert!(shield.covers_day(date(0)));
    assert!(shield.covers_day(date(7)));
    assert!(shield.covers_day(date(14)));
    assert!(!shield.covers_day(date(15)));
    assert!(!shield.covers_day(date(-1)));
}

#[test]
fn protected_days_between_counts_only_the_overlap() {
    let shield = protection(0, 14);
    // Fully inside the window.
    assert_eq!(shield.protected_days_between(date(0), date(5)), 5);
    // Spans past the end of the window: only the covered part counts
    // (days 11-14 of the (10, 20] range overlap the [0, 14] window).
    assert_eq!(shield.protected_days_between(date(10), date(20)), 4);
    // Entirely before the window starts.
    assert_eq!(shield.protected_days_between(date(-10), date(-1)), 0);
    // Entirely after the window ends.
    assert_eq!(shield.protected_days_between(date(20), date(25)), 0);
}

#[test]
fn protected_days_between_is_zero_for_an_empty_or_inverted_range() {
    let shield = protection(0, 14);
    assert_eq!(shield.protected_days_between(date(5), date(5)), 0);
    assert_eq!(shield.protected_days_between(date(5), date(2)), 0);
}
