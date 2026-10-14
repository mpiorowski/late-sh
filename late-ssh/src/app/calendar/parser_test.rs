use super::parser::infer;
use chrono::{NaiveDate, TimeZone};
use late_core::models::calendar::EventTiming;
fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}
#[test]
fn calendar_suffix_inference_dates_and_times() {
    let today = d("2026-10-02");
    let tz = chrono_tz::UTC;
    for (title, date) in [
        ("Party — tomorrow", "2026-10-03"),
        ("Meeting Friday", "2026-10-02"),
        ("Meeting next Friday", "2026-10-09"),
        ("Leap Feb 29 2028", "2028-02-29"),
        ("Trip October 31", "2026-10-31"),
        ("Trip 2027-01-01", "2027-01-01"),
    ] {
        let i = infer(title, today, today, tz).unwrap();
        assert_eq!(
            i.timing,
            EventTiming::AllDay {
                start: d(date),
                end_exclusive: d(date).succ_opt().unwrap()
            }
        );
    }
    for (suffix, hour) in [
        ("at 16:00", 16),
        ("4pm", 16),
        ("4 PM", 16),
        ("12am", 0),
        ("12:00 PM", 12),
    ] {
        let i = infer(&format!("Hello {suffix}"), today, today, tz).unwrap();
        assert_eq!(i.title, "Hello");
        assert_eq!(
            i.timing,
            EventTiming::Timed {
                start: tz
                    .with_ymd_and_hms(2026, 10, 2, hour, 0, 0)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                end: None
            }
        );
    }
    assert_eq!(
        infer("Tea tomorrow at 4:30 PM", today, today, tz)
            .unwrap()
            .title,
        "Tea"
    );
}
#[test]
fn calendar_suffix_inference_leaves_invalid_and_ambiguous_titles() {
    let today = d("2026-10-02");
    for title in [
        "tomorrow",
        "2026-10-02",
        "Hello 02/03/2026",
        "Hello Feb 30 at 4pm",
        "Hello 2026-02-29 at 4pm",
        "Hello 25:00",
        "Hello someday",
        "Hello in two days",
        "Hello tomorrow at",
        "Hello Oct 32",
    ] {
        assert!(
            infer(title, today, today, chrono_tz::UTC).is_none(),
            "{title}"
        );
    }
    let tz = chrono_tz::America::New_York;
    assert!(infer("Gap 2026-03-08 02:30", today, today, tz).is_none());
    assert!(infer("Repeat 2026-11-01 01:30", today, today, tz).is_none());
    let i = infer("Coffee 2026-03-08 03:30", today, today, tz).unwrap();
    assert_eq!(i.title, "Coffee");
}
