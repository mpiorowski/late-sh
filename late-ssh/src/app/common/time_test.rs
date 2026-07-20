use chrono::Utc;
use crate::app::common::time::*;
use chrono::TimeZone;

#[test]
fn formats_valid_timezone() {
    let now = Utc
        .with_ymd_and_hms(2026, 4, 19, 12, 30, 0)
        .single()
        .unwrap();
    assert_eq!(
        timezone_current_time(now, Some("Europe/Warsaw")).as_deref(),
        Some("Sun 14:30")
    );
}

#[test]
fn ignores_invalid_timezone() {
    let now = Utc
        .with_ymd_and_hms(2026, 4, 19, 12, 30, 0)
        .single()
        .unwrap();
    assert_eq!(timezone_current_time(now, Some("not/a-timezone")), None);
}
