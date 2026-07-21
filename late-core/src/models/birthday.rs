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
