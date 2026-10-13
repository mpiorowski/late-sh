//! Explicit date fields accept more forms than automatic title inference.
use super::parser::{month, weekday};
use anyhow::{Result, anyhow, ensure};
use chrono::{Datelike, Days, Months, NaiveDate};

const EXAMPLES: &str = "Use YYYY-MM-DD, Oct 2, +2w or 2 months ago";

/// Offsets use `base`; today/weekday words and omitted years use account-local
/// `today`. Keeping the clocks explicit makes navigation and editing deterministic.
pub fn parse(input: &str, base: NaiveDate, today: NaiveDate) -> Result<NaiveDate> {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    let tokens: Vec<_> = lower.split_whitespace().collect();
    let found = match tokens.as_slice() {
        ["today"] => Some(today),
        ["tomorrow"] => today.succ_opt(),
        ["yesterday"] => today.pred_opt(),
        [day] if weekday(day).is_some() => shift_weekday(today, day, 0),
        ["next", day] if weekday(day).is_some() => shift_weekday(today, day, 1),
        ["last", day] if weekday(day).is_some() => shift_weekday(today, day, -1),
        ["next", unit] => Some(offset(&format!("1 {unit}"), base, false)?),
        ["last", unit] => Some(offset(&format!("1 {unit}"), base, true)?),
        _ => None,
    };
    if let Some(date) = found {
        return supported(date);
    }

    // Year-first numeric dates have a fixed order. Other numeric dates are only
    // accepted when their day/month order is unambiguous (e.g. 31/10/2026).
    let numeric: Vec<_> = lower.split(['-', '/', '.']).collect();
    if numeric.len() == 3
        && numeric
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        let (year, month, day) = if numeric[0].len() == 4 {
            (numeric[0], numeric[1], numeric[2])
        } else if numeric[2].len() == 4 {
            let a = numeric[0].parse::<u32>()?;
            let b = numeric[1].parse::<u32>()?;
            if a > 12 && b <= 12 {
                (numeric[2], numeric[1], numeric[0])
            } else if b > 12 && a <= 12 || a == b {
                (numeric[2], numeric[0], numeric[1])
            } else {
                return Err(anyhow!("Ambiguous date; use YYYY-MM-DD or a month name"));
            }
        } else {
            return Err(anyhow!("Include a four-digit year in numeric dates"));
        };
        return civil(year.parse()?, month.parse()?, day.parse()?);
    }

    let named = lower.replace([',', '-', '/', '.'], " ");
    let names: Vec<_> = named.split_whitespace().collect();
    let (month, day, year) = match names.as_slice() {
        [m, d] if named_month(m).is_some() => (m, d, None),
        [d, m] if named_month(m).is_some() => (m, d, None),
        [m, d, y] if named_month(m).is_some() => (m, d, Some(y)),
        [d, m, y] if named_month(m).is_some() => (m, d, Some(y)),
        _ => return relative(&normalized, &lower, base),
    };
    let year = match year {
        Some(y) if y.len() == 4 => y.parse()?,
        Some(_) => return Err(anyhow!("Use a four-digit year")),
        None => today.year(),
    };
    civil(year, named_month(month).unwrap(), day_number(day)?)
}

fn named_month(s: &str) -> Option<u32> {
    if s == "sept" { Some(9) } else { month(s) }
}

fn day_number(s: &str) -> Result<u32> {
    for suffix in ["st", "nd", "rd", "th"] {
        if let Some(number) = s.strip_suffix(suffix) {
            let day: u32 = number.parse()?;
            let expected = match day % 100 {
                11..=13 => "th",
                _ => match day % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                },
            };
            ensure!(suffix == expected, "Invalid day ordinal");
            return Ok(day);
        }
    }
    Ok(s.parse()?)
}

fn civil(year: i32, month: u32, day: u32) -> Result<NaiveDate> {
    supported(NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| anyhow!("Invalid date"))?)
}

fn supported(date: NaiveDate) -> Result<NaiveDate> {
    // Keeps visible ranges and database date arithmetic safely inside their
    // representable bounds, even after navigation at either end of this range.
    ensure!(
        (1..=9999).contains(&date.year()),
        "Date must be in years 0001–9999"
    );
    Ok(date)
}

fn shift_weekday(today: NaiveDate, day: &str, direction: i32) -> Option<NaiveDate> {
    let target = weekday(day)?;
    let current = today.weekday().num_days_from_monday();
    if direction < 0 {
        let days = (current + 7 - target) % 7;
        today.checked_sub_days(Days::new(if days == 0 { 7 } else { days }.into()))
    } else {
        let days = (target + 7 - current) % 7;
        today.checked_add_days(Days::new(
            if days == 0 && direction > 0 { 7 } else { days }.into(),
        ))
    }
}

fn relative(input: &str, lower: &str, base: NaiveDate) -> Result<NaiveDate> {
    let (expression, backwards) = if lower.ends_with(" ago") {
        (&input[..input.len() - 4], true)
    } else if lower.starts_with("in ") {
        (&input[3..], false)
    } else if lower.ends_with(" from now") {
        (&input[..input.len() - 9], false)
    } else if let Some(s) = input.strip_prefix('-').or_else(|| input.strip_prefix('−')) {
        (s, true)
    } else {
        (input.strip_prefix('+').unwrap_or(input), false)
    };
    supported(offset(expression, base, backwards)?)
}

fn offset(expression: &str, base: NaiveDate, backwards: bool) -> Result<NaiveDate> {
    let mut rest = expression.trim();
    ensure!(!rest.is_empty(), EXAMPLES);
    let mut months = 0u32;
    let mut fixed = String::new();
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        ensure!(end > 0, EXAMPLES);
        let number = &rest[..end];
        let amount: u32 = number
            .parse()
            .map_err(|_| anyhow!("Date offset is too large"))?;
        rest = rest[end..].trim_start();
        let end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = &rest[..end];
        let lower = unit.to_ascii_lowercase();
        let calendar_months =
            if unit == "M" || matches!(lower.as_str(), "month" | "months" | "mo" | "mos") {
                Some(amount)
            } else if matches!(lower.as_str(), "year" | "years" | "yr" | "yrs" | "y") {
                Some(
                    amount
                        .checked_mul(12)
                        .ok_or_else(|| anyhow!("Date offset is too large"))?,
                )
            } else {
                ensure!(
                    matches!(
                        lower.as_str(),
                        "day" | "days" | "d" | "week" | "weeks" | "wk" | "wks" | "w"
                    ),
                    "Date offsets use whole days, weeks, months or years"
                );
                fixed.push_str(&format!("{number}{lower} "));
                None
            };
        if let Some(n) = calendar_months {
            months = months
                .checked_add(n)
                .ok_or_else(|| anyhow!("Date offset is too large"))?;
        }
        rest = rest[end..].trim_start();
    }
    let days = if fixed.is_empty() {
        0
    } else {
        humantime::parse_duration(&fixed)?.as_secs() / 86400
    };
    // Months/years retain calendar semantics (including leap days), rather than
    // humantime's average-length duration units. Apply them before weeks/days.
    let date = if backwards {
        base.checked_sub_months(Months::new(months))
            .and_then(|d| d.checked_sub_days(Days::new(days)))
    } else {
        base.checked_add_months(Months::new(months))
            .and_then(|d| d.checked_add_days(Days::new(days)))
    };
    date.ok_or_else(|| anyhow!("Date offset is out of range"))
}
