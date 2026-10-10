use super::*;
use crate::app::calendar::{state::Hit, svc::CalendarService};
use chrono::{NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::{CalendarEvent, EventTiming},
};
use ratatui::layout::Rect;
use uuid::Uuid;

fn state() -> CalendarState {
    let mut state = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    state.loading = false;
    state.source = CalendarSource::Personal(state.viewer);
    state.selected = NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
    state
}

fn event() -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(1),
        owner_id: Some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creation_tier: CreationTier::User,
        mod_editable: false,
        title: "Selected appointment".into(),
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

fn click(x: u16, y: u16, button: MouseButton) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down,
        button: Some(button),
        x,
        y,
        modifiers: Default::default(),
    }
}

#[tokio::test]
async fn calendar_empty_agenda_new_and_cancel_return_to_agenda() {
    let mut s = state();
    s.push_modal(Modal::Agenda);
    handle_modal(&mut s, &ParsedInput::Char('n'));
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
    escape(&mut s);
    assert!(matches!(s.modal, Some(Modal::Agenda)));
    escape(&mut s);
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_blank_click_breaks_event_double_click_sequence() {
    let mut s = state();
    let event = event();
    s.events.push(event.clone());
    s.hits.borrow_mut().push(Hit {
        area: Rect::new(2, 2, 20, 1),
        action: Action::Event(event.id),
    });
    assert!(handle_mouse(&mut s, &click(3, 3, MouseButton::Left)));
    assert_eq!(s.selection, Selection::Event(event.id));
    assert!(s.last_click.borrow().is_some());
    assert!(!handle_mouse(&mut s, &click(40, 15, MouseButton::Left)));
    assert!(s.last_click.borrow().is_none());
    let generation = s.open_generation;
    assert!(handle_mouse(&mut s, &click(3, 3, MouseButton::Left)));
    assert_eq!(
        s.open_generation,
        generation + 1,
        "selection cancels old opens but must not start one"
    );
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_context_menu_dismissal_does_not_activate_underlying_new_button() {
    let mut s = state();
    s.open_context_menu(ClickTarget::Date(s.selected), (12, 8));
    s.hits.borrow_mut().push(Hit {
        area: Rect::new(2, 2, 10, 1),
        action: Action::New,
    });
    handle_menu(&mut s, &ParsedInput::Mouse(click(3, 3, MouseButton::Left)));
    assert!(s.context_menu.is_none());
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_context_menu_escape_keeps_details_open() {
    let mut s = state();
    let event = event();
    s.events.push(event.clone());
    s.select_event(event.id, None, false);
    s.push_modal(Modal::Details(event.clone()));
    s.open_context_menu(ClickTarget::Event(event.id), (12, 8));
    handle_menu(&mut s, &ParsedInput::Byte(0x1b));
    assert!(s.context_menu.is_none());
    assert!(matches!(s.modal, Some(Modal::Details(_))));
    assert_eq!(s.modal_parents.len(), 1);
}

#[tokio::test]
async fn calendar_modal_and_menu_letters_accept_terminal_character_events() {
    let mut s = state();
    let event = event();
    s.events.push(event.clone());
    s.push_modal(Modal::Agenda);
    handle_modal(&mut s, &ParsedInput::Char('j'));
    assert_eq!(s.selection, Selection::Event(event.id));
    handle_modal(&mut s, &ParsedInput::Char('e'));
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
    escape(&mut s);
    assert!(matches!(s.modal, Some(Modal::Agenda)));
    s.open_context_menu(ClickTarget::Event(event.id), (12, 8));
    handle_menu(&mut s, &ParsedInput::Char('j'));
    assert_eq!(s.context_menu.as_ref().unwrap().selected, 1);
    handle_menu(&mut s, &ParsedInput::Byte(b'\r'));
    assert!(s.context_menu.is_none());
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
}

#[tokio::test]
async fn calendar_source_arrows_wrap_and_clear_the_previous_calendar() {
    let mut s = state();
    let shared = Uuid::from_u128(2);
    s.public.push(late_core::models::calendar::PublicCalendar {
        owner_id: shared,
        username: "shared".into(),
    });
    s.source = CalendarSource::Server;
    let date = s.selected;
    s.events.push(event());
    s.selection = Selection::Event(s.events[0].id);
    s.agenda_scroll = 5;

    act(&mut s, Action::CycleSource(-1));
    assert_eq!(s.source, CalendarSource::Personal(shared));
    assert!(s.events.is_empty());
    assert_eq!(s.selection, Selection::Date);
    assert_eq!(s.agenda_scroll, 0);
    assert_eq!(s.selected, date);
    assert!(s.modal.is_none());

    act(&mut s, Action::CycleSource(1));
    assert_eq!(s.source, CalendarSource::Server);
    act(&mut s, Action::CycleSource(1));
    assert_eq!(s.source, CalendarSource::Personal(s.viewer));
    act(&mut s, Action::Source);
    assert!(matches!(s.modal, Some(Modal::Source(1))));
}

#[tokio::test]
async fn calendar_view_arrows_wrap_and_preserve_the_selected_date() {
    let mut s = state();
    let date = s.selected;
    act(&mut s, Action::CycleView(-1));
    assert_eq!(s.view, CalendarView::List);
    act(&mut s, Action::CycleView(1));
    assert_eq!(s.view, CalendarView::Month);
    act(&mut s, Action::CycleView(1));
    assert_eq!(s.view, CalendarView::Week);
    assert!(matches!(s.selection, Selection::Slot(_)));
    assert_eq!(s.selected, date);
    assert!(s.modal.is_none());
    act(&mut s, Action::View);
    assert!(matches!(s.modal, Some(Modal::View(1))));
}

fn imports() -> Vec<super::super::ical::Candidate> {
    super::super::ical::parse("BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:First\nDTSTART;VALUE=DATE:20261004\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:Tea tomorrow at 4pm\nDTSTART:20261101T083027Z\nDTEND:20261101T093029Z\nEND:VEVENT\nEND:VCALENDAR", chrono_tz::UTC).unwrap()
}

#[tokio::test]
async fn calendar_import_typing_paste_limits_and_cancelled_replies() {
    let mut s = state();
    act(&mut s, Action::New);
    let Some(Modal::Editor(editor)) = &mut s.modal else {
        panic!("editor");
    };
    editor.fields[0].insert_str("Unfinished draft");
    let original = editor.fingerprint();
    act(&mut s, Action::Import);
    for c in "https://example.org/events.ics".chars() {
        handle_modal(&mut s, &ParsedInput::Char(c));
    }
    let Some(Modal::Import(import)) = &mut s.modal else {
        panic!("import");
    };
    assert_eq!(
        import.input.lines().join("\n"),
        "https://example.org/events.ics"
    );
    let before = import.input.lines().to_owned();
    import.handle(
        &ParsedInput::Paste(vec![b'x'; super::super::ical::MAX_BYTES + 1]),
        3,
    );
    assert_eq!(import.input.lines(), before);
    assert!(import.error.as_ref().unwrap().contains("1 MiB"));
    import.loading = true;
    let generation = s.import_generation;
    handle_modal(&mut s, &ParsedInput::Byte(0x1b));
    assert!(matches!(&s.modal, Some(Modal::Editor(editor)) if editor.fingerprint() == original));
    assert!(!s.apply(super::super::svc::Reply::Imported {
        generation,
        result: Ok(imports())
    }));
    act(&mut s, Action::Import);
    if let Some(Modal::Import(import)) = &mut s.modal {
        import.loading = true;
    }
    assert!(!s.apply(super::super::svc::Reply::Imported {
        generation,
        result: Ok(imports())
    }));
    assert!(matches!(&s.modal, Some(Modal::Import(import)) if import.loading));
}

#[tokio::test]
async fn calendar_import_chooses_one_new_draft_and_preserves_seconds_dst_and_title() {
    let mut s = state();
    s.tz = chrono_tz::America::Denver;
    act(&mut s, Action::New);
    act(&mut s, Action::Import);
    if let Some(Modal::Import(import)) = &mut s.modal {
        import.loading = true;
    }
    assert!(s.apply(super::super::svc::Reply::Imported {
        generation: s.import_generation,
        result: Ok(imports())
    }));
    assert!(
        matches!(&s.modal, Some(Modal::Import(import)) if import.candidates.as_ref().unwrap().len()==2)
    );
    handle_modal(&mut s, &ParsedInput::Arrow(b'B'));
    handle_modal(&mut s, &ParsedInput::Byte(b'\r'));
    let Some(Modal::Editor(editor)) = &mut s.modal else {
        panic!("editor");
    };
    assert_eq!(editor.existing, None);
    assert!(editor.dirty());
    assert_eq!(editor.text(3), "01:30:27");
    assert_eq!(
        editor.occurrence,
        Some(late_core::models::calendar::Occurrence::Later)
    );
    let draft = editor.draft(s.selected, s.tz).unwrap();
    assert_eq!(draft.title, "Tea tomorrow at 4pm");
    assert!(
        matches!(draft.timing, EventTiming::Timed { start, end:Some(end) }
        if start == "2026-11-01T08:30:27Z".parse::<chrono::DateTime<Utc>>().unwrap()
        && end == "2026-11-01T09:30:29Z".parse::<chrono::DateTime<Utc>>().unwrap())
    );
    assert!(s.events.is_empty());
    assert!(!s.pending);
    escape(&mut s);
    assert!(matches!(&s.modal, Some(Modal::Editor(editor)) if editor.discard_prompt));
    act(&mut s, Action::Discard);
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_import_read_only_and_invalid_choice_leave_reviewable_input() {
    let mut s = state();
    s.source = CalendarSource::Server;
    act(&mut s, Action::New);
    assert!(s.modal.is_none());
    assert!(s.error.as_deref().unwrap().contains("read-only"));
    s.source = CalendarSource::Personal(s.viewer);
    act(&mut s, Action::New);
    act(&mut s, Action::Import);
    if let Some(Modal::Import(import)) = &mut s.modal {
        import.loading = true;
        import.input.insert_str("original content");
    }
    let mut choices = imports();
    choices[0].draft = Err("Recurring series are not supported".into());
    s.apply(super::super::svc::Reply::Imported {
        generation: s.import_generation,
        result: Ok(choices),
    });
    handle_modal(&mut s, &ParsedInput::Byte(b'\r'));
    assert!(
        matches!(&s.modal, Some(Modal::Import(import)) if import.error.as_deref().unwrap().contains("Recurring"))
    );
    handle_modal(&mut s, &ParsedInput::Char('b'));
    assert!(
        matches!(&s.modal, Some(Modal::Import(import)) if import.candidates.is_none() && import.input.lines()==["original content"])
    );
    escape(&mut s);
    escape(&mut s);
    s.source = CalendarSource::Server;
    s.role = CreationTier::Admin;
    act(&mut s, Action::New);
    act(&mut s, Action::Import);
    if let Some(Modal::Import(import)) = &mut s.modal {
        import.candidates = Some(imports());
    }
    s.role = CreationTier::User;
    act(&mut s, Action::Choice(0));
    assert!(
        matches!(&s.modal, Some(Modal::Import(import)) if import.error.as_deref()==Some("This calendar is read-only"))
    );
}

#[tokio::test]
async fn calendar_import_updates_existing_draft_and_preserves_restricted_settings() {
    let mut s = state();
    s.role = CreationTier::Moderator;
    let mut e = event();
    e.owner_id = None;
    e.creation_tier = CreationTier::Moderator;
    e.notice_lead_seconds = Some(3600);
    e.revision = 7;
    s.events.push(e.clone());
    s.selection = Selection::Event(e.id);
    s.push_modal(Modal::Details(e.clone()));
    act(&mut s, Action::Edit);
    let Some(Modal::Editor(editor)) = &s.modal else {
        panic!("editor");
    };
    assert!(!editor.access.notifications);
    let initial = editor.initial.clone();
    act(&mut s, Action::Import);
    let Some(Modal::Import(import)) = &mut s.modal else {
        panic!("import");
    };
    import.candidates = Some(imports());
    act(&mut s, Action::Choice(0));
    let Some(Modal::Editor(editor)) = &s.modal else {
        panic!("editor");
    };
    assert_eq!(editor.existing, Some((e.id, 7)));
    assert_eq!(editor.source, CalendarSource::Server);
    assert_eq!(editor.initial, initial);
    assert!(editor.dirty());
    assert!(editor.notifications);
    assert_eq!(editor.text(6), "1h");
    assert!(!editor.delegated);
    assert_eq!(editor.text(0), "First");
    assert_eq!(s.events[0].title, e.title);
    escape(&mut s);
    act(&mut s, Action::Discard);
    assert!(
        matches!(&s.modal, Some(Modal::Details(details)) if details.id == e.id && details.title == e.title)
    );
}

#[tokio::test]
async fn calendar_copy_read_only_event_from_details_and_context_menu() {
    let mut s = state();
    let mut e = event();
    e.owner_id = None;
    e.creation_tier = CreationTier::Admin;
    s.events.push(e.clone());
    s.selection = Selection::Event(e.id);
    s.push_modal(Modal::Details(e.clone()));
    handle_modal(&mut s, &ParsedInput::Char('y'));
    let ics = s.clipboard.take().unwrap();
    let copied = super::super::ical::parse(&ics, s.tz)
        .unwrap()
        .remove(0)
        .draft
        .unwrap();
    assert_eq!(copied.title, e.title);
    assert_eq!(copied.timing, e.timing);
    s.pop_modal();
    s.open_context_menu(ClickTarget::Event(e.id), (2, 2));
    assert_eq!(
        s.context_menu.as_ref().unwrap().items,
        [MenuAction::Open, MenuAction::Copy]
    );
    act(&mut s, Action::MenuChoice(1));
    assert!(
        s.clipboard
            .take()
            .unwrap()
            .contains(&format!("UID:{}@late.sh", e.id))
    );
}
