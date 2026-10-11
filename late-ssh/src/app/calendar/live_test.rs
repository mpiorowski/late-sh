use super::live::*;
use crate::app::live::{pick::LiveSource, ui::PICTURE_COLS};
use chrono::{Duration, TimeZone, Utc};
use late_core::models::calendar::{CalendarEvent, EventTiming, STRIP_LEAD};
use uuid::Uuid;

fn event(id: u128, start: chrono::DateTime<Utc>, going: i64) -> CalendarEvent {
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: None,
        creator_id: Uuid::nil(),
        creator_name: "mat".into(),
        title: "Movie night".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(2)),
        },
        creator_timezone: "UTC".into(),
        starts_at: start,
        ends_at: start + Duration::hours(2),
        going,
        revision: 1,
    }
}

fn text(spans: &[ratatui::text::Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn line_text(line: &ratatui::text::Line<'_>) -> String {
    text(&line.spans)
}

/// An all-day post on `day`, in UTC.
fn all_day(id: u128, day: chrono::NaiveDate, title: &str) -> CalendarEvent {
    let start = day.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let mut e = event(id, start, 0);
    e.title = title.into();
    e.timing = EventTiming::AllDay {
        start: day,
        end_exclusive: day.succ_opt().unwrap(),
    };
    e.ends_at = start + Duration::days(1);
    e
}

/// A timed post joins the strip an hour before it starts, stamped then,
/// and joins again at its start, stamped then; before the hour and after
/// the end it is not offered. An all-day post on its day is never offered.
#[test]
fn an_event_joins_an_hour_before_and_again_at_its_start() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let soon = event(1, now + Duration::minutes(30), 2);
    let on = event(2, now - Duration::minutes(10), 0);
    let far = event(3, now + Duration::hours(3), 0);
    let over = event(4, now - Duration::hours(3), 0);
    let day = all_day(5, now.date_naive(), "Hackathon");
    let live = [soon.clone(), on.clone(), far, over, day];
    let candidates = candidates(&live, now);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].source, LiveSource::BoardEvent(soon.id));
    assert_eq!(candidates[0].updated, soon.starts_at - STRIP_LEAD);
    assert_eq!(candidates[1].source, LiveSource::BoardEvent(on.id));
    assert_eq!(candidates[1].updated, on.starts_at);
    assert!(view(&live, Uuid::from_u128(4), chrono_tz::UTC, now).is_none());
    assert!(view(&live, Uuid::from_u128(3), chrono_tz::UTC, now).is_none());
    assert!(view(&live, Uuid::from_u128(5), chrono_tz::UTC, now).is_none());
}

/// The Live panel lists the timed posts with their start as a clock time
/// in the viewer's zone; the all-day post is not a panel row.
#[test]
fn panel_rows_say_the_start_in_the_viewers_zone() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let soon = event(1, now + Duration::minutes(30), 2);
    let day = all_day(5, now.date_naive(), "Hackathon");
    let rows = panel_events(&[soon.clone(), day], chrono_tz::Europe::Warsaw);
    assert_eq!(
        rows,
        vec![PanelEvent {
            id: soon.id,
            title: "Movie night".into(),
            time: "22:30".into(),
            going: 2,
        }]
    );
}

/// The topic row reads the day's all-day posts, in the order given, and
/// is absent on a day without one or once the day is over.
#[test]
fn the_days_all_day_posts_make_the_topic_row() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let today = now.date_naive();
    let live = [
        event(1, now + Duration::minutes(30), 2),
        all_day(5, today, "Hackathon"),
        all_day(6, today, "Community call"),
        all_day(7, today.succ_opt().unwrap(), "Tomorrow"),
    ];
    assert_eq!(
        day_text(&live, now).as_deref(),
        Some("\u{1F4C5} Hackathon · Community call")
    );
    assert_eq!(
        day_text(&live, now + Duration::days(1)).as_deref(),
        Some("\u{1F4C5} Tomorrow")
    );
    assert_eq!(day_text(&live[..1], now), None);
}

/// What the strip says: how long until it starts, that it is on, how
/// many are in, and who posted it; the viewer's own event says so.
#[test]
fn the_words_say_when_how_many_and_who() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let soon = event(1, now + Duration::minutes(30), 2);
    let strip = view(&[soon], Uuid::from_u128(1), chrono_tz::UTC, now).unwrap();
    assert_eq!(when(&strip), "in 30m");
    assert!(!glow(&strip));
    let painted = body(40, &strip);
    assert_eq!(text(&painted.words[1]), "Movie night");
    assert_eq!(text(&painted.words[2]), "in 30m · 2 in");
    assert_eq!(text(&painted.words[3]), "mat posted it");
    assert_eq!(painted.picture.len(), 7);
    for line in &painted.picture {
        assert_eq!(line.width(), usize::from(PICTURE_COLS));
    }
    assert!(line_text(&painted.picture[1]).contains("OCT"));
    assert!(line_text(&painted.picture[5]).contains("20:30"));
    assert_eq!(
        text(&compact_spans(60, &strip)),
        "event mat · Movie night · in 30m"
    );

    let on = event(2, now - Duration::minutes(10), 0);
    let strip = view(&[on], Uuid::from_u128(2), chrono_tz::UTC, now).unwrap();
    assert_eq!(when(&strip), "on now");
    assert!(glow(&strip));
    let painted = body(40, &strip);
    assert_eq!(text(&painted.words[2]), "on now · nobody's in yet");
    assert_eq!(text(&painted.words[3]), "mat posted it");
    assert_eq!(
        text(&compact_spans(60, &strip)),
        "event mat · Movie night · on now"
    );
}
