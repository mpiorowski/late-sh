use chrono::{NaiveDate, TimeZone, Utc};

use super::hung_label;

#[test]
fn the_splash_caption_counts_the_days_since_the_hang() {
    let shown_on = NaiveDate::from_ymd_opt(2026, 9, 22).unwrap();
    let hung = |day: u32| Utc.with_ymd_and_hms(2026, 9, day, 23, 50, 0).unwrap();
    assert_eq!(hung_label(shown_on, hung(21)), "hung yesterday");
    assert_eq!(hung_label(shown_on, hung(19)), "hung 3 days ago");
    // A replica behind the piece's clock still says yesterday.
    assert_eq!(hung_label(shown_on, hung(22)), "hung yesterday");
}
