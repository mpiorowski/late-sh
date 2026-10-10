use super::ical::*;
use chrono::{Duration, TimeZone, Utc};
use late_core::models::calendar::{CalendarEvent, CreationTier, EventTiming};
use uuid::Uuid;

fn content(properties: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n{properties}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

fn draft(properties: &str, tz: chrono_tz::Tz) -> late_core::models::calendar::EventDraft {
    parse(&content(properties), tz)
        .unwrap()
        .remove(0)
        .draft
        .unwrap()
}

#[test]
fn calendar_ical_dates_escapes_folding_and_metadata() {
    let d = draft(
        "DTSTART;VALUE=DATE:20261004\r\nDTEND;VALUE=DATE:20261007\r\nSUMMARY:Tea\\, café\\; \\\\日本\r\n 語\r\nDESCRIPTION:First\\nSecond\r\nLOCATION:Room\\, 2\r\nURL:https://example.org/a,b",
        chrono_tz::UTC,
    );
    assert_eq!(d.title, "Tea, café; \\日本語");
    assert_eq!(
        d.description,
        "First\nSecond\n\nLocation: Room, 2\n\nURL: https://example.org/a,b"
    );
    assert_eq!(
        d.timing,
        EventTiming::AllDay {
            start: "2026-10-04".parse().unwrap(),
            end_exclusive: "2026-10-07".parse().unwrap()
        }
    );
    let d = draft(
        "DTSTART;VALUE=DATE:20261004\r\nDURATION:P2D",
        chrono_tz::America::Denver,
    );
    assert!(
        matches!(d.timing, EventTiming::AllDay {end_exclusive,..} if end_exclusive.to_string()=="2026-10-06")
    );
}

#[test]
fn calendar_ical_utc_zoned_floating_and_repeated_times() {
    for (properties, expected) in [
        ("DTSTART:20261004T153027Z", "2026-10-04T15:30:27Z"),
        (
            "DTSTART;TZID=America/Denver:20261004T093027",
            "2026-10-04T15:30:27Z",
        ),
        ("DTSTART:20261004T093027", "2026-10-04T15:30:27Z"),
        (
            "DTSTART;TZID=America/Denver:20261101T013000",
            "2026-11-01T07:30:00Z",
        ),
    ] {
        let d = draft(properties, chrono_tz::America::Denver);
        assert!(
            matches!(d.timing, EventTiming::Timed { start,.. } if start==expected.parse::<chrono::DateTime<Utc>>().unwrap())
        );
    }
    let d = draft(
        "DTSTART:20261004T150000Z\r\nDURATION:PT1H30M",
        chrono_tz::UTC,
    );
    assert!(
        matches!(d.timing, EventTiming::Timed { start,end:Some(end) } if end-start==Duration::minutes(90))
    );
    let d = draft(
        "DTSTART;TZID=America/Denver:20260307T120000\r\nDURATION:P1DT1H",
        chrono_tz::UTC,
    );
    assert!(
        matches!(d.timing, EventTiming::Timed { start,end:Some(end) } if end-start==Duration::hours(24))
    );
}

#[test]
fn calendar_ical_rejects_unsupported_and_invalid_events_without_losing_other_choices() {
    for properties in [
        "DTSTART;VALUE=DATE:20261004\r\nDTEND:20261005T000000Z",
        "DTSTART:20261004T150000Z\r\nDTEND:20261004T140000Z",
        "DTSTART:20261004T150000Z\r\nDTEND:20261004T160000Z\r\nDURATION:PT1H",
        "DTSTART;TZID=Made/Up:20261004T150000",
        "DTSTART;TZID=America/Denver:20260308T023000",
        "DTSTART;VALUE=DATE:20261004\r\nDURATION:PT1H",
        "DTSTART:20261004T150000Z\r\nRRULE:FREQ=DAILY",
        "DTSTART:20261004T150000Z\r\nSTATUS:CANCELLED",
        "DTSTART:20261004T150000Z\r\nDURATION:P999999999999999999W",
        "DTSTART:20261004T150000Z\r\nSUMMARY:Bad\u{1b}text",
    ] {
        assert!(
            parse(&content(properties), chrono_tz::UTC).unwrap()[0]
                .draft
                .is_err(),
            "{properties}"
        );
    }
    assert!(parse("not a calendar", chrono_tz::UTC).is_err());
    assert!(parse(&"x".repeat(MAX_BYTES + 1), chrono_tz::UTC).is_err());
    let content = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Bad\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:Good\nDTSTART;VALUE=DATE:20261004\nEND:VEVENT\nEND:VCALENDAR";
    let events = parse(content, chrono_tz::UTC).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events[0].draft.is_err());
    assert!(events[1].draft.is_ok());
}

#[test]
fn calendar_ical_export_round_trip_is_utf8_folded_and_uses_exclusive_dates() {
    let start = Utc.with_ymd_and_hms(2026, 10, 4, 15, 30, 27).unwrap();
    let mut event = CalendarEvent {
        id: Uuid::nil(),
        owner_id: Some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creation_tier: CreationTier::User,
        mod_editable: false,
        title: "café, 日本語; \\".repeat(15),
        description: format!("{}  ", "First\nSecond,with;slashes\\\n🗓".repeat(40)),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::minutes(45)),
        },
        creator_timezone: "America/Denver".into(),
        notice_lead_seconds: Some(900),
        notice_start: start,
        notice_end: start + Duration::hours(1),
        revision: 4,
    };
    for timing in [
        event.timing.clone(),
        EventTiming::AllDay {
            start: "2026-10-04".parse().unwrap(),
            end_exclusive: "2026-10-07".parse().unwrap(),
        },
    ] {
        event.timing = timing;
        let ics = export(&event);
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
        assert!(ics.split("\r\n").all(|line| line.len() <= 75));
        assert!(!ics.replace("\r\n", "").contains(['\n', '\r']));
        let imported = parse(&ics, chrono_tz::UTC)
            .unwrap()
            .remove(0)
            .draft
            .unwrap();
        assert_eq!(imported.title, event.title);
        assert_eq!(imported.description, event.description);
        assert_eq!(imported.timing, event.timing);
        assert_eq!(imported.notice_lead_seconds, Some(900));
    }
}

#[test]
fn calendar_ical_url_schemes_and_single_event_fragment() {
    assert_eq!(
        import_url("webcal://example.org/calendar.ics")
            .unwrap()
            .unwrap(),
        "https://example.org/calendar.ics"
    );
    assert!(
        import_url("https://example.org/a?secret=123")
            .unwrap()
            .is_some()
    );
    assert!(
        import_url("BEGIN:VCALENDAR\nEND:VCALENDAR")
            .unwrap()
            .is_none()
    );
    for s in [
        "file:///tmp/calendar.ics",
        "ftp://example.org/cal",
        "https://user:pass@example.org/cal",
        "not a URL",
    ] {
        assert!(import_url(s).is_err());
    }
    assert_eq!(
        parse(
            "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261004\nEND:VEVENT",
            chrono_tz::UTC
        )
        .unwrap()
        .len(),
        1
    );
    let nested = format!("{}{}", "BEGIN:X\n".repeat(17), "END:X\n".repeat(17));
    assert!(
        parse(&nested, chrono_tz::UTC)
            .unwrap_err()
            .to_string()
            .contains("nested")
    );
}
