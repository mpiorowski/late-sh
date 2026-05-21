//! Welcome-back banner content: a small curated list of friendly reminders, a
//! seed-based picker, and a relative-time formatter. Pure — no clock or DB
//! dependency. Callers pass `now` explicitly so the logic is unit-testable.

use chrono::{DateTime, Utc};

/// Show a welcome-back banner only when the previous session ended at least
/// this long ago. Anything more recent is treated as a quick reconnect and
/// suppressed.
pub const WELCOME_MIN_GAP_SECS: i64 = 60 * 60; // 1 hour

/// Reminders shown alongside the welcome-back greeting. Order is not part of
/// any contract; `pick_reminder` always picks via modulo so reordering or
/// extending the list is safe.
pub const REMINDERS: &[&str] = &[
    "remember to look after your bonsai 🌱",
    "pull up a comfy pillow and chill 🛋",
    "remember to be kind 💛",
    "stretch your shoulders — you've earned it",
    "take a sip of water",
    "be soft with someone today",
    "look out a window for a minute",
    "you don't have to reply right away",
];

/// Picks a reminder deterministically from `seed`. Callers typically derive
/// the seed from `user_id` ^ day-of-year so the message is stable for a
/// session but varies day-to-day.
pub fn pick_reminder(seed: u64) -> &'static str {
    let idx = (seed as usize) % REMINDERS.len();
    REMINDERS[idx]
}

/// Humanises the gap between `then` and `now` into a short relative string
/// for the welcome-back line ("3 hours ago", "yesterday", "5 days ago", "a
/// moment ago"). Treats future timestamps as "a moment ago" defensively.
pub fn humanize_since(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = now.signed_duration_since(then);
    let secs = diff.num_seconds();
    if secs < 60 {
        return "a moment ago".to_string();
    }
    let mins = diff.num_minutes();
    if mins < 60 {
        return format!("{} minute{} ago", mins, if mins == 1 { "" } else { "s" });
    }
    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" });
    }
    let days = diff.num_days();
    if days == 1 {
        return "yesterday".to_string();
    }
    if days < 7 {
        return format!("{days} days ago");
    }
    let weeks = days / 7;
    if weeks < 5 {
        return format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" });
    }
    let months = days / 30;
    if months < 12 {
        return format!("{} month{} ago", months, if months == 1 { "" } else { "s" });
    }
    let years = days / 365;
    format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
}

/// Builds the welcome banner message. Returns `None` when the banner should
/// be suppressed (returning user reconnecting within `WELCOME_MIN_GAP_SECS`).
/// `previous_last_seen = None` is treated as a first-time visit and always
/// produces a banner.
pub fn build_welcome_message(
    username: &str,
    previous_last_seen: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    reminder_seed: u64,
) -> Option<String> {
    let reminder = pick_reminder(reminder_seed);
    match previous_last_seen {
        None => Some(format!("welcome to late.sh, @{username} — {reminder}")),
        Some(then) => {
            let gap = now.signed_duration_since(then).num_seconds();
            if gap < WELCOME_MIN_GAP_SECS {
                return None;
            }
            let when = humanize_since(then, now);
            Some(format!(
                "welcome back, @{username} — last on {when}. {reminder}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    fn at_s(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
            .unwrap()
    }

    #[test]
    fn reminders_are_non_empty_and_unique() {
        assert!(!REMINDERS.is_empty());
        for r in REMINDERS {
            assert!(!r.is_empty(), "empty reminder string");
        }
        let mut sorted: Vec<&&str> = REMINDERS.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), REMINDERS.len(), "duplicate reminders");
    }

    #[test]
    fn pick_reminder_is_deterministic() {
        for seed in 0..256u64 {
            assert_eq!(pick_reminder(seed), pick_reminder(seed));
        }
    }

    #[test]
    fn pick_reminder_covers_all_indices() {
        for (i, expected) in REMINDERS.iter().enumerate() {
            assert_eq!(pick_reminder(i as u64), *expected);
        }
    }

    #[test]
    fn pick_reminder_wraps_modulo() {
        let n = REMINDERS.len() as u64;
        assert_eq!(pick_reminder(n), pick_reminder(0));
        assert_eq!(pick_reminder(n * 7 + 3), pick_reminder(3));
    }

    #[test]
    fn humanize_since_buckets() {
        let now = at(2026, 5, 21, 12, 0);
        assert_eq!(
            humanize_since(at_s(2026, 5, 21, 11, 59, 30), now),
            "a moment ago"
        );
        assert_eq!(humanize_since(at(2026, 5, 21, 11, 59), now), "1 minute ago");
        assert_eq!(
            humanize_since(at(2026, 5, 21, 11, 58), now),
            "2 minutes ago"
        );
        assert_eq!(humanize_since(at(2026, 5, 21, 11, 0), now), "1 hour ago");
        assert_eq!(humanize_since(at(2026, 5, 21, 9, 0), now), "3 hours ago");
        assert_eq!(humanize_since(at(2026, 5, 20, 12, 0), now), "yesterday");
        assert_eq!(humanize_since(at(2026, 5, 18, 12, 0), now), "3 days ago");
        assert_eq!(humanize_since(at(2026, 5, 13, 12, 0), now), "1 week ago"); // 8d → 1w
        assert_eq!(humanize_since(at(2026, 5, 7, 12, 0), now), "2 weeks ago"); // 14d → 2w
        assert_eq!(humanize_since(at(2026, 4, 1, 12, 0), now), "1 month ago"); // 50d → 1mo
        assert_eq!(humanize_since(at(2025, 5, 21, 12, 0), now), "1 year ago");
    }

    #[test]
    fn humanize_since_handles_future_gracefully() {
        let now = at(2026, 5, 21, 12, 0);
        let future = at(2026, 5, 21, 13, 0);
        assert_eq!(humanize_since(future, now), "a moment ago");
    }

    #[test]
    fn build_welcome_message_first_visit() {
        let msg = build_welcome_message("hardlygospel", None, at(2026, 5, 21, 12, 0), 0);
        let m = msg.expect("first visit always produces a message");
        assert!(m.starts_with("welcome to late.sh, @hardlygospel"));
        assert!(m.contains(REMINDERS[0]));
    }

    #[test]
    fn build_welcome_message_suppresses_quick_reconnect() {
        let now = at(2026, 5, 21, 12, 0);
        let recent = at(2026, 5, 21, 11, 5); // 55 min — under the 60-min gap
        assert_eq!(build_welcome_message("u", Some(recent), now, 0), None);
    }

    #[test]
    fn build_welcome_message_returns_for_returning_user() {
        let now = at(2026, 5, 21, 12, 0);
        let earlier = at(2026, 5, 21, 9, 0); // 3 hours ago
        let m = build_welcome_message("hardlygospel", Some(earlier), now, 2)
            .expect("3-hour gap should produce a banner");
        assert!(m.starts_with("welcome back, @hardlygospel"));
        assert!(m.contains("3 hours ago"));
        assert!(m.contains(REMINDERS[2]));
    }
}
