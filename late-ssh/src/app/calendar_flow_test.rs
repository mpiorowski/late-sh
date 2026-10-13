use crate::{
    app::{
        calendar::{
            input::act,
            navigation::Selection,
            state::{Action, Modal},
        },
        common::primitives::Screen,
        live::pick::LiveSource,
        profile::state::profile_params_from_profile,
        state::App,
    },
    pg_listener::Signal,
    test_helpers::{make_app, new_test_db, render_plain, wait_for_app},
};
use chrono::{Duration, Utc};
use late_core::models::calendar::{CalendarSource, CalendarStore, EventDraft, EventTiming};
use late_core::test_utils::create_test_user;
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use uuid::Uuid;

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

/// Save the account timezone the way Settings does and wait for the page
/// to take it.
async fn set_timezone(app: &mut App, user_id: Uuid, zone: &str) {
    wait_for_app(app, "profile loaded", |a| {
        !a.profile_state.profile().username.is_empty()
    })
    .await;
    let mut params = profile_params_from_profile(app.profile_state.profile());
    params.timezone = Some(zone.to_string());
    app.profile_state.service().edit_profile(user_id, params);
    let tz: chrono_tz::Tz = zone.parse().unwrap();
    wait_for_app(app, "account timezone", |a| a.calendar.tz == tz).await;
}

/// Open `id` from the loaded page and press `e`.
async fn edit(app: &mut App, id: Uuid) {
    app.handle_input(b"7");
    wait_for_app(app, "event loaded", |a| {
        !a.calendar.loading && a.calendar.events.iter().any(|e| e.id == id)
    })
    .await;
    act(&mut app.calendar, Action::Event(id));
    wait_for_app(
        app,
        "details",
        |a| matches!(&a.calendar.modal, Some(Modal::Details(e)) if e.id == id),
    )
    .await;
    app.handle_input(b"e");
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
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

/// The whole arc of a post from the keyboard: `7`, `n`, a title with the
/// time in it, save, say you are in, and the board shows the count.
#[tokio::test]
async fn posting_to_the_board_and_saying_youre_in() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_poster").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-post-flow");
    app.show_splash = false;
    app.handle_input(b"7");
    assert_eq!(app.screen, Screen::Calendars);
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("Board shown"), "{rendered}");
    assert!(!rendered.contains("Upcoming events"));

    app.handle_input(b"nMovie night tomorrow at 9pm");
    // Account Settings opens over the editor and hands back to it untouched.
    app.handle_input(b"\x0f");
    assert!(app.show_settings);
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "account settings escape", |a| !a.show_settings).await;
    app.handle_input(b"\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!("no editor");
    };
    assert_eq!(e.text(0), "Movie night");
    assert_eq!(e.text(3), "21:00");
    // A draft is private until the poster flips it: Tab to the target, Space.
    assert_eq!(e.source, CalendarSource::Personal);
    app.handle_input(b"\t ");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!("no editor");
    };
    assert_eq!(e.source, CalendarSource::Board);
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "board post saved", |a| !a.calendar.pending).await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!("details after save");
    };
    let id = saved.id;
    assert!(saved.is_board());
    assert_eq!(saved.creator_name, "calendar_poster");
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("nobody's in yet"), "{rendered}");

    app.handle_input(b"i");
    wait_for_app(&mut app, "rsvp lands", |a| {
        !a.calendar.pending && a.calendar.going(id)
    })
    .await;
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("you're in"), "{rendered}");
    assert!(rendered.contains("I'm out"));
    app.handle_input(b"i");
    wait_for_app(&mut app, "rsvp taken back", |a| {
        !a.calendar.pending && !a.calendar.going(id)
    })
    .await;

    act(&mut app.calendar, Action::Cancel);
    assert!(app.calendar.modal.is_none());
    wait_for_app(&mut app, "post in the grid", |a| {
        !a.calendar.loading && a.calendar.events.iter().any(|e| e.id == id)
    })
    .await;
    // A right click on it offers the poster's full menu.
    let hit = event_hit(&mut app, id);
    mouse_click(&mut app, hit, 2);
    let menu = app.calendar.context_menu.as_ref().expect("context menu");
    assert_eq!(
        menu.items.len(),
        4,
        "open, in, edit, delete: {:?}",
        menu.items
    );
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "menu closes", |a| {
        a.calendar.context_menu.is_none()
    })
    .await;
}

/// A moderator sees a stranger's post with Delete and without Edit, and the
/// delete returns to the agenda with the selection moved on.
#[tokio::test]
async fn staff_take_a_post_down_from_its_details() {
    let db = new_test_db().await;
    let poster = create_test_user(&db.db, "calendar_stranger").await;
    let staff = create_test_user(&db.db, "calendar_mod").await;
    db.db
        .get()
        .await
        .unwrap()
        .execute(
            "UPDATE users SET is_moderator=true WHERE id=$1",
            &[&staff.id],
        )
        .await
        .unwrap();
    let mut app = make_app(db.db.clone(), staff.id, "calendar-staff-flow");
    app.show_splash = false;
    app.handle_input(b"7");
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    let date = app.calendar.selected;
    let post = CalendarStore::new(db.db.clone())
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &EventDraft {
                title: "Loud party".into(),
                description: String::new(),
                timing: EventTiming::AllDay {
                    start: date,
                    end_exclusive: date.succ_opt().unwrap(),
                },
            },
        )
        .await
        .unwrap()
        .event;
    app.calendar.refresh();
    wait_for_app(&mut app, "post loaded", |a| {
        !a.calendar.loading && a.calendar.events.iter().any(|e| e.id == post.id) && a.calendar.staff
    })
    .await;
    // Landing on the day marks its post; Enter opens it.
    app.calendar.select_date(date);
    app.handle_input(b"\r");
    wait_for_app(
        &mut app,
        "details",
        |a| matches!(&a.calendar.modal, Some(Modal::Details(e)) if e.id == post.id),
    )
    .await;
    app.handle_input(b"e");
    assert!(
        matches!(&app.calendar.modal, Some(Modal::Details(_))),
        "staff do not edit a stranger's words"
    );
    assert_eq!(
        app.calendar.error.as_deref(),
        Some("This event is not yours to change")
    );
    app.handle_input(b"d");
    assert!(matches!(app.calendar.modal, Some(Modal::Delete(_))));
    let rendered = render_plain(&mut app);
    assert!(
        rendered.contains("Posted by calendar_stranger"),
        "{rendered}"
    );
    app.handle_input(b"y");
    wait_for_app(&mut app, "post deleted", |a| !a.calendar.pending).await;
    assert!(app.calendar.modal.is_none());
    assert_ne!(app.calendar.selection, Selection::Event(post.id));
    assert_eq!(app.calendar.selected, date);
}

/// An event an hour out reaches the live strip and the Live panel, and
/// opening it from there lands on the board with its details up.
#[tokio::test]
async fn an_event_about_to_start_opens_from_the_live_strip() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_live").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-live-flow");
    app.show_splash = false;
    // Fifty-eight minutes out: the strip stamp (an hour before) is two
    // minutes old, inside the five the strip keeps something up.
    let start = Utc::now() + Duration::minutes(58);
    let event = CalendarStore::new(db.db.clone())
        .save(
            user.id,
            CalendarSource::Board,
            None,
            &EventDraft {
                title: "Standup".into(),
                description: String::new(),
                timing: EventTiming::Timed {
                    start,
                    end: Some(start + Duration::hours(1)),
                },
            },
        )
        .await
        .unwrap()
        .event;
    // The replica's board worker, nudged once as a write's notify would.
    let (signals, worker) = mpsc::unbounded_channel();
    let _worker = app.calendar.service().start_notify_worker(worker);
    signals.send(Signal::Resync).unwrap();
    app.handle_input(b"1");
    assert_eq!(app.screen, Screen::Dashboard);
    wait_for_app(&mut app, "strip shows the event", |a| {
        a.live.showing() == Some(LiveSource::BoardEvent(event.id))
    })
    .await;
    // The test database has no #lounge, so no card carries the strip; the
    // Live panel on the sidebar lists it with its start.
    let rendered = render_plain(&mut app);
    let time = start.format("%H:%M").to_string();
    assert!(
        rendered.contains("event") && rendered.contains(&time),
        "{rendered}"
    );
    assert!(crate::app::live::input::open_from_key(&mut app));
    assert_eq!(app.screen, Screen::Calendars);
    assert!(matches!(&app.calendar.modal, Some(Modal::Details(e)) if e.id == event.id));
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("On the board"), "{rendered}");
}

/// The editor reads its clock fields in the zone it opened in. Changing the
/// account timezone in Settings while it is open, then saving a title-only
/// change, leaves the event at the instant it was.
#[tokio::test]
async fn a_timezone_change_mid_edit_leaves_the_event_where_it_was() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_traveler").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-tz-flow");
    app.show_splash = false;
    set_timezone(&mut app, user.id, "Europe/Warsaw").await;
    let start = (Utc::now().date_naive() + Duration::days(3))
        .and_hms_opt(18, 0, 0)
        .unwrap()
        .and_utc();
    let store = CalendarStore::new(db.db.clone());
    let event = store
        .save(
            user.id,
            CalendarSource::Personal,
            None,
            &EventDraft {
                title: "Dentist".into(),
                description: String::new(),
                timing: EventTiming::Timed {
                    start,
                    end: Some(start + Duration::hours(1)),
                },
            },
        )
        .await
        .unwrap()
        .event;
    edit(&mut app, event.id).await;
    set_timezone(&mut app, user.id, "America/New_York").await;
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    app.handle_input(b"!\x13");
    wait_for_app(&mut app, "edit saved", |a| !a.calendar.pending).await;
    let saved = store.event(user.id, event.id).await.unwrap();
    assert_eq!(saved.title, "Dentist!");
    assert_eq!(saved.timing, event.timing);
}

/// A save that lost to a newer revision recovers from the keyboard alone:
/// Ctrl+L adopts the newer revision with the draft kept, and the next save
/// lands.
#[tokio::test]
async fn a_conflicted_save_reloads_from_the_keyboard() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_two_terminals").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-conflict-flow");
    app.show_splash = false;
    let day = Utc::now().date_naive() + Duration::days(1);
    let draft = |title: &str| EventDraft {
        title: title.into(),
        description: String::new(),
        timing: EventTiming::AllDay {
            start: day,
            end_exclusive: day.succ_opt().unwrap(),
        },
    };
    let store = CalendarStore::new(db.db.clone());
    let event = store
        .save(user.id, CalendarSource::Personal, None, &draft("Dinner"))
        .await
        .unwrap()
        .event;
    edit(&mut app, event.id).await;
    // The same account saves from another terminal first.
    let newer = store
        .save(
            user.id,
            CalendarSource::Personal,
            Some((event.id, event.revision)),
            &draft("Dinner at 8"),
        )
        .await
        .unwrap()
        .event;
    app.handle_input(b"!\x13");
    wait_for_app(&mut app, "save refused", |a| !a.calendar.pending).await;
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("Event changed elsewhere"), "{rendered}");
    assert!(rendered.contains("Ctrl+L reload"), "{rendered}");

    app.handle_input(b"\x0c");
    wait_for_app(&mut app, "newer revision adopted", |a| {
        matches!(
            &a.calendar.modal,
            Some(Modal::Editor(e)) if e.error.as_deref().is_some_and(|m| m.starts_with("Reloaded revision"))
        )
    })
    .await;
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "save lands", |a| !a.calendar.pending).await;
    let saved = store.event(user.id, event.id).await.unwrap();
    assert_eq!(saved.title, "Dinner!");
    assert_eq!(saved.revision, newer.revision + 1);
}
