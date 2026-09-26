use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Tz;

pub fn timezone_current_time(now: DateTime<Utc>, timezone: Option<&str>) -> Option<String> {
    let timezone = timezone?.trim();
    if timezone.is_empty() {
        return None;
    }
    let tz: Tz = timezone.parse().ok()?;
    Some(now.with_timezone(&tz).format("%a %H:%M").to_string())
}
/// The same instant in the user's saved timezone, falling back to UTC when it
/// is unset or unparseable — the fallback `timezone_current_time` signals by
/// returning `None` and its callers spell as a `UTC` prefix.
///
/// Returns the whole datetime rather than a formatted string because the
/// status bar's clock formats it more than one way and also needs the hour on
/// its own for the matching clock face.
pub fn timezone_now(now: DateTime<Utc>, timezone: Option<&str>) -> DateTime<FixedOffset> {
    timezone
        .map(str::trim)
        .filter(|tz| !tz.is_empty())
        .and_then(|tz| tz.parse::<Tz>().ok())
        .map(|tz| now.with_timezone(&tz).fixed_offset())
        .unwrap_or_else(|| now.fixed_offset())
}

/// One absolute instant written for a viewer: their account zone when they
/// have set one, UTC otherwise. `%Z` names the zone either way, so the string
/// can never be read against the wrong clock.
pub fn instant_for_viewer(at: DateTime<Utc>, timezone: Option<Tz>) -> String {
    match timezone {
        Some(tz) => at.with_timezone(&tz).format("%b %d %H:%M %Z").to_string(),
        None => at.format("%b %d %H:%M UTC").to_string(),
    }
}
