use chrono::NaiveDate;

use super::{curated_board_key, daily_board_key, monthly_board_key};

#[test]
fn daily_board_key_uses_iso_date() {
    let date = NaiveDate::from_ymd_opt(2026, 4, 30).expect("valid date");
    assert_eq!(daily_board_key(date), "daily:2026-04-30");
}

#[test]
fn monthly_board_key_uses_year_month() {
    let date = NaiveDate::from_ymd_opt(2026, 4, 30).expect("valid date");
    assert_eq!(monthly_board_key(date), "monthly:2026-04");
}

#[test]
fn curated_board_key_uses_iso_date_and_optional_suffix() {
    let date = NaiveDate::from_ymd_opt(2026, 5, 25).expect("valid date");
    assert_eq!(curated_board_key(date, 0), "curated:2026-05-25");
    assert_eq!(curated_board_key(date, 1), "curated:2026-05-25-2");
}
