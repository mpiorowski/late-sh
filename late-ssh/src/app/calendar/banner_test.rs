use super::banner::*;
use chrono::{Duration, TimeZone, Utc};
use late_core::models::calendar::{CalendarEvent, EventTiming};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

/// A two-hour timed post.
fn timed(id: u128, start: chrono::DateTime<Utc>, title: &str) -> CalendarEvent {
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: None,
        creator_id: Uuid::nil(),
        creator_name: "mat".into(),
        title: title.into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(2)),
        },
        creator_timezone: "UTC".into(),
        starts_at: start,
        ends_at: start + Duration::hours(2),
        going: 0,
        revision: 1,
    }
}

/// An all-day post on `day`, in UTC.
fn all_day(id: u128, day: chrono::NaiveDate, title: &str) -> CalendarEvent {
    let start = day.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let mut e = timed(id, start, title);
    e.timing = EventTiming::AllDay {
        start: day,
        end_exclusive: day.succ_opt().unwrap(),
    };
    e.ends_at = start + Duration::days(1);
    e
}

fn post(title: &str, time: Option<(&str, bool)>) -> DayPost {
    DayPost {
        title: title.into(),
        when: match time {
            None => DayWhen::AllDay,
            Some((time, over)) => DayWhen::At {
                time: time.into(),
                over,
            },
        },
    }
}

fn row(posts: &[DayPost], width: usize) -> String {
    let text: String = day_spans(posts, width)
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(text.width() <= width, "{text:?} is wider than {width}");
    text
}

/// The row lists every board post on the viewer's today for the whole day:
/// the all-day ones first, then the timed ones soonest first with their
/// start in the viewer's zone, the ended one still there and marked over.
/// Tomorrow's posts are not on it.
#[test]
fn the_row_lists_todays_posts_all_day_first_then_by_start() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let today = now.date_naive();
    let board = [
        timed(1, now - Duration::hours(6), "Standup"),
        all_day(2, today, "Hackathon"),
        timed(3, now - Duration::minutes(10), "Movie night"),
        timed(4, now + Duration::hours(2), "Late show"),
        all_day(5, today.succ_opt().unwrap(), "Tomorrow"),
        timed(6, now + Duration::hours(5), "After midnight"),
    ];
    assert_eq!(
        day_posts(&board, chrono_tz::UTC, now),
        vec![
            post("Hackathon", None),
            post("Standup", Some(("14:00", true))),
            post("Movie night", Some(("19:50", false))),
            post("Late show", Some(("22:00", false))),
        ]
    );
    assert_eq!(day_posts(&board[4..], chrono_tz::UTC, now), vec![]);
}

/// A viewer's today is their zone's: the post just past UTC midnight is
/// tomorrow's in UTC and today's two hours east, where its start reads in
/// local time.
#[test]
fn today_is_the_viewers_zone() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let board = [timed(6, now + Duration::hours(5), "After midnight")];
    assert_eq!(day_posts(&board, chrono_tz::UTC, now), vec![]);
    assert_eq!(
        day_posts(&board, chrono_tz::Europe::Warsaw, now + Duration::hours(3)),
        vec![post("After midnight", Some(("03:00", false)))]
    );
}

/// With room for everything the row is the titles in full, a timed post
/// with its start after it.
#[test]
fn a_roomy_row_says_every_post_in_full() {
    let posts = [
        post("Hackathon", None),
        post("KubeCon NA", Some(("17:00", true))),
        post("Rust 1.100 rls", Some(("22:00", false))),
    ];
    assert_eq!(
        row(&posts, 80),
        "\u{1F4C5} Hackathon · KubeCon NA (17:00) · Rust 1.100 rls (22:00)"
    );
}

/// Short of room the titles share what there is: a short one keeps all of
/// itself and hands its spare columns to the long ones, which are cut with
/// an ellipsis. The times are never cut.
#[test]
fn a_tight_row_cuts_the_long_titles_and_keeps_the_times() {
    let posts = [
        post("Tea", None),
        post("A very long community call", Some(("17:00", false))),
        post("Another long release party", Some(("22:00", false))),
    ];
    assert_eq!(
        row(&posts, 48),
        "\u{1F4C5} Tea · A very lo… (17:00) · Another l… (22:00)"
    );
}

/// A day too crowded for every title to keep four letters and the ellipsis
/// folds its last posts into a count, as few as it takes.
#[test]
fn a_crowded_row_folds_the_last_posts_into_a_count() {
    let posts: Vec<DayPost> = ["Alpha one", "Bravo two", "Charlie three", "Delta four"]
        .into_iter()
        .map(|title| post(title, Some(("09:00", false))))
        .collect();
    assert_eq!(
        row(&posts, 38),
        "\u{1F4C5} Alph… (09:00) · Bravo… (09:00) · +2"
    );
    assert_eq!(
        row(&posts, 56),
        "\u{1F4C5} Alpha… (09:00) · Bravo… (09:00) · Charl… (09:00) · +1"
    );
}

/// A post that is over reads dimmer than one still to come.
#[test]
fn an_ended_post_is_dim() {
    let posts = [
        post("Standup", Some(("14:00", true))),
        post("Movie night", Some(("21:00", false))),
    ];
    let spans = day_spans(&posts, 80);
    let style_of = |title: &str| {
        spans
            .iter()
            .find(|span| span.content.starts_with(title))
            .unwrap()
            .style
    };
    assert_ne!(style_of("Standup"), style_of("Movie night"));
    assert_eq!(
        style_of("Standup").fg,
        Some(crate::app::common::theme::AMBER_DIM())
    );
}
