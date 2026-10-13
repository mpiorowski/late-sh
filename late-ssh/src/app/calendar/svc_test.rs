use super::{
    state::{CalendarState, Modal},
    svc::*,
};
use crate::{
    app::activity::{channel, event::ActivityKind, publisher::ActivityPublisher},
    pg_listener::{Channel, PgListener, Signal},
    test_helpers::new_test_db,
};
use chrono::{Duration, Utc};
use late_core::{models::calendar::*, test_utils::create_test_user};
use tokio::{
    sync::{mpsc, watch},
    time::timeout,
};

fn timed(from_now: Duration) -> EventDraft {
    EventDraft {
        title: "Movie night @everyone".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start: Utc::now() + from_now,
            end: Some(Utc::now() + from_now + Duration::hours(2)),
        },
    }
}

async fn snapshot(svc: &CalendarService, viewer: uuid::Uuid, board: bool) -> Snapshot {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let today = Utc::now().date_naive();
    svc.load(
        Query {
            viewer,
            board,
            from: today - Duration::days(1),
            to: today + Duration::days(2),
            tz: chrono_tz::UTC,
            generation: 1,
        },
        tx,
    );
    match timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Reply::Loaded { result: Ok(s), .. } => s,
        other => panic!("{other:?}"),
    }
}

async fn contains(rx: &mut watch::Receiver<Vec<CalendarEvent>>, id: uuid::Uuid, present: bool) {
    timeout(std::time::Duration::from_secs(10), async {
        loop {
            if rx.borrow_and_update().iter().any(|e| e.id == id) == present {
                break;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("board snapshot refreshed");
}

async fn next_payload(rx: &mut mpsc::UnboundedReceiver<Signal>) -> String {
    match timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Signal::Notify { payload, .. } => payload,
        Signal::Resync => panic!("the listener reconnected mid-test"),
    }
}

/// `calendar_changed` wakes every session, so it fires only for what
/// another session can see: a board post and an "I'm in". A personal
/// event's whole life and a sweep that claims nothing stay quiet.
#[tokio::test]
async fn only_board_changes_wake_the_sessions() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "cal_quiet").await;
    let store = CalendarStore::new(db.db.clone());
    let mut listener = PgListener::new();
    let mut observed = listener.subscribe(&[Channel::CalendarChanged]);
    let connection = listener.start(db.db.config().clone());
    assert_eq!(
        timeout(std::time::Duration::from_secs(10), observed.recv())
            .await
            .unwrap()
            .unwrap(),
        Signal::Resync
    );
    let own = store
        .save(
            user.id,
            CalendarSource::Personal,
            None,
            &timed(Duration::days(2)),
        )
        .await
        .unwrap()
        .event;
    let own = store
        .save(
            user.id,
            CalendarSource::Personal,
            Some((own.id, own.revision)),
            &timed(Duration::days(3)),
        )
        .await
        .unwrap()
        .event;
    store.delete(user.id, own.id, own.revision).await.unwrap();
    assert!(store.claim_started(Utc::now()).await.unwrap().is_empty());
    // Notifications arrive in commit order: a marker sent after all of the
    // above arrives first only if none of it notified.
    db.db
        .get()
        .await
        .unwrap()
        .execute("SELECT pg_notify('calendar_changed', 'marker')", &[])
        .await
        .unwrap();
    assert_eq!(next_payload(&mut observed).await, "marker");

    let post = store
        .save(
            user.id,
            CalendarSource::Board,
            None,
            &timed(Duration::days(2)),
        )
        .await
        .unwrap()
        .event;
    assert_eq!(next_payload(&mut observed).await, "");
    store.set_rsvp(user.id, post.id, true).await.unwrap();
    assert_eq!(next_payload(&mut observed).await, "");
    connection.abort();
}

/// The minute tick re-reads the board alone, without invalidating every
/// session; a notify invalidates them and re-reads the board without
/// blanking it first, so the strip and the panel keep what they show.
#[tokio::test]
async fn the_board_stays_up_and_the_minute_tick_stays_quiet() {
    let db = new_test_db().await;
    let poster = create_test_user(&db.db, "cal_steady").await;
    let svc = CalendarService::new(db.db.clone());
    let post = svc
        .store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &timed(Duration::minutes(30)),
        )
        .await
        .unwrap()
        .event;
    let mut epoch = svc.subscribe();
    let mut board = svc.board_live();
    let (signals, worker_signals) = mpsc::unbounded_channel();
    // The worker's first minute tick fires at once.
    let worker = svc.start_notify_worker(worker_signals);
    contains(&mut board, post.id, true).await;
    assert!(
        !epoch.has_changed().unwrap(),
        "the minute's re-read invalidated every session"
    );

    signals
        .send(Signal::Notify {
            channel: Channel::CalendarChanged,
            payload: String::new(),
        })
        .unwrap();
    timeout(std::time::Duration::from_secs(10), epoch.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(
        board.borrow().iter().any(|e| e.id == post.id),
        "the board blanked while the notify re-read it"
    );
    worker.abort();
}

/// The two lines the board ships into #lounge: a post, as the poster's own
/// line with the title made mention-safe, and the start, claimed by the
/// sweeper once with the count.
#[tokio::test]
async fn a_post_and_its_start_each_ship_one_line() {
    let db = new_test_db().await;
    let poster = create_test_user(&db.db, "cal_poster").await;
    let guest = create_test_user(&db.db, "cal_guest").await;
    let (activity_tx, mut activity_rx) = channel::new(16);
    let svc = CalendarService::new(db.db.clone())
        .with_activity(ActivityPublisher::new(db.db.clone(), activity_tx));
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.save(
        poster.id,
        CalendarSource::Board,
        None,
        timed(-Duration::minutes(5)),
        tx.clone(),
    );
    let posted = match timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Reply::Saved(Ok(event)) => event,
        other => panic!("{other:?}"),
    };
    let line = timeout(std::time::Duration::from_secs(10), activity_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(line.username, "cal_poster");
    assert!(
        matches!(&line.kind, ActivityKind::EventPosted { event_id, title, .. } if *event_id == posted.id && title == "Movie night everyone"),
        "{line:?}"
    );
    assert!(
        line.action
            .starts_with("posted Movie night everyone to the board")
    );
    assert!(!line.action.contains('@'));

    // An edit is not a post.
    let mut edit = timed(-Duration::minutes(5));
    edit.title = "Movie night, renamed".into();
    svc.save(
        poster.id,
        CalendarSource::Board,
        Some((posted.id, posted.revision)),
        edit,
        tx.clone(),
    );
    assert!(matches!(
        timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap(),
        Reply::Saved(Ok(_))
    ));
    assert!(activity_rx.try_recv().is_err(), "an edit ships no line");

    svc.rsvp(guest.id, posted.id, true, tx.clone());
    assert!(matches!(
        timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap(),
        Reply::Rsvp(Ok(event)) if event.going == 1
    ));
    // Nobody ever says anything about an "I'm in".
    assert!(activity_rx.try_recv().is_err());

    // The sweeper claims the start once: run two sweeps, get one line.
    svc.sweep_for_test().await;
    svc.sweep_for_test().await;
    let line = timeout(std::time::Duration::from_secs(10), activity_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(line.username, "board");
    assert!(
        matches!(&line.kind, ActivityKind::EventStarting { event_id, going, .. } if *event_id == posted.id && *going == 1),
        "{line:?}"
    );
    assert_eq!(line.action, "Movie night, renamed is on, 1 in");
    assert!(activity_rx.try_recv().is_err(), "claimed once, told once");
}

/// A refusal comes back as the user's own words and nothing is written; the
/// load that follows carries the viewer's role, their own events and what
/// they are in.
#[tokio::test]
async fn refusals_are_told_and_a_load_carries_the_viewers_side() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "cal_user").await;
    let staff = create_test_user(&db.db, "cal_staff").await;
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
    let svc = CalendarService::new(db.db.clone());
    let post = svc
        .store
        .save(
            staff.id,
            CalendarSource::Board,
            None,
            &timed(Duration::hours(3)),
        )
        .await
        .unwrap()
        .event;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.delete(user.id, post.id, post.revision, tx.clone());
    assert!(matches!(
        timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap(),
        Reply::Deleted(Err(message)) if message == CalendarRefusal::ReadOnly.message()
    ));
    let mut bad = timed(Duration::hours(3));
    bad.title = "   ".into();
    svc.save(user.id, CalendarSource::Board, None, bad, tx.clone());
    assert!(matches!(
        timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap(),
        Reply::Saved(Err(SaveError::Other(message))) if message == "Title is required"
    ));
    svc.store
        .save(
            user.id,
            CalendarSource::Personal,
            None,
            &timed(Duration::hours(1)),
        )
        .await
        .unwrap();
    svc.store.set_rsvp(user.id, post.id, true).await.unwrap();

    let loaded = snapshot(&svc, user.id, true).await;
    assert!(!loaded.staff);
    assert_eq!(loaded.rsvps, vec![post.id]);
    assert!(
        loaded
            .events
            .iter()
            .any(|e| e.id == post.id && e.going == 1)
    );
    let hidden = snapshot(&svc, user.id, false).await;
    assert!(
        !hidden.events.iter().any(|e| e.is_board()),
        "the board hidden is the viewer's own alone"
    );
    assert!(snapshot(&svc, staff.id, true).await.staff);
}

/// The listener keeps the board's shared snapshot in step, rebuilds it after
/// a dropped connection, and a session with a post open sees staff take it
/// down under them.
#[tokio::test]
async fn the_listener_keeps_the_board_in_step_and_closes_a_deleted_post() {
    let db = new_test_db().await;
    let poster = create_test_user(&db.db, "cal_poster").await;
    let staff = create_test_user(&db.db, "cal_staff").await;
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
    let svc = CalendarService::new(db.db.clone());
    let mut listener = PgListener::new();
    let signals = listener.subscribe(&[Channel::CalendarChanged]);
    let mut observed = listener.subscribe(&[Channel::CalendarChanged]);
    let worker = svc.start_notify_worker(signals);
    let connection = listener.start(db.db.config().clone());
    assert_eq!(
        timeout(std::time::Duration::from_secs(10), observed.recv())
            .await
            .unwrap()
            .unwrap(),
        Signal::Resync
    );
    let post = svc
        .store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &timed(Duration::minutes(30)),
        )
        .await
        .unwrap()
        .event;
    let mut board = svc.board_live();
    contains(&mut board, post.id, true).await;

    // Kill only this isolated database's LISTEN connection, then move the
    // event out of the live window while notifications are down. Resync
    // must reconstruct the snapshot.
    let client = db.db.get().await.unwrap();
    let pid: i32 = client.query_one("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND query LIKE 'LISTEN calendar_changed;%' AND pid<>pg_backend_pid()", &[]).await.unwrap().get(0);
    client
        .execute("SELECT pg_terminate_backend($1)", &[&pid])
        .await
        .unwrap();
    let post = svc
        .store
        .save(
            poster.id,
            CalendarSource::Board,
            Some((post.id, post.revision)),
            &timed(Duration::days(3)),
        )
        .await
        .unwrap()
        .event;
    timeout(std::time::Duration::from_secs(10), async {
        while observed.recv().await.unwrap() != Signal::Resync {}
    })
    .await
    .expect("listener reconnects");
    contains(&mut board, post.id, false).await;

    // A session has the post open; staff take it down; the session's
    // details close through the same notification path real sessions use.
    let mut state = CalendarState::new(svc.clone(), poster.id);
    state.tick(false, chrono_tz::UTC);
    state.modal = Some(Modal::Details(post.clone()));
    let mut epoch = svc.subscribe();
    epoch.borrow_and_update();
    svc.store
        .delete(staff.id, post.id, post.revision)
        .await
        .unwrap();
    timeout(std::time::Duration::from_secs(10), epoch.changed())
        .await
        .unwrap()
        .unwrap();
    state.tick(false, chrono_tz::UTC);
    timeout(std::time::Duration::from_secs(10), async {
        loop {
            state.tick(false, chrono_tz::UTC);
            if state.modal.is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the deleted post's details close");
    assert_eq!(
        state.error.as_deref(),
        Some(CalendarRefusal::Gone.message().as_str())
    );
    worker.abort();
    connection.abort();
}
