//! Pure birthday helpers. Birthdays are stored year-less as `MM-DD` strings in
//! the user `settings` JSONB (privacy: no year). All logic here is pure and
//! unit-tested with no DB or clock dependency — callers pass `today` in.

use chrono::{Datelike, NaiveDate};

/// Normalises arbitrary input to a canonical `MM-DD` string, or `None` if it
/// is not a valid month/day. Accepts `M-D`, `MM-DD`, `MM/DD`. Feb 29 is
/// allowed (it is a real birthday); day-of-month is validated against the
/// longest possible month length.
pub fn normalize_birthday(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split(['-', '/']);
    let month: u32 = parts.next()?.trim().parse().ok()?;
    let day: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) {
        return None;
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 29,
        _ => return None,
    };
    if !(1..=max_day).contains(&day) {
        return None;
    }
    Some(format!("{month:02}-{day:02}"))
}

/// Days from `today` until the next occurrence of the `MM-DD` birthday.
/// `Some(0)` means it is today. A Feb-29 birthday in a non-leap year is
/// observed on Feb 28. Returns `None` if `birthday` is not valid `MM-DD`.
pub fn days_until(birthday: &str, today: NaiveDate) -> Option<i64> {
    let canonical = normalize_birthday(birthday)?;
    let mut it = canonical.split('-');
    let month: u32 = it.next()?.parse().ok()?;
    let day: u32 = it.next()?.parse().ok()?;

    for year in [today.year(), today.year() + 1] {
        let observed = NaiveDate::from_ymd_opt(year, month, day)
            .or_else(|| NaiveDate::from_ymd_opt(year, month, day.saturating_sub(1)));
        if let Some(date) = observed
            && date >= today
        {
            return Some((date - today).num_days());
        }
    }
    None
}

/// True when the birthday falls on `today`.
pub fn is_today(birthday: &str, today: NaiveDate) -> bool {
    days_until(birthday, today) == Some(0)
}

/// True when the birthday is within `window` days ahead (1..=window). Today
/// itself is excluded — that is `is_today`'s job.
pub fn is_upcoming(birthday: &str, today: NaiveDate, window: i64) -> bool {
    matches!(days_until(birthday, today), Some(d) if d >= 1 && d <= window)
}

/// Human-readable "day Month" label for a `MM-DD` birthday, e.g. `"7 March"`.
/// Returns `None` if `birthday` is not a valid `MM-DD` string.
pub fn month_day_label(birthday: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let canonical = normalize_birthday(birthday)?;
    let mut it = canonical.split('-');
    let month: usize = it.next()?.parse().ok()?;
    let day: u32 = it.next()?.parse().ok()?;
    let name = MONTHS.get(month.checked_sub(1)?)?;
    Some(format!("{day} {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
