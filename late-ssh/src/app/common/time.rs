use chrono::{DateTime, Utc};
use chrono_tz::Tz;

pub fn timezone_current_time(now: DateTime<Utc>, timezone: Option<&str>) -> Option<String> {
    let timezone = timezone?.trim();
    if timezone.is_empty() {
        return None;
    }
    let tz: Tz = timezone.parse().ok()?;
    Some(now.with_timezone(&tz).format("%a %H:%M").to_string())
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
