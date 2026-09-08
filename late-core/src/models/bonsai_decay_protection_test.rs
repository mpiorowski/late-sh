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
