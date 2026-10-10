use super::{
    navigation::{ClickTarget, MenuAction, Selection},
    state::{CalendarState, Editor, Modal},
    svc::CalendarService,
};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::{CalendarEvent, CalendarSource, CalendarView, CreationTier, EventTiming},
};
use std::time::Instant;
use uuid::Uuid;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
}

fn event(id: u128, day: u32, hour: u32) -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: Some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creation_tier: CreationTier::User,
        mod_editable: false,
        title: format!("Event {id}"),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(1)),
        },
        creator_timezone: "UTC".into(),
        notice_lead_seconds: None,
        notice_start: start,
        notice_end: start + Duration::hours(1),
        revision: 1,
    }
}

fn state() -> CalendarState {
    let mut state = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    state.loading = false;
    state.selected = date(2);
    state.source = CalendarSource::Personal(state.viewer);
    state.list_rows.set(5);
    state.agenda_rows.set(6);
    state
}

#[tokio::test]
async fn calendar_date_and_slot_selection_never_target_an_event() {
    let mut s = state();
    let e = event(1, 2, 9);
    s.events.push(e.clone());
    assert!(s.selected_target_event().is_none());
    assert!(s.select_event(e.id, None, false));
    assert_eq!(s.selected_target_event().unwrap().id, e.id);
    s.select_date(date(2));
    assert!(s.selected_target_event().is_none());
    s.select_slot(date(2), 977);
    assert_eq!(s.selection, Selection::Slot(960));
    assert_eq!(s.slot_minute, 960);
    assert!(s.selected_target_event().is_none());
}

#[tokio::test]
async fn calendar_selection_follows_uuid_and_chooses_neighbor_after_removal() {
    let mut s = state();
    s.view = CalendarView::List;
    let a = event(1, 2, 9);
    let b = event(2, 3, 9);
    let c = event(3, 4, 9);
    s.events = vec![a.clone(), b.clone(), c.clone()];
    s.select_event(b.id, None, false);
    s.events.insert(0, event(4, 1, 9));
    s.events.reverse();
    s.reconcile_selection();
    assert_eq!(s.selection, Selection::Event(b.id));
    assert_eq!(s.event_index, 2);
    s.events.retain(|e| e.id != b.id);
    s.reconcile_selection();
    assert_eq!(s.selection, Selection::Event(c.id));
    s.events.clear();
    s.loading = true;
    s.reconcile_selection();
    assert_eq!(s.selection, Selection::Event(c.id));
    s.loading = false;
    s.reconcile_selection();
    assert_eq!(s.selection, Selection::Date);
}

#[tokio::test]
async fn calendar_event_selection_keeps_clicked_overnight_segment() {
    let mut s = state();
    s.view = CalendarView::Week;
    let mut e = event(1, 2, 23);
    if let EventTiming::Timed { end, .. } = &mut e.timing {
        *end = Some(Utc.with_ymd_and_hms(2026, 10, 3, 2, 0, 0).unwrap());
    }
    s.events.push(e.clone());
    s.hour_rows.set(6);
    s.select_event(e.id, Some(date(3)), false);
    assert_eq!(s.selected, date(3));
    assert_eq!(s.slot_minute, 0);
    assert_eq!(s.hour_scroll, 0);
    assert!(s.reveal_event.get());
    s.select_event(e.id, Some(date(2)), false);
    assert_eq!(s.slot_minute, 23 * 60);
    assert!(s.hour_scroll <= 46 && s.hour_scroll + s.hour_rows.get() > 46);
}

#[tokio::test]
async fn calendar_keyboard_selection_uses_actual_list_height() {
    let mut s = state();
    s.view = CalendarView::List;
    s.list_rows.set(3);
    s.events = (1..=6).map(|day| event(day.into(), day, 9)).collect();
    s.move_selected_event(1, false);
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(1)));
    s.move_selected_event(3, false);
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(4)));
    // Each event has a date header: event four is row seven, zero based.
    assert_eq!(s.scroll, 5);
    s.move_selected_event(-2, false);
    assert_eq!(s.scroll, 3);
}

#[tokio::test]
async fn calendar_nested_modals_restore_each_browser_cursor_and_scroll() {
    let mut s = state();
    s.view = CalendarView::List;
    let page_event = event(1, 2, 9);
    let mut notice = event(2, 4, 11);
    notice.notice_lead_seconds = Some(86400);
    notice.notice_start = Utc::now() - Duration::days(1);
    notice.notice_end = Utc::now() + Duration::days(1);
    s.events = vec![page_event.clone()];
    s.notices = vec![notice.clone()];
    s.select_event(page_event.id, None, false);
    s.scroll = 7;
    s.agenda_scroll = 9;
    s.hour_scroll = 35;
    s.day_scroll.set(12);
    s.picker_scroll.set(3);
    s.push_modal(Modal::Upcoming);
    s.select_event(notice.id, None, true);
    s.scroll = 4;
    s.push_modal(Modal::Details(notice.clone()));
    s.scroll = 12;
    s.pop_modal();
    assert!(matches!(s.modal, Some(Modal::Upcoming)));
    assert_eq!(s.selection, Selection::Event(notice.id));
    assert_eq!(s.selected, date(4));
    assert_eq!(s.scroll, 4);
    s.pop_modal();
    assert!(s.modal.is_none());
    assert_eq!(s.selection, Selection::Event(page_event.id));
    assert_eq!(s.selected, date(2));
    assert_eq!((s.scroll, s.agenda_scroll, s.hour_scroll), (7, 9, 35));
    assert_eq!(s.day_scroll.get(), 12);
    assert_eq!(s.picker_scroll.get(), 3);
    assert!(s.modal_parents.is_empty());
}

#[tokio::test]
async fn calendar_saved_editor_returns_to_updated_details_without_duplicate_history() {
    let mut s = state();
    let mut e = event(1, 2, 9);
    s.events.push(e.clone());
    s.select_event(e.id, None, false);
    s.push_modal(Modal::Details(e.clone()));
    s.scroll = 6;
    s.push_modal(Modal::Editor(Box::new(Editor::from_event(
        &e, s.viewer, s.role, s.tz,
    ))));
    e.title = "Edited title".into();
    e.revision += 1;
    s.replace_saved(e.clone());
    assert!(matches!(&s.modal, Some(Modal::Details(saved)) if saved == &e));
    assert_eq!(s.scroll, 6);
    assert_eq!(s.modal_parents.len(), 1);
    s.pop_modal();
    assert!(s.modal.is_none());
    assert_eq!(s.selected_target_event().unwrap().title, "Edited title");
}

#[tokio::test]
async fn calendar_delete_returns_to_browser_and_selects_next_event() {
    let mut s = state();
    s.view = CalendarView::List;
    let a = event(1, 2, 9);
    let b = event(2, 2, 10);
    s.events = vec![a.clone(), b.clone()];
    s.select_event(a.id, None, false);
    s.scroll = 3;
    s.push_modal(Modal::Details(a.clone()));
    s.push_modal(Modal::Delete(a.clone()));
    s.finish_delete(a.id);
    assert!(s.modal.is_none());
    assert!(s.modal_parents.is_empty());
    assert!(s.scroll <= 1, "the surviving event must be visible");
    assert_eq!(s.selection, Selection::Event(b.id));
    assert_eq!(s.selected_target_event().unwrap().id, b.id);
}

#[tokio::test]
async fn calendar_shared_event_in_hidden_history_cannot_reappear() {
    let mut s = state();
    let mut shared = event(1, 2, 9);
    shared.owner_id = Some(Uuid::from_u128(42));
    s.events.push(shared.clone());
    s.push_modal(Modal::Agenda);
    s.select_event(shared.id, None, false);
    s.push_modal(Modal::Details(shared));
    s.push_modal(Modal::View(0));
    s.events.clear();
    s.clear_shared_navigation();
    assert!(matches!(s.modal, Some(Modal::Agenda)));
    s.pop_modal();
    assert!(s.modal.is_none());
    assert!(s.modal_parents.is_empty());
}

#[tokio::test]
async fn calendar_double_click_requires_same_semantic_target_and_surface() {
    let mut s = state();
    let now = Instant::now();
    let interval = std::time::Duration::from_millis(100);
    let target = ClickTarget::Event(Uuid::from_u128(1));
    assert!(!s.register_click(target, now));
    assert!(s.register_click(target, now + interval));
    assert!(!s.register_click(target, now + interval * 2));
    assert!(!s.register_click(ClickTarget::Date(date(2)), now + interval * 3));
    assert!(!s.register_click(target, now + interval * 4));
    s.push_modal(Modal::Agenda);
    assert!(!s.register_click(target, now + interval * 5));
    assert!(s.register_click(target, now + interval * 6));
    assert!(!s.register_click(target, now + interval * 7));
    assert!(!s.register_click(target, now + interval * 12));
}

#[tokio::test]
async fn calendar_context_menu_uses_target_permissions_and_does_not_enter_history() {
    let mut s = state();
    let mut protected = event(1, 2, 9);
    protected.owner_id = None;
    protected.creation_tier = CreationTier::Admin;
    s.events = vec![protected.clone()];
    s.open_context_menu(ClickTarget::Event(protected.id), (12, 8));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        [MenuAction::Open, MenuAction::Copy]
    );
    assert_eq!(s.selection, Selection::Event(protected.id));
    assert!(s.modal_parents.is_empty());
    s.role = CreationTier::Admin;
    s.open_context_menu(ClickTarget::Event(protected.id), (12, 8));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        [
            MenuAction::Open,
            MenuAction::Edit,
            MenuAction::Delete,
            MenuAction::Copy
        ]
    );
    s.open_context_menu(
        ClickTarget::Slot {
            date: date(3),
            minute: 570,
        },
        (12, 8),
    );
    assert_eq!(
        s.context_menu.as_ref().unwrap().selected_action(),
        Some(MenuAction::New)
    );
    assert_eq!(s.selection, Selection::Slot(570));
    assert_eq!(s.selected, date(3));
    s.source = CalendarSource::Server;
    s.role = CreationTier::User;
    s.open_context_menu(ClickTarget::Date(date(3)), (12, 8));
    assert_eq!(s.context_menu.as_ref().unwrap().items, [MenuAction::Agenda]);
}

#[tokio::test]
async fn calendar_event_context_menu_preserves_clicked_segment_date() {
    let mut s = state();
    let mut e = event(1, 2, 23);
    if let EventTiming::Timed { end, .. } = &mut e.timing {
        *end = Some(Utc.with_ymd_and_hms(2026, 10, 3, 2, 0, 0).unwrap());
    }
    s.events.push(e.clone());
    s.open_context_menu(ClickTarget::EventAt(e.id, date(3)), (12, 8));
    assert_eq!(s.selected, date(3));
    assert_eq!(s.selection, Selection::Event(e.id));
}

#[tokio::test]
async fn calendar_expired_upcoming_selection_moves_to_surviving_notice() {
    let mut s = state();
    let mut a = event(1, 2, 9);
    let mut b = event(2, 2, 10);
    for e in [&mut a, &mut b] {
        e.notice_lead_seconds = Some(86400);
        e.notice_start = Utc::now() - Duration::days(1);
        e.notice_end = Utc::now() + Duration::days(1);
    }
    s.notices = vec![a.clone(), b.clone()];
    s.push_modal(Modal::Upcoming);
    assert_eq!(s.selection, Selection::Event(a.id));
    s.notices[0].notice_end = Utc::now() - Duration::seconds(1);
    assert!(s.tick(false, chrono_tz::UTC));
    assert_eq!(s.selection, Selection::Event(b.id));
    assert_eq!(s.selected_target_event().unwrap().id, b.id);
}

#[tokio::test]
async fn calendar_selection_change_cancels_pending_detail_open() {
    use super::svc::Reply;
    let mut s = state();
    let a = event(1, 2, 9);
    let b = event(2, 2, 10);
    s.events = vec![a.clone(), b.clone()];
    for next in [
        Selection::Event(b.id),
        Selection::Date,
        Selection::Slot(600),
    ] {
        s.select_event(a.id, None, false);
        s.open(a.id);
        let generation = s.open_generation;
        match next {
            Selection::Event(_) => s.move_selected_event(1, false),
            Selection::Date => s.select_date(date(2)),
            Selection::Slot(minute) => s.select_slot(date(2), minute),
        }
        assert!(!s.apply(Reply::Opened {
            generation,
            result: Ok(a.clone()),
        }));
        assert!(s.modal.is_none());
        assert_eq!(s.selection, next);
    }
}

#[tokio::test]
async fn calendar_delete_preserves_neighbor_through_invalidation_before_reply() {
    let mut s = state();
    s.view = CalendarView::List;
    let a = event(1, 2, 9);
    let b = event(2, 3, 10);
    s.events = vec![a.clone(), b.clone()];
    s.select_event(a.id, None, false);
    s.push_modal(Modal::Details(a.clone()));
    s.push_modal(Modal::Delete(a.clone()));
    s.loading = true;
    s.events.clear();
    s.finish_delete(a.id);
    assert!(s.modal.is_none());
    assert_eq!(s.selection, Selection::Event(b.id));
    assert!(s.selected_target_event().is_none());
    s.events.push(b.clone());
    s.loading = false;
    s.reconcile_selection();
    assert_eq!(s.selected_target_event().unwrap().id, b.id);
    assert_eq!(s.selected, date(3));
    assert_eq!(s.slot_minute, 600);
}

#[tokio::test]
async fn calendar_hidden_all_day_keyboard_selection_opens_agenda() {
    let mut s = state();
    s.view = CalendarView::Day;
    s.events = (1..=3)
        .map(|id| {
            let mut e = event(id, 2, 9);
            e.timing = EventTiming::AllDay {
                start: date(2),
                end_exclusive: date(3),
            };
            e
        })
        .collect();
    s.move_selected_event(1, false);
    s.move_selected_event(1, false);
    assert!(s.modal.is_none());
    s.move_selected_event(1, false);
    assert!(matches!(s.modal, Some(Modal::Agenda)));
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(3)));
    s.pop_modal();
    assert!(s.modal.is_none());
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(3)));
}
