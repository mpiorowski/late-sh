use super::{
    editor::EditorControl,
    state::*,
    svc::{CalendarService, Reply, Snapshot},
};
use chrono::{Duration, NaiveDate};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::*,
};
use uuid::Uuid;
fn date(s: &str) -> NaiveDate {
    s.parse().unwrap()
}
fn editor() -> Editor {
    Editor::new(
        CalendarSource::Personal(Uuid::nil()),
        date("2026-10-02"),
        EventAccess {
            edit: true,
            notifications: true,
            delegate: false,
        },
    )
}
#[test]
fn calendar_editor_infers_once_and_preserves_invalid_draft() {
    let mut e = editor();
    e.fields[0].insert_str("Tea tomorrow at 4pm");
    e.blur_title(date("2026-10-02"), chrono_tz::UTC);
    assert_eq!(e.text(0), "Tea");
    assert_eq!(e.text(2), "2026-10-03");
    assert_eq!(e.text(3), "16:00");
    assert!(e.ever_assigned);
    e.fields[0].insert_str(" tomorrow");
    e.fields[2] = ratatui_textarea::TextArea::default();
    e.blur_title(date("2026-10-02"), chrono_tz::UTC);
    assert_eq!(e.text(0), "Tea tomorrow");
    assert!(e.draft(date("2026-10-02"), chrono_tz::UTC).is_err());
    assert_eq!(e.text(0), "Tea tomorrow");
    let mut e = editor();
    e.fields[0].insert_str("Leap Feb 29 2028");
    let d = e.draft(date("2026-10-02"), chrono_tz::UTC).unwrap();
    assert!(
        matches!(d.timing,EventTiming::AllDay{end_exclusive,..} if end_exclusive==date("2028-03-01"))
    );
    e.ever_assigned = true;
    e.fields[0].insert_str(" tomorrow");
    e.blur_title(date("2026-10-02"), chrono_tz::UTC);
    assert_eq!(e.text(0), "Leap tomorrow");
}
#[test]
fn calendar_month_week_and_duration_boundaries() {
    assert_eq!(shift_month(date("2028-01-31"), 1), date("2028-02-29"));
    assert_eq!(shift_month(date("2026-01-31"), 1), date("2026-02-28"));
    assert_eq!(shift_month(date("2026-01-01"), -1), date("2025-12-01"));
    assert_eq!(week_start(date("2026-10-04"), 0), date("2026-09-28"));
    assert_eq!(week_start(date("2026-10-04"), 6), date("2026-10-04"));
    assert_eq!(parse_duration("24h").unwrap(), 86400);
    assert_eq!(parse_duration("0m").unwrap(), 0);
    for (input, expected) in [
        ("24 hours", 86400),
        ("1 day", 86400),
        ("1h 30m", 5400),
        ("2 weeks 3 days", 17 * 86400),
        ("0", 0),
        ("3600", 3600),
        ("1500ms 500ms", 2),
        ("3650d", 315360000),
    ] {
        assert_eq!(parse_duration(input).unwrap(), expected, "{input}");
    }
    for s in [
        "-1h",
        "forever",
        "99999999999999999999d",
        "3651d",
        "1ms",
        "1.5s",
    ] {
        assert!(parse_duration(s).is_err());
    }
}

#[test]
fn calendar_editor_normalizes_human_dates_on_blur_and_save() {
    let today = date("2026-10-02");
    let mut e = editor();
    e.fields[0].insert_str("Trip");
    e.ever_assigned = true;
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["2 months ago".to_string()]);
    e.focus = EditorControl::StartDate;
    e.focus(EditorControl::EndDate, today, chrono_tz::UTC);
    assert_eq!(e.text(2), "2026-08-02");
    assert!(e.error.is_none());
    e.fields[4] = ratatui_textarea::TextArea::from(vec!["Oct 3rd, 2026".to_string()]);
    e.notifications = true;
    e.fields[6] = ratatui_textarea::TextArea::from(vec!["1 day 2 hours".to_string()]);
    let draft = e.draft(today, chrono_tz::UTC).unwrap();
    assert_eq!(e.text(4), "2026-10-03");
    assert_eq!(
        draft.timing,
        EventTiming::AllDay {
            start: date("2026-08-02"),
            end_exclusive: date("2026-10-04"),
        }
    );
    assert_eq!(draft.notice_lead_seconds, Some(26 * 3600));
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["02/03/2026".to_string()]);
    e.focus = EditorControl::StartDate;
    e.focus(EditorControl::EndDate, today, chrono_tz::UTC);
    assert!(e.error.as_deref().unwrap().contains("Ambiguous"));
    assert_eq!(e.text(2), "02/03/2026");
    assert!(e.draft(today, chrono_tz::UTC).is_err());
    assert_eq!(e.text(0), "Trip");
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["Oct 2".to_string()]);
    e.focus = EditorControl::StartDate;
    e.focus(EditorControl::EndDate, today, chrono_tz::UTC);
    assert!(e.error.is_none());
    e.error = Some("Revision conflict; reload before saving".into());
    e.focus = EditorControl::StartDate;
    e.focus(EditorControl::EndDate, today, chrono_tz::UTC);
    assert!(e.error.as_deref().unwrap().contains("Revision conflict"));
}

#[test]
fn calendar_editor_requires_dst_choice_and_rejects_overnight_invalid_end() {
    let mut e = editor();
    e.fields[0].insert_str("DST appointment");
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["2026-11-01".to_string()]);
    e.fields[3] = ratatui_textarea::TextArea::from(vec!["01:30".to_string()]);
    e.all_day = false;
    e.ever_assigned = true;
    let tz = chrono_tz::America::New_York;
    assert!(e.draft(date("2026-10-02"), tz).is_err());
    e.occurrence = Some(Occurrence::Earlier);
    let a = e.draft(date("2026-10-02"), tz).unwrap();
    e.occurrence = Some(Occurrence::Later);
    let b = e.draft(date("2026-10-02"), tz).unwrap();
    assert_eq!(
        b.timing.bounds(tz).unwrap().0 - a.timing.bounds(tz).unwrap().0,
        Duration::hours(1)
    );
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["2026-03-08".to_string()]);
    e.fields[3] = ratatui_textarea::TextArea::from(vec!["02:30".to_string()]);
    assert!(e.draft(date("2026-10-02"), tz).is_err());
    assert_eq!(e.text(0), "DST appointment");
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["2026-11-01".to_string()]);
    e.fields[3] = ratatui_textarea::TextArea::from(vec!["01:45".to_string()]);
    e.fields[4] = ratatui_textarea::TextArea::from(vec!["2026-11-01".to_string()]);
    e.fields[5] = ratatui_textarea::TextArea::from(vec!["01:15".to_string()]);
    e.occurrence = Some(Occurrence::Earlier);
    e.end_occurrence = Some(Occurrence::Later);
    let folded = e.draft(date("2026-10-02"), tz).unwrap();
    let bounds = folded.timing.bounds(tz).unwrap();
    assert_eq!(bounds.1 - bounds.0, Duration::minutes(30));
    e.set_timing(&folded.timing, tz);
    assert_eq!(e.occurrence, Some(Occurrence::Earlier));
    assert_eq!(e.end_occurrence, Some(Occurrence::Later));
    e.fields[2] = ratatui_textarea::TextArea::from(vec!["2026-10-02".to_string()]);
    e.fields[3] = ratatui_textarea::TextArea::from(vec!["23:30".to_string()]);
    e.fields[4] = ratatui_textarea::TextArea::default();
    e.fields[5] = ratatui_textarea::TextArea::from(vec!["01:30".to_string()]);
    assert!(e.draft(date("2026-10-02"), tz).is_err());
    e.fields[4] = ratatui_textarea::TextArea::from(vec!["2026-10-03".to_string()]);
    assert!(e.draft(date("2026-10-02"), tz).is_ok());
    assert_eq!(effective_timezone(Some("invalid zone")), chrono_tz::UTC);
}
#[tokio::test]
async fn calendar_stale_responses_and_resize_geometry() {
    let viewer = Uuid::now_v7();
    let db = Db::new(&DbConfig::default()).unwrap();
    let mut s = CalendarState::new(CalendarService::new(db), viewer);
    let generation = s.generation;
    s.selected += Duration::days(40);
    s.refresh();
    assert!(!s.apply(Reply::Loaded {
        generation,
        result: Ok(Snapshot {
            events: Vec::new(),
            event_error: None,
            personal_notices: Vec::new(),
            public: Vec::new(),
            preferences: CalendarPreferences {
                public: true,
                ..Default::default()
            },
            role: CreationTier::Admin
        })
    }));
    assert!(!s.preferences.public);
    s.hits.borrow_mut().push(Hit {
        area: ratatui::layout::Rect::new(2, 3, 4, 5),
        action: Action::Today,
    });
    s.invalidate_geometry();
    assert!(s.hits.borrow().is_empty());
}
