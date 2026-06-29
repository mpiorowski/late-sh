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

#[cfg(test)]
mod tests {
    use super::*;
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
}
