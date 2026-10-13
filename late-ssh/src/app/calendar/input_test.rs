use super::{
    input::{act, escape, handle_key, handle_menu, handle_modal, handle_mouse},
    navigation::{ClickTarget, Selection},
    state::{Action, CalendarState, Hit, Modal},
    svc::CalendarService,
};
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::{CalendarEvent, CalendarSource, CalendarView, EventTiming},
};
use ratatui::layout::Rect;
use uuid::Uuid;

fn state() -> CalendarState {
    let mut state = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    state.loading = false;
    state.selected = NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
    state
}

fn event(board: bool) -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(1),
        owner_id: (!board).then_some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creator_name: "me".into(),
        title: "Selected appointment".into(),
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

fn click(x: u16, y: u16, button: MouseButton) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down,
        button: Some(button),
        x,
        y,
        modifiers: Default::default(),
    }
}

/// `n` opens a private draft whether or not the board is shown: posting to
/// everyone is a deliberate flip of the target, never the default.
#[tokio::test]
async fn a_new_draft_is_private_until_flipped() {
    let mut s = state();
    act(&mut s, Action::New);
    let Some(Modal::Editor(e)) = &s.modal else {
        panic!("no editor");
    };
    assert_eq!(e.source, CalendarSource::Personal);
    escape(&mut s);
    act(&mut s, Action::Board);
    assert!(!s.show_board);
    act(&mut s, Action::New);
    let Some(Modal::Editor(e)) = &s.modal else {
        panic!("no editor");
    };
    assert_eq!(e.source, CalendarSource::Personal);
    escape(&mut s);
    act(&mut s, Action::View);
    assert_eq!(s.view, CalendarView::List);
    act(&mut s, Action::View);
    assert_eq!(s.view, CalendarView::Month);
}

/// "I'm in" needs a board event under the cursor; a bare date or a
/// personal event says why nothing happened instead of sending a write.
#[tokio::test]
async fn rsvp_needs_a_board_event() {
    let mut s = state();
    act(&mut s, Action::Rsvp);
    assert_eq!(s.error.as_deref(), Some("Select an event first"));
    assert!(!s.pending);
    let mine = event(false);
    s.events.push(mine.clone());
    s.select_event(mine.id, None);
    act(&mut s, Action::Rsvp);
    assert_eq!(s.error.as_deref(), Some("Only board events take an I'm in"));
    assert!(!s.pending);
    s.events[0].owner_id = None;
    s.select_event(mine.id, None);
    act(&mut s, Action::Rsvp);
    assert!(s.pending, "a board event sends the write");
}

/// Enter on an empty day starts a new event there; on a day with one it
/// opens the marked event.
#[tokio::test]
async fn enter_starts_a_new_event_on_an_empty_day_and_opens_the_marked_one() {
    let mut s = state();
    let day = s.selected;
    let date = day + Duration::days(3);
    s.select_date(date);
    handle_key(&mut s, &ParsedInput::Byte(b'\r'));
    let Some(Modal::Editor(e)) = &s.modal else {
        panic!("no editor");
    };
    assert_eq!(e.text(2), date.to_string());
    escape(&mut s);
    assert!(s.modal.is_none());

    s.events.push(event(true));
    s.select_date(day);
    assert_eq!(s.selection, Selection::Event(Uuid::from_u128(1)));
    let opens = s.open_generation;
    handle_key(&mut s, &ParsedInput::Byte(b'\r'));
    assert_eq!(s.open_generation, opens + 2, "Enter asks for the event");
    assert!(s.modal.is_none(), "the details land with the reply");
}

#[tokio::test]
async fn blank_click_breaks_an_event_double_click_sequence() {
    let mut s = state();
    let e = event(true);
    s.events.push(e.clone());
    let rect = Rect::new(5, 5, 10, 1);
    s.hits.borrow_mut().push(Hit {
        area: rect,
        action: Action::Event(e.id),
    });
    assert!(handle_mouse(&mut s, &click(6, 6, MouseButton::Left)));
    assert_eq!(s.selection, Selection::Event(e.id));
    assert!(!handle_mouse(&mut s, &click(40, 20, MouseButton::Left)));
    assert!(handle_mouse(&mut s, &click(6, 6, MouseButton::Left)));
    assert!(
        s.modal.is_none(),
        "a click on padding in between is not a double"
    );
    assert!(handle_mouse(&mut s, &click(6, 6, MouseButton::Left)));
    assert!(s.open_generation > 0, "the double opens it");
}

#[tokio::test]
async fn context_menu_dismissal_does_not_activate_the_underlying_button() {
    let mut s = state();
    let e = event(true);
    s.events.push(e.clone());
    s.open_context_menu(ClickTarget::Event(e.id), (3, 3));
    s.hits.borrow_mut().push(Hit {
        area: Rect::new(0, 0, 10, 1),
        action: Action::New,
    });
    handle_menu(&mut s, &ParsedInput::Mouse(click(1, 1, MouseButton::Left)));
    assert!(s.context_menu.is_none());
    assert!(s.modal.is_none(), "the click only closed the menu");
}

#[tokio::test]
async fn context_menu_escape_keeps_details_open() {
    let mut s = state();
    let e = event(true);
    s.events.push(e.clone());
    s.push_modal(Modal::Details(e.clone()));
    s.open_context_menu(ClickTarget::Event(e.id), (3, 3));
    handle_menu(&mut s, &ParsedInput::Byte(0x1b));
    assert!(s.context_menu.is_none());
    assert!(matches!(s.modal, Some(Modal::Details(_))));
    handle_modal(&mut s, &ParsedInput::Char('i'));
    assert!(s.pending, "i in details says I'm in");
}

#[tokio::test]
async fn modal_and_menu_letters_accept_terminal_character_events() {
    let mut s = state();
    let e = event(true);
    s.events.push(e.clone());
    s.push_modal(Modal::Details(e.clone()));
    handle_modal(&mut s, &ParsedInput::Char('e'));
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
    escape(&mut s);
    s.open_context_menu(ClickTarget::Event(e.id), (3, 3));
    handle_menu(&mut s, &ParsedInput::Char('j'));
    assert_eq!(s.context_menu.as_ref().unwrap().selected, 1);
}
