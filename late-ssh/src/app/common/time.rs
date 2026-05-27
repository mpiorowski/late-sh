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

/// Compact duration like "2h 14m", "47m", "12s". Used by the sidebar
/// presence row where horizontal real estate is scarce. Caps at days so
/// long-lived sessions render "3d 4h" instead of "76h 14m".
pub fn format_short_duration(secs: u64) -> String {
    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    if secs >= DAY {
        let d = secs / DAY;
        let h = (secs % DAY) / HOUR;
        if h == 0 {
            format!("{d}d")
        } else {
            format!("{d}d {h}h")
        }
    } else if secs >= HOUR {
        let h = secs / HOUR;
        let m = (secs % HOUR) / MIN;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    } else if secs >= MIN {
        format!("{}m", secs / MIN)
    } else {
        format!("{secs}s")
    }
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

    #[test]
    fn format_short_duration_buckets_correctly() {
        assert_eq!(format_short_duration(0), "0s");
        assert_eq!(format_short_duration(47), "47s");
        assert_eq!(format_short_duration(60), "1m");
        assert_eq!(format_short_duration(599), "9m");
        assert_eq!(format_short_duration(3600), "1h");
        assert_eq!(format_short_duration(3600 + 14 * 60), "1h 14m");
        assert_eq!(format_short_duration(2 * 3600 + 59 * 60), "2h 59m");
    }

    #[test]
    fn format_short_duration_caps_at_days() {
        assert_eq!(format_short_duration(24 * 3600), "1d");
        assert_eq!(format_short_duration(24 * 3600 + 4 * 3600), "1d 4h");
        assert_eq!(format_short_duration(3 * 24 * 3600 + 4 * 3600), "3d 4h");
    }
}
