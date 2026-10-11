//! Deterministic, trailing-only date/time inference. No natural-language service.
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use chrono_tz::Tz;
use late_core::models::calendar::{EventTiming, local_instant};
#[derive(Debug, PartialEq, Eq)]
pub struct Inferred {
    pub title: String,
    pub timing: EventTiming,
}
enum Parsed<T> {
    Value(T),
    Invalid,
    No,
}
pub(super) fn weekday(s: &str) -> Option<u32> {
    [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ]
    .iter()
    .position(|d| *d == s || &d[..3] == s)
    .map(|n| n as u32)
}
pub(super) fn month(s: &str) -> Option<u32> {
    [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    .iter()
    .position(|d| *d == s || &d[..3] == s)
    .map(|n| n as u32 + 1)
}
fn date(tokens: &[&str], today: NaiveDate) -> Parsed<(NaiveDate, usize)> {
    let Some(first) = tokens.first() else {
        return Parsed::No;
    };
    let s = first.to_ascii_lowercase();
    match s.as_str() {
        "today" => return Parsed::Value((today, 1)),
        "tomorrow" => {
            return today
                .succ_opt()
                .map(|d| Parsed::Value((d, 1)))
                .unwrap_or(Parsed::Invalid);
        }
        _ => {}
    }
    let next = s == "next";
    let day = if next {
        tokens.get(1).map(|t| t.to_ascii_lowercase())
    } else {
        Some(s.clone())
    };
    if let Some(w) = day.as_deref().and_then(weekday) {
        let mut delta = (w + 7 - today.weekday().num_days_from_monday()) % 7;
        if next && delta == 0 {
            delta = 7;
        }
        return Parsed::Value((
            today + Duration::days(delta as i64),
            if next { 2 } else { 1 },
        ));
    }
    if s.len() == 10 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-') {
        return NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map(|d| Parsed::Value((d, 1)))
            .unwrap_or(Parsed::Invalid);
    }
    if s.contains('/') && s.chars().all(|c| c.is_ascii_digit() || c == '/') {
        return Parsed::Invalid;
    }
    if let Some(m) = month(&s) {
        let Some(d) = tokens
            .get(1)
            .and_then(|d| d.trim_end_matches(',').parse::<u32>().ok())
        else {
            return Parsed::Invalid;
        };
        let mut used = 2;
        let year = if let Some(y) = tokens
            .get(2)
            .filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
        {
            used = 3;
            y.parse().unwrap_or(0)
        } else {
            today.year()
        };
        return NaiveDate::from_ymd_opt(year, m, d)
            .map(|d| Parsed::Value((d, used)))
            .unwrap_or(Parsed::Invalid);
    }
    Parsed::No
}
fn time(tokens: &[&str]) -> Parsed<NaiveTime> {
    if tokens.is_empty() || tokens.len() > 2 {
        return Parsed::No;
    }
    let combined = tokens.join("").to_ascii_lowercase();
    let (clock, meridiem) = if let Some(c) = combined.strip_suffix("am") {
        (c, Some(false))
    } else if let Some(c) = combined.strip_suffix("pm") {
        (c, Some(true))
    } else {
        (combined.as_str(), None)
    };
    if meridiem.is_none() && (!clock.contains(':') || tokens.len() != 1) {
        return Parsed::No;
    }
    let parts: Vec<_> = clock.split(':').collect();
    if parts.len() > 2
        || parts
            .iter()
            .any(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()))
    {
        return Parsed::No;
    }
    let Ok(mut h) = parts[0].parse::<u32>() else {
        return Parsed::Invalid;
    };
    let m = if parts.len() == 2 {
        if parts[1].len() != 2 {
            return Parsed::Invalid;
        }
        parts[1].parse().unwrap_or(99)
    } else {
        0
    };
    if let Some(pm) = meridiem {
        if !(1..=12).contains(&h) {
            return Parsed::Invalid;
        }
        h = h % 12 + if pm { 12 } else { 0 };
    }
    NaiveTime::from_hms_opt(h, m, 0)
        .map(Parsed::Value)
        .unwrap_or(Parsed::Invalid)
}
pub fn infer(title: &str, provisional: NaiveDate, today: NaiveDate, tz: Tz) -> Option<Inferred> {
    let mut starts: Vec<_> = title
        .char_indices()
        .filter_map(|(i, c)| {
            (!c.is_whitespace() && (i == 0 || title[..i].ends_with(char::is_whitespace)))
                .then_some(i)
        })
        .collect();
    // A supported suffix is at most six tokens. Long titles remain cheap.
    if starts.len() > 6 {
        starts = starts.split_off(starts.len() - 6);
    }
    for start in starts {
        let tokens: Vec<_> = title[start..].split_whitespace().collect();
        let (d, used) = match date(&tokens, today) {
            Parsed::Value(v) => v,
            Parsed::Invalid => return None,
            Parsed::No => (provisional, 0),
        };
        let mut rest = &tokens[used..];
        if rest.first().is_some_and(|t| t.eq_ignore_ascii_case("at")) {
            rest = &rest[1..];
            if rest.is_empty() {
                continue;
            }
        }
        let timing = if used > 0 && rest.is_empty() {
            EventTiming::AllDay {
                start: d,
                end_exclusive: d.succ_opt()?,
            }
        } else {
            match time(rest) {
                Parsed::Value(t) => EventTiming::Timed {
                    start: local_instant(d.and_time(t), tz, None).ok()?,
                    end: None,
                },
                Parsed::Invalid => return None,
                Parsed::No => continue,
            }
        };
        let remaining = title[..start].trim_end_matches(|c: char| {
            c.is_whitespace() || matches!(c, '-' | '–' | '—' | '|' | ',')
        });
        if remaining.trim().is_empty() {
            return None;
        }
        return Some(Inferred {
            title: remaining.to_string(),
            timing,
        });
    }
    None
}
