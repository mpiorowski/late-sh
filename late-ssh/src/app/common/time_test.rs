use crate::app::common::time::*;
use chrono::TimeZone;
use chrono::Utc;

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

#[test]
fn instants_carry_the_viewers_zone_and_fall_back_to_utc() {
    let at = Utc
        .with_ymd_and_hms(2026, 8, 28, 14, 30, 0)
        .single()
        .unwrap();
    assert_eq!(
        instant_for_viewer(at, Some(chrono_tz::Europe::Warsaw)),
        "Aug 28 16:30 CEST"
    );
    // No account zone: the label says UTC rather than leaving the reader to
    // guess which clock the time is on.
    assert_eq!(instant_for_viewer(at, None), "Aug 28 14:30 UTC");
}
