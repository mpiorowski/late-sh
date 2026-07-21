use crate::models::birthday::*;
use chrono::NaiveDate;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[test]
fn normalize_accepts_and_canonicalises() {
    assert_eq!(normalize_birthday("3-7").as_deref(), Some("03-07"));
    assert_eq!(normalize_birthday("03/07").as_deref(), Some("03-07"));
    assert_eq!(normalize_birthday(" 12-25 ").as_deref(), Some("12-25"));
    assert_eq!(normalize_birthday("02-29").as_deref(), Some("02-29"));
}

#[test]
fn normalize_rejects_garbage() {
    assert_eq!(normalize_birthday(""), None);
    assert_eq!(normalize_birthday("13-01"), None);
    assert_eq!(normalize_birthday("00-10"), None);
    assert_eq!(normalize_birthday("02-30"), None);
    assert_eq!(normalize_birthday("04-31"), None);
    assert_eq!(normalize_birthday("2026-03-07"), None);
    assert_eq!(normalize_birthday("notadate"), None);
}

#[test]
fn days_until_same_day_is_zero() {
    assert_eq!(days_until("03-07", d(2026, 3, 7)), Some(0));
    assert!(is_today("3-7", d(2026, 3, 7)));
}

#[test]
fn days_until_later_this_year() {
    assert_eq!(days_until("03-10", d(2026, 3, 7)), Some(3));
    assert!(is_upcoming("03-10", d(2026, 3, 7), 7));
    assert!(!is_upcoming("03-10", d(2026, 3, 7), 2));
}

#[test]
fn days_until_wraps_to_next_year() {
    // 1 Jan from 31 Dec is one day away, not negative.
    assert_eq!(days_until("01-01", d(2025, 12, 31)), Some(1));
}

#[test]
fn feb29_observed_on_feb28_in_non_leap_year() {
    // 2027 is not a leap year.
    assert_eq!(days_until("02-29", d(2027, 2, 28)), Some(0));
    assert!(is_today("02-29", d(2027, 2, 28)));
}

#[test]
fn upcoming_excludes_today_and_past_window() {
    assert!(!is_upcoming("03-07", d(2026, 3, 7), 7)); // today, not upcoming
    assert!(is_upcoming("03-07", d(2026, 2, 28), 7));
    assert!(!is_upcoming("03-07", d(2026, 2, 20), 7)); // outside window
}

#[test]
fn month_day_label_formats_and_rejects_garbage() {
    assert_eq!(month_day_label("03-07").as_deref(), Some("7 March"));
    assert_eq!(month_day_label("3-7").as_deref(), Some("7 March"));
    assert_eq!(month_day_label("12-25").as_deref(), Some("25 December"));
    assert_eq!(month_day_label("02-29").as_deref(), Some("29 February"));
    assert_eq!(month_day_label("notadate"), None);
    assert_eq!(month_day_label("13-40"), None);
}
