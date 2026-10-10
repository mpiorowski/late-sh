use super::{
    state::{CalendarState, Modal},
    svc::*,
};
use crate::{
    pg_listener::{Channel, PgListener, Signal},
    test_helpers::new_test_db,
};
use chrono::{Duration, Utc};
use late_core::{models::calendar::*, test_utils::create_test_user};
use tokio::{
    sync::{mpsc, watch},
    time::timeout,
};

async fn snapshot(svc: &CalendarService, viewer: uuid::Uuid, source: CalendarSource) -> Snapshot {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let today = Utc::now().date_naive();
    svc.load(
        Query {
            viewer,
            source,
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

#[tokio::test]
async fn calendar_ical_import_reviews_before_save_and_export_can_be_saved_as_new_event() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_ical_import").await;
    let svc = CalendarService::new(db.db.clone());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let input = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Imported café\nDTSTART:20261004T153027Z\nDURATION:PT45M\nEND:VEVENT\nEND:VCALENDAR";
    svc.import(input.into(), chrono_tz::UTC, 17, tx.clone());
    let reply = timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let Reply::Imported {
        generation: 17,
        result: Ok(mut candidates),
    } = reply
    else {
        panic!("{reply:?}");
    };
    let draft = candidates.remove(0).draft.unwrap();
    let count: i64 = db
        .db
        .get()
        .await
        .unwrap()
        .query_one("SELECT count(*) FROM calendar_events", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    svc.save(
        user.id,
        CalendarSource::Personal(user.id),
        None,
        draft,
        tx.clone(),
    );
    let reply = timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let Reply::Saved(Ok(saved)) = reply else {
        panic!("{reply:?}");
    };
    assert_eq!(saved.title, "Imported café");
    assert_eq!(saved.owner_id, Some(user.id));
    let exported = super::ical::export(&saved);
    let draft = super::ical::parse(&exported, chrono_tz::UTC)
        .unwrap()
        .remove(0)
        .draft
        .unwrap();
    let copy = svc
        .store
        .save(user.id, CalendarSource::Personal(user.id), None, &draft)
        .await
        .unwrap();
    assert_ne!(copy.id, saved.id);
    assert_eq!(copy.timing, saved.timing);
    assert_eq!(copy.title, saved.title);
    svc.import(
        "http://127.0.0.1/calendar.ics".into(),
        chrono_tz::UTC,
        18,
        tx,
    );
    let reply = timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        reply,
        Reply::Imported {
            generation: 18,
            result: Err(_)
        }
    ));
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
    .expect("server snapshot refreshed");
}
#[tokio::test]
async fn calendar_listener_reconstructs_notices_and_revocation_keeps_own_notices() {
    let db = new_test_db().await;
    let owner = create_test_user(&db.db, "calendar_shared").await;
    let viewer = create_test_user(&db.db, "calendar_session").await;
    db.db
        .get()
        .await
        .unwrap()
        .execute("UPDATE users SET is_admin=true WHERE id=$1", &[&viewer.id])
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
    let mut draft = EventDraft {
        title: "Notice".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start: Utc::now() + Duration::minutes(10),
            end: None,
        },
        notice_lead_seconds: Some(86400),
        mod_editable: false,
    };
    let server = svc
        .store
        .save(viewer.id, CalendarSource::Server, None, &draft)
        .await
        .unwrap();
    let mut notices = svc.server_notices();
    contains(&mut notices, server.id, true).await;
    // Kill only this isolated database's LISTEN connection, then change an
    // event while notifications are disconnected. Resync must reconstruct it.
    let client = db.db.get().await.unwrap();
    let pid: i32 = client.query_one("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND query LIKE 'LISTEN calendar_changed;%' AND pid<>pg_backend_pid()", &[]).await.unwrap().get(0);
    client
        .execute("SELECT pg_terminate_backend($1)", &[&pid])
        .await
        .unwrap();
    let mut disabled = draft.clone();
    disabled.notice_lead_seconds = None;
    let server = svc
        .store
        .save(
            viewer.id,
            CalendarSource::Server,
            Some((server.id, server.revision)),
            &disabled,
        )
        .await
        .unwrap();
    timeout(std::time::Duration::from_secs(10), async {
        while observed.recv().await.unwrap() != Signal::Resync {}
    })
    .await
    .expect("listener reconnects");
    contains(&mut notices, server.id, false).await;
    svc.store
        .delete(viewer.id, server.id, server.revision)
        .await
        .unwrap();
    contains(&mut notices, server.id, false).await;
    let own = svc
        .store
        .save(viewer.id, CalendarSource::Personal(viewer.id), None, &draft)
        .await
        .unwrap();
    draft.title = "Shared notice".into();
    let shared = svc
        .store
        .save(owner.id, CalendarSource::Personal(owner.id), None, &draft)
        .await
        .unwrap();
    let preferences = svc
        .store
        .save_preferences(
            owner.id,
            &CalendarPreferences {
                public: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let loaded = snapshot(&svc, viewer.id, CalendarSource::Personal(owner.id)).await;
    assert!(loaded.events.iter().any(|e| e.id == shared.id));
    assert_eq!(
        loaded
            .personal_notices
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![own.id]
    );
    let mut state = CalendarState::new(svc.clone(), viewer.id);
    state.source = CalendarSource::Personal(owner.id);
    // Consume earlier invalidations, then revoke access through the same
    // PostgreSQL notification path real sessions use.
    state.tick(false, chrono_tz::UTC);
    state.events = loaded.events;
    state.modal = Some(Modal::Details(shared));
    let mut epoch = svc.subscribe();
    epoch.borrow_and_update();
    svc.store
        .save_preferences(
            owner.id,
            &CalendarPreferences {
                public: false,
                ..preferences
            },
        )
        .await
        .unwrap();
    timeout(std::time::Duration::from_secs(10), epoch.changed())
        .await
        .unwrap()
        .unwrap();
    state.tick(false, chrono_tz::UTC);
    assert!(state.events.is_empty());
    assert!(state.modal.is_none());
    let loaded = snapshot(&svc, viewer.id, CalendarSource::Personal(owner.id)).await;
    assert!(loaded.events.is_empty());
    assert!(loaded.event_error.is_some());
    assert_eq!(
        loaded
            .personal_notices
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![own.id]
    );
    worker.abort();
    connection.abort();
}
