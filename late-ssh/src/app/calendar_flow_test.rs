use crate::{
    app::{
        calendar::{
            editor::EditorControl,
            input::act,
            navigation::Selection,
            state::{Action, Modal},
        },
        common::primitives::Screen,
        state::App,
    },
    test_helpers::{make_app, new_test_db, render_plain, wait_for_app},
};
use late_core::models::calendar::{
    CalendarPreferences, CalendarSource, CalendarStore, CalendarView, EventDraft, EventTiming,
};
use late_core::test_utils::create_test_user;
use ratatui::layout::Rect;
use uuid::Uuid;
#[tokio::test]
async fn calendar_go_date_human_input_preview_validation_and_mouse_submit() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_human_dates").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-human-dates");
    app.show_splash = false;
    app.handle_input(b"7");
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    app.calendar.selected = "2028-08-31".parse().unwrap();
    app.handle_input(b"g2 months ago");
    assert!(render_plain(&mut app).contains("Go to 2028-06-30"));
    app.handle_input(b"\r");
    assert!(app.calendar.modal.is_none());
    assert_eq!(app.calendar.selected.to_string(), "2028-06-30");
    app.handle_input(b"g02/03/2026\r");
    assert!(app.calendar.error.as_deref().unwrap().contains("Ambiguous"));
    let Some(Modal::Go(input)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(input.lines(), &["02/03/2026"]);
    assert_eq!(app.calendar.selected.to_string(), "2028-06-30");
    app.handle_input(b"\x15Oct 2nd, 2028");
    assert!(app.calendar.error.is_none());
    app.resize(48, 16).unwrap();
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("Go to 2028-10-02"), "{rendered}");
    let go = app
        .calendar
        .hits
        .borrow()
        .iter()
        .find(|h| h.action == Action::Save)
        .unwrap()
        .area;
    app.handle_input(format!("\x1b[<0;{};{}M", go.x + 1, go.y + 1).as_bytes());
    assert!(app.calendar.modal.is_none());
    assert_eq!(app.calendar.selected.to_string(), "2028-10-02");
    app.handle_input(b"g+2w\r");
    assert_eq!(app.calendar.selected.to_string(), "2028-10-16");
    app.handle_input(b"gtoday\r");
    assert_eq!(app.calendar.selected, app.calendar.today());
    app.handle_input(b"g-2 weeks\x1b");
    wait_for_app(&mut app, "go-to-date escape", |a| {
        a.calendar.modal.is_none()
    })
    .await;
    assert_eq!(app.calendar.selected, app.calendar.today());

    app.handle_input(b"c\x1b[B\rnTrip\t\t\t\x152 Oct 2028\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(2), "2028-10-02");
    assert_eq!(e.focus, EditorControl::EndDate);
    assert!(e.ever_assigned);
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "human calendar date save", |a| {
        !a.calendar.pending
    })
    .await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(
        saved.timing.dates(app.calendar.tz).0.to_string(),
        "2028-10-02"
    );
}

#[tokio::test]
async fn calendar_app_navigation_modal_priority_and_editor_text() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_ui").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-ui-flow");
    app.show_splash = false;
    app.handle_input(b"7");
    assert_eq!(app.screen, Screen::Calendars);
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    assert!(render_plain(&mut app).contains("Upcoming events"));
    app.handle_input(b"C");
    assert!(matches!(app.calendar.modal, Some(Modal::Source(_))));
    app.handle_input(b"\x1b[B\r");
    assert_eq!(app.calendar.source, CalendarSource::Personal(user.id));
    app.handle_input(b"n");
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    app.handle_input(b"Unicode \xe6\x97\xa5 2028-02-29");
    app.handle_input(b"\x0f");
    assert!(app.show_settings);
    app.handle_input(b"7sv");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(0), "Unicode 日 2028-02-29");
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "account settings escape", |a| !a.show_settings).await;
    app.handle_input(b"\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(0), "Unicode 日");
    assert_eq!(e.text(2), "2028-02-29");
    app.handle_input(b"7sScCvVnNgGtT");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(1), "7sScCvVnNgGtT");
    assert_eq!(app.screen, Screen::Calendars);
    act(&mut app.calendar, Action::Cancel);
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert!(e.discard_prompt);
    act(&mut app.calendar, Action::Keep);
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "calendar save", |a| !a.calendar.pending).await;
    assert!(matches!(app.calendar.modal, Some(Modal::Details(_))));
    act(&mut app.calendar, Action::Cancel);
    for key in [b"s", b"S"] {
        app.handle_input(key);
        assert!(matches!(app.calendar.modal, Some(Modal::Settings { .. })));
        act(&mut app.calendar, Action::Cancel);
    }
    app.handle_input(b"\x0f");
    assert!(app.show_settings);
    app.show_settings = false;
    app.handle_input(b"\t");
    assert_eq!(app.screen, Screen::Clubhouse);
    app.handle_input(b"\x1b[Z");
    assert_eq!(app.screen, Screen::Calendars);
    app.calendar.view = CalendarView::Week;
    render_plain(&mut app);
    assert!(!app.calendar.hits.borrow().is_empty());
    app.resize(48, 16).unwrap();
    assert!(app.calendar.hits.borrow().is_empty());
    render_plain(&mut app);
    let grid = app
        .calendar
        .panes
        .borrow()
        .iter()
        .find(|p| p.pane == crate::app::calendar::state::Pane::Grid)
        .unwrap()
        .area;
    let before = app.calendar.hour_scroll;
    app.handle_input(format!("\x1b[<65;{};{}M", grid.x + 1, grid.y + 1).as_bytes());
    assert_eq!(app.calendar.hour_scroll, before + 3);
    app.calendar.view = CalendarView::Month;
    app.handle_input(b"n");
    app.handle_input(b"Mouse Save 2028-03-01");
    render_plain(&mut app);
    let save = app
        .calendar
        .hits
        .borrow()
        .iter()
        .find(|h| h.action == Action::Save)
        .unwrap()
        .area;
    app.handle_input(format!("\x1b[<0;{};{}M", save.x + 1, save.y + 1).as_bytes());
    wait_for_app(&mut app, "mouse calendar save", |a| !a.calendar.pending).await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(saved.title, "Mouse Save");
    assert_eq!(
        saved.timing.dates(app.calendar.tz).0.to_string(),
        "2028-03-01"
    );
    app.handle_input(b"\x1b[3~");
    assert!(matches!(app.calendar.modal, Some(Modal::Delete(_))));
    app.handle_input(b"y");
    wait_for_app(&mut app, "calendar confirmed delete", |a| {
        !a.calendar.pending
    })
    .await;
    assert!(app.calendar.modal.is_none());
}

fn event_hit(app: &mut App, id: Uuid) -> Rect {
    render_plain(app);
    app.calendar
        .hits
        .borrow()
        .iter()
        .find(|hit| matches!(hit.action, Action::Event(event) | Action::EventAt(event, _) if event == id))
        .unwrap()
        .area
}

fn mouse_click(app: &mut App, rect: Rect, button: u8) {
    app.handle_input(
        format!(
            "\x1b[<{button};{};{}M\x1b[<{button};{};{}m",
            rect.x + 1,
            rect.y + 1,
            rect.x + 1,
            rect.y + 1,
        )
        .as_bytes(),
    );
}

#[tokio::test]
async fn calendar_mouse_select_open_context_and_keyboard_menu_priority() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_pointer").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-pointer-flow");
    app.show_splash = false;
    app.handle_input(b"7c\x1b[B\r");
    wait_for_app(&mut app, "personal calendar", |a| !a.calendar.loading).await;
    let date = app.calendar.selected;
    let event = CalendarStore::new(db.db.clone())
        .save(
            user.id,
            CalendarSource::Personal(user.id),
            None,
            &EventDraft {
                title: "Pointer appointment".into(),
                description: String::new(),
                timing: EventTiming::AllDay {
                    start: date,
                    end_exclusive: date.succ_opt().unwrap(),
                },
                notice_lead_seconds: None,
                mod_editable: false,
            },
        )
        .await
        .unwrap();
    app.calendar.view = CalendarView::List;
    app.calendar.refresh();
    wait_for_app(&mut app, "pointer event loaded", |a| {
        a.calendar.events.iter().any(|e| e.id == event.id)
    })
    .await;
    let target = event_hit(&mut app, event.id);
    app.interaction_mode = late_core::models::user::InteractionMode::Keyboard;
    mouse_click(&mut app, target, 0);
    mouse_click(&mut app, target, 2);
    assert!(app.calendar.modal.is_none());
    assert!(app.calendar.context_menu.is_none());
    assert_ne!(app.calendar.selection, Selection::Event(event.id));
    app.interaction_mode = late_core::models::user::InteractionMode::Hybrid;
    mouse_click(&mut app, target, 0);
    assert_eq!(app.calendar.selection, Selection::Event(event.id));
    assert!(app.calendar.modal.is_none());
    mouse_click(&mut app, target, 0);
    wait_for_app(
        &mut app,
        "double-click details",
        |a| matches!(&a.calendar.modal, Some(Modal::Details(e)) if e.id == event.id),
    )
    .await;
    act(&mut app.calendar, Action::Cancel);
    assert!(app.calendar.modal.is_none());
    let target = event_hit(&mut app, event.id);
    mouse_click(&mut app, target, 2);
    assert!(app.calendar.context_menu.is_some());
    app.handle_input(b"n7v");
    assert_eq!(app.screen, Screen::Calendars);
    assert!(app.calendar.modal.is_none());
    assert!(app.calendar.context_menu.is_some());
    app.handle_input(b"\x1b[B\r");
    assert!(app.calendar.context_menu.is_none());
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    render_plain(&mut app);
    let cancel = app
        .calendar
        .hits
        .borrow()
        .iter()
        .find(|hit| hit.action == Action::Cancel)
        .unwrap()
        .area;
    app.interaction_mode = late_core::models::user::InteractionMode::Keyboard;
    mouse_click(&mut app, cancel, 0);
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    app.interaction_mode = late_core::models::user::InteractionMode::Hybrid;
    app.handle_input(b" revised");
    act(&mut app.calendar, Action::Cancel);
    assert!(matches!(&app.calendar.modal, Some(Modal::Editor(e)) if e.discard_prompt));
    app.handle_input(b"\x1b");
    wait_for_app(
        &mut app,
        "discard confirmation escape",
        |a| matches!(&a.calendar.modal, Some(Modal::Editor(e)) if !e.discard_prompt),
    )
    .await;
    assert!(
        matches!(&app.calendar.modal, Some(Modal::Editor(e)) if e.text(0) == "Pointer appointment revised")
    );
    act(&mut app.calendar, Action::Cancel);
    app.handle_input(b"d");
    assert!(app.calendar.modal.is_none());
    app.calendar.view = CalendarView::Month;
    app.calendar.select_date(date);
    app.handle_input(b"e\x1b[3~");
    assert!(
        app.calendar.modal.is_none(),
        "Date focus must not edit/delete an implicit first event"
    );
}

#[tokio::test]
async fn calendar_nested_details_edit_save_cancel_and_delete_return_to_agenda() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_nested").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-nested-flow");
    app.show_splash = false;
    app.handle_input(b"7c\x1b[B\r");
    wait_for_app(&mut app, "nested personal calendar", |a| {
        !a.calendar.loading
    })
    .await;
    let date = app.calendar.selected;
    app.handle_input(b"nNested appointment\x13");
    wait_for_app(&mut app, "nested event saved", |a| !a.calendar.pending).await;
    let Some(Modal::Details(event)) = &app.calendar.modal else {
        panic!()
    };
    let id = event.id;
    act(&mut app.calendar, Action::Cancel);
    wait_for_app(&mut app, "nested event loaded", |a| {
        !a.calendar.loading && a.calendar.events.iter().any(|e| e.id == id)
    })
    .await;
    app.calendar.select_date(date);
    app.handle_input(b"\r");
    assert!(matches!(app.calendar.modal, Some(Modal::Agenda)));
    app.handle_input(b"j\r");
    wait_for_app(
        &mut app,
        "nested details",
        |a| matches!(&a.calendar.modal, Some(Modal::Details(e)) if e.id == id),
    )
    .await;
    app.handle_input(b"e");
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    act(&mut app.calendar, Action::Cancel);
    assert!(matches!(&app.calendar.modal, Some(Modal::Details(e)) if e.id == id));
    app.handle_input(b"e revised\x13");
    wait_for_app(&mut app, "nested edit saved", |a| !a.calendar.pending).await;
    assert!(
        matches!(&app.calendar.modal, Some(Modal::Details(e)) if e.id == id && e.title == "Nested appointment revised")
    );
    app.handle_input(b"\x1b[3~");
    assert!(matches!(app.calendar.modal, Some(Modal::Delete(_))));
    app.handle_input(b"n");
    assert!(matches!(&app.calendar.modal, Some(Modal::Details(e)) if e.id == id));
    app.handle_input(b"\x1b[3~y");
    wait_for_app(&mut app, "nested event deleted", |a| !a.calendar.pending).await;
    assert!(matches!(app.calendar.modal, Some(Modal::Agenda)));
    assert_ne!(app.calendar.selection, Selection::Event(id));
    assert_eq!(app.calendar.selected, date);
    act(&mut app.calendar, Action::Cancel);
    assert!(app.calendar.modal.is_none());
    assert_eq!(app.calendar.selected, date);
}

#[tokio::test]
async fn calendar_timed_slot_keyboard_and_double_click_create_at_selected_time() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_slots").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-slot-flow");
    app.show_splash = false;
    app.handle_input(b"7c\x1b[B\r");
    wait_for_app(&mut app, "slot personal calendar", |a| !a.calendar.loading).await;
    app.calendar.view = CalendarView::Day;
    let date = app.calendar.selected;
    app.calendar.select_slot(date, 9 * 60);
    app.handle_input(b"\x1b[B\r");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert!(!e.all_day);
    assert_eq!(e.text(3), "09:30");
    assert_eq!(e.text(2), date.to_string());
    app.handle_input(b"Review tomorrow at 4pm\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(0), "Review tomorrow at 4pm");
    assert_eq!(
        e.text(3),
        "09:30",
        "Explicit slot selection prevents title inference from changing the chosen time"
    );
    act(&mut app.calendar, Action::Cancel);
    app.handle_input(b"d");
    assert!(app.calendar.modal.is_none());
    render_plain(&mut app);
    let (target, selected_date, minute) = app
        .calendar
        .hits
        .borrow()
        .iter()
        .find_map(|hit| {
            if let Action::Slot(date, minute) = hit.action {
                Some((hit.area, date, minute))
            } else {
                None
            }
        })
        .unwrap();
    mouse_click(&mut app, target, 0);
    assert_eq!(app.calendar.selection, Selection::Slot(minute));
    assert!(app.calendar.modal.is_none());
    mouse_click(&mut app, target, 0);
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert!(!e.all_day);
    assert_eq!(e.text(2), selected_date.to_string());
    assert_eq!(e.text(3), format!("{:02}:{:02}", minute / 60, minute % 60));
}

#[tokio::test]
async fn calendar_public_profile_link_opens_read_only_source_and_clears_on_resize() {
    let db = new_test_db().await;
    let owner = create_test_user(&db.db, "calendar_profile_owner").await;
    let viewer = create_test_user(&db.db, "calendar_profile_viewer").await;
    CalendarStore::new(db.db.clone())
        .save_preferences(
            owner.id,
            &CalendarPreferences {
                public: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut app = make_app(db.db.clone(), viewer.id, "calendar-profile-link");
    app.show_splash = false;
    wait_for_app(&mut app, "public calendar sources", |a| !a.calendar.loading).await;
    app.set_screen(Screen::Profiles);
    app.profile_modal_state.open(owner.id, &owner.username);
    app.show_profile_modal = true;
    assert!(render_plain(&mut app).contains("c Open calendar"));
    assert!(app.profile_modal_state.calendar_link.get().width > 0);
    app.resize(80, 24).unwrap();
    assert_eq!(app.profile_modal_state.calendar_link.get().width, 0);
    app.handle_input(b"c");
    assert_eq!(app.screen, Screen::Calendars);
    assert_eq!(app.calendar.source, CalendarSource::Personal(owner.id));
    assert!(!app.show_profile_modal);
    app.handle_input(b"n");
    assert!(!matches!(app.calendar.modal, Some(Modal::Editor(_))));
    assert_eq!(
        app.calendar.error.as_deref(),
        Some("This calendar is read-only")
    );
}

#[tokio::test]
async fn calendar_ical_paste_review_save_and_clipboard_follow_real_input_routing() {
    use base64::Engine;
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_exchange_flow").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-exchange-flow");
    app.show_splash = false;
    app.handle_input(b"7c\x1b[B\r");
    wait_for_app(&mut app, "personal calendar", |a| !a.calendar.loading).await;
    assert_eq!(app.calendar.source, CalendarSource::Personal(user.id));
    app.handle_input(b"i");
    assert!(app.calendar.modal.is_none());
    app.handle_input(b"\x1b[200~https://example.org/events.ics\x1b[201~");
    assert!(app.calendar.modal.is_none());
    app.handle_input(b"n");
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    app.handle_input(b"\x1b[Z\r");
    assert!(matches!(app.calendar.modal, Some(Modal::Import(_))));
    let input = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Clipboard café\r\nDESCRIPTION:First\\nSecond\\, third\r\nDTSTART:20261004T153027Z\r\nDTEND:20261004T163029Z\r\nEND:VEVENT\r\nEND:VCALENDAR";
    app.handle_input(format!("\x1b[200~{input}\x1b[201~").as_bytes());
    app.handle_input(b"\x13");
    assert!(!app.show_settings);
    wait_for_app(&mut app, "import review", |a| {
        matches!(a.calendar.modal, Some(Modal::Editor(_)))
    })
    .await;
    let Some(Modal::Editor(editor)) = &app.calendar.modal else {
        panic!("editor");
    };
    assert_eq!(editor.text(0), "Clipboard café");
    assert_eq!(editor.text(1), "First\nSecond, third");
    assert!(app.calendar.events.is_empty());
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "import save", |a| {
        matches!(a.calendar.modal, Some(Modal::Details(_))) && !a.calendar.pending
    })
    .await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!("details");
    };
    let saved = saved.clone();
    app.handle_input(b"y");
    let copied = app.pending_clipboard.as_ref().unwrap().clone();
    let unfolded = icalendar::parser::unfold(&copied);
    let document = icalendar::parser::read_components(&unfolded).unwrap();
    let event = &document[0].components[0];
    assert_eq!(
        event.find_prop("SUMMARY").unwrap().val.as_str(),
        saved.title
    );
    assert_eq!(
        event.find_prop("DESCRIPTION").unwrap().val.as_str(),
        saved.description
    );
    assert_eq!(
        event.find_prop("DTSTART").unwrap().val.as_str(),
        "20261004T153027Z"
    );
    assert_eq!(
        event.find_prop("DTEND").unwrap().val.as_str(),
        "20261004T163029Z"
    );
    let expected = format!(
        "\x1b]52;c;{}\x07",
        base64::engine::general_purpose::STANDARD.encode(copied.as_bytes())
    );
    render_plain(&mut app);
    assert!(
        app.pending_terminal_commands
            .iter()
            .any(|bytes| bytes == expected.as_bytes())
    );
    assert!(app.pending_clipboard.is_none());

    app.handle_input(b"e\x1b[Z\r");
    assert!(matches!(app.calendar.modal, Some(Modal::Import(_))));
    let replacement =
        "BEGIN:VEVENT\r\nSUMMARY:Revised appointment\r\nDTSTART;VALUE=DATE:20261006\r\nEND:VEVENT";
    app.handle_input(format!("\x1b[200~{replacement}\x1b[201~").as_bytes());
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "edit import review", |a| {
        matches!(a.calendar.modal, Some(Modal::Editor(_)))
    })
    .await;
    let Some(Modal::Editor(editor)) = &app.calendar.modal else {
        panic!("editor");
    };
    assert_eq!(editor.existing, Some((saved.id, saved.revision)));
    assert_eq!(editor.text(0), "Revised appointment");
    assert_eq!(editor.text(2), "2026-10-06");
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "edit import save", |a| {
        matches!(&a.calendar.modal, Some(Modal::Details(e)) if e.id == saved.id && e.revision > saved.revision)
            && !a.calendar.pending
    }).await;
    let rows = db
        .db
        .get()
        .await
        .unwrap()
        .query(
            "SELECT id, title FROM calendar_events WHERE owner_id=$1",
            &[&user.id],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, uuid::Uuid>(0), saved.id);
    assert_eq!(rows[0].get::<_, String>(1), "Revised appointment");
}
