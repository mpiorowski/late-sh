use super::{
    navigation::{ClickTarget, MenuAction, Selection},
    state::{CalendarState, Editor, Modal},
    svc::CalendarService,
};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::{CalendarEvent, CalendarSource, CalendarView, EventTiming},
};
use std::time::Instant;
use uuid::Uuid;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
}

fn viewer() -> Uuid {
    Uuid::from_u128(0xA)
}

fn event(id: u128, day: u32, hour: u32) -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: Some(viewer()),
        creator_id: viewer(),
        creator_name: "me".into(),
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

fn board_event(id: u128, day: u32, hour: u32, poster: Uuid) -> CalendarEvent {
    let mut e = event(id, day, hour);
    e.owner_id = None;
    e.creator_id = poster;
    e.creator_name = "mat".into();
    e
}

fn state() -> CalendarState {
    let mut state = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        viewer(),
    );
    state.loading = false;
    state.selected = date(2);
    state.list_rows.set(5);
    state.agenda_rows.set(6);
    state
}

/// Landing on a day marks its first event, so Enter and the action keys
/// have a target; an empty day has none.
#[tokio::test]
async fn landing_on_a_day_marks_its_first_event() {
    let mut s = state();
    let later = event(1, 2, 21);
    let first = event(2, 2, 9);
    s.events = vec![later.clone(), first.clone()];
    assert!(s.selected_target_event().is_none());
    s.select_date(date(2));
    assert_eq!(s.selected_target_event().unwrap().id, first.id);
    assert_eq!(s.selection, Selection::Event(first.id));
    s.select_date(date(3));
    assert!(s.selected_target_event().is_none());
    assert_eq!(s.selection, Selection::Date);
}

#[tokio::test]
async fn selection_follows_uuid_and_chooses_neighbor_after_removal() {
    let mut s = state();
    s.events = vec![event(1, 2, 9), event(2, 2, 10), event(3, 2, 11)];
    s.select_event(Uuid::from_u128(2), None);
    s.events.sort_by_key(|e| std::cmp::Reverse(e.id));
    s.reconcile_selection();
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(2)));
    assert_eq!(s.event_index, 1, "the order is by time, not storage");
    s.push_modal(Modal::Delete(s.events[1].clone()));
    s.finish_delete(Uuid::from_u128(2));
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(3)));
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn event_selection_keeps_the_clicked_overnight_segment() {
    let mut s = state();
    let mut e = event(1, 2, 23);
    e.timing = EventTiming::Timed {
        start: Utc.with_ymd_and_hms(2026, 10, 2, 23, 0, 0).unwrap(),
        end: Some(Utc.with_ymd_and_hms(2026, 10, 3, 2, 0, 0).unwrap()),
    };
    s.events.push(e.clone());
    s.select_event(e.id, Some(date(3)));
    assert_eq!(s.selected, date(3));
    s.select_date(date(5));
    s.select_event(e.id, None);
    assert_eq!(
        s.selected,
        date(2),
        "away from it, selection lands on its start"
    );
}

#[tokio::test]
async fn nested_modals_restore_each_browser_cursor_and_scroll() {
    let mut s = state();
    s.view = CalendarView::List;
    s.events = (1..=8).map(|n| event(n, n as u32, 9)).collect();
    s.select_event(Uuid::from_u128(6), None);
    let page_scroll = s.scroll;
    s.push_modal(Modal::Details(s.events[5].clone()));
    s.push_modal(Modal::Delete(s.events[5].clone()));
    s.scroll = 3;
    s.pop_modal();
    assert!(matches!(s.modal, Some(Modal::Details(_))));
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(6)));
    s.pop_modal();
    assert!(s.modal.is_none());
    assert_eq!(s.selected, date(6));
    assert_eq!(s.scroll, page_scroll);
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(6)));
}

#[tokio::test]
async fn a_saved_editor_returns_to_updated_details_without_duplicate_history() {
    let mut s = state();
    let e = event(1, 2, 9);
    s.events.push(e.clone());
    s.select_event(e.id, None);
    s.push_modal(Modal::Details(e.clone()));
    s.push_modal(Modal::Editor(Box::new(Editor::from_event(
        &e,
        viewer(),
        false,
        chrono_tz::UTC,
    ))));
    let mut saved = e.clone();
    saved.title = "Renamed".into();
    saved.revision = 2;
    s.replace_saved(saved);
    assert!(matches!(&s.modal, Some(Modal::Details(open)) if open.title == "Renamed"));
    assert_eq!(s.modal_parents.len(), 1);
    assert_eq!(s.events[0].revision, 2);
    s.pop_modal();
    assert!(s.modal.is_none());
    assert!(s.modal_parents.is_empty());
}

#[tokio::test]
async fn double_click_requires_the_same_target_and_surface() {
    let s = state();
    let now = Instant::now();
    let target = ClickTarget::Date(date(2));
    assert!(!s.register_click(target, now));
    assert!(s.register_click(target, now));
    assert!(!s.register_click(target, now), "the double was spent");
    assert!(!s.register_click(ClickTarget::Date(date(3)), now));
    assert!(!s.register_click(
        ClickTarget::Date(date(3)),
        now + std::time::Duration::from_secs(2)
    ));
}

/// The menu offers what the viewer may do: the poster edits and deletes,
/// anyone says they are in or out, staff delete a stranger's post, and a
/// personal event takes no "I'm in".
#[tokio::test]
async fn context_menus_follow_the_three_rules() {
    let mut s = state();
    let stranger = Uuid::from_u128(0xB);
    let own_post = board_event(1, 2, 9, viewer());
    let their_post = board_event(2, 2, 10, stranger);
    let mine = event(3, 2, 11);
    s.events = vec![own_post.clone(), their_post.clone(), mine.clone()];
    s.rsvps.push(their_post.id);

    s.open_context_menu(ClickTarget::Event(own_post.id), (0, 0));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        vec![
            MenuAction::Open,
            MenuAction::Rsvp(true),
            MenuAction::Edit,
            MenuAction::Delete
        ]
    );
    s.open_context_menu(ClickTarget::Event(their_post.id), (0, 0));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        vec![MenuAction::Open, MenuAction::Rsvp(false)],
        "already in, so the menu offers out; not theirs to edit"
    );
    s.staff = true;
    s.open_context_menu(ClickTarget::Event(their_post.id), (0, 0));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        vec![
            MenuAction::Open,
            MenuAction::Rsvp(false),
            MenuAction::Delete
        ],
        "staff take a post down but do not edit its words"
    );
    s.open_context_menu(ClickTarget::Event(mine.id), (0, 0));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        vec![MenuAction::Open, MenuAction::Edit, MenuAction::Delete]
    );
    s.open_context_menu(ClickTarget::Date(date(4)), (0, 0));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        vec![MenuAction::New]
    );
    assert_eq!(s.selected, date(4));
    assert!(s.modal_parents.is_empty(), "a menu is not a modal frame");
    let _ = CalendarSource::Board;
}
