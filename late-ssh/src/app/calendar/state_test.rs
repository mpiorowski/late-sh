use super::{
    navigation::Selection,
    state::*,
    svc::{CalendarService, Reply, Snapshot},
};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::*,
};
use uuid::Uuid;

fn date(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn service() -> CalendarService {
    CalendarService::new(Db::new(&DbConfig::default()).unwrap())
}

fn event(id: u128, start: chrono::DateTime<Utc>, board: bool) -> CalendarEvent {
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: (!board).then_some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creator_name: "mat".into(),
        title: format!("Event {id}"),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(1)),
        },
        creator_timezone: "UTC".into(),
        starts_at: start,
        ends_at: start + Duration::hours(1),
        going: 0,
        revision: 1,
    }
}

#[test]
fn month_and_week_boundaries() {
    assert_eq!(shift_month(date("2028-01-31"), 1), date("2028-02-29"));
    assert_eq!(shift_month(date("2026-01-31"), 1), date("2026-02-28"));
    assert_eq!(shift_month(date("2026-01-01"), -1), date("2025-12-01"));
    assert_eq!(week_start(date("2026-10-04")), date("2026-09-28"));
    assert_eq!(week_start(date("2026-09-28")), date("2026-09-28"));
}

/// A reply for a load the session has moved past is dropped, and the
/// reply that does land carries the viewer's role and what they are in.
#[tokio::test]
async fn stale_loads_are_ignored_and_a_fresh_load_lands_whole() {
    let mut s = CalendarState::new(service(), Uuid::now_v7());
    let stale = s.generation;
    s.selected += Duration::days(40);
    s.refresh();
    let snapshot = |staff| Snapshot {
        events: Vec::new(),
        rsvps: vec![Uuid::from_u128(1)],
        staff,
    };
    assert!(!s.apply(Reply::Loaded {
        generation: stale,
        result: Ok(snapshot(true)),
    }));
    assert!(!s.staff);
    assert!(s.apply(Reply::Loaded {
        generation: s.generation,
        result: Ok(snapshot(true)),
    }));
    assert!(s.staff);
    assert!(s.going(Uuid::from_u128(1)));
    s.hits.borrow_mut().push(Hit {
        area: ratatui::layout::Rect::new(2, 3, 4, 5),
        action: Action::Today,
    });
    s.invalidate_geometry();
    assert!(s.hits.borrow().is_empty());
}

/// An "I'm in" answer flips the viewer's side and carries the count into
/// every copy of the event the session holds.
#[tokio::test]
async fn an_rsvp_reply_flips_the_viewer_and_updates_the_count_everywhere() {
    let mut s = CalendarState::new(service(), Uuid::now_v7());
    s.loading = false;
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 21, 0, 0).unwrap();
    let e = event(1, start, true);
    s.events.push(e.clone());
    s.modal = Some(Modal::Details(e.clone()));
    let mut counted = e.clone();
    counted.going = 4;
    assert!(s.apply(Reply::Rsvp(Ok(counted.clone()))));
    assert!(s.going(e.id));
    assert_eq!(s.events[0].going, 4);
    assert!(matches!(&s.modal, Some(Modal::Details(open)) if open.going == 4));
    counted.going = 3;
    s.apply(Reply::Rsvp(Ok(counted)));
    assert!(!s.going(e.id));
    assert_eq!(s.events[0].going, 3);
    assert!(!s.pending);
}

/// Live is the board's shared snapshot minus what has ended since it was
/// read; the session never adds its own events to it.
#[tokio::test]
async fn live_is_the_shared_snapshot_minus_what_ended() {
    let service = service();
    let mut s = CalendarState::new(service.clone(), Uuid::now_v7());
    let now = Utc::now();
    let soon = event(1, now + Duration::minutes(30), true);
    let on = event(2, now - Duration::minutes(10), true);
    let over = event(3, now - Duration::hours(3), true);
    service.publish_board(vec![soon.clone(), on.clone(), over]);
    s.tick(false, chrono_tz::UTC);
    let ids: Vec<_> = s.live().into_iter().map(|e| e.id).collect();
    assert_eq!(
        ids,
        vec![soon.id, on.id],
        "the snapshot's order, the over one gone"
    );
}

/// Opening an event from the strip lands on its day with its details up,
/// over whatever modal was open.
#[tokio::test]
async fn show_event_lands_on_its_day_with_details_open() {
    let mut s = CalendarState::new(service(), Uuid::now_v7());
    s.loading = false;
    s.selected = date("2026-10-02");
    s.modal = Some(Modal::Delete(event(
        8,
        Utc.with_ymd_and_hms(2026, 10, 2, 20, 0, 0).unwrap(),
        true,
    )));
    let e = event(
        9,
        Utc.with_ymd_and_hms(2026, 11, 14, 20, 0, 0).unwrap(),
        true,
    );
    s.show_event(e.clone());
    assert_eq!(s.selected, date("2026-11-14"));
    assert_eq!(s.selection, Selection::Event(e.id));
    assert!(matches!(&s.modal, Some(Modal::Details(open)) if open.id == e.id));
    assert_eq!(
        s.modal_parents.len(),
        1,
        "the page is the only frame under it"
    );
}
