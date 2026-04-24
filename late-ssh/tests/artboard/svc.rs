//! Service integration tests for artboard flows against the in-proc server and DB.

use chrono::NaiveDate;
use dartboard_core::Canvas;
use dartboard_core::{CanvasOp, Pos};
use dartboard_local::MAX_PLAYERS;
use late_core::models::artboard::Snapshot;
use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::artboard::svc::DartboardEvent;
use late_ssh::dartboard;

use super::{connected_service, helpers::new_test_db, shared_provenance, test_color, wait_for};

#[test]
fn services_share_canvas_updates() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let alice = connected_service(server.clone(), "alice", shared.clone());
    let bob = connected_service(server, "bob", shared);

    let alice_rx = alice.subscribe_state();
    let bob_rx = bob.subscribe_state();

    wait_for(|| {
        let snapshot = alice_rx.borrow().clone();
        (snapshot.your_user_id.is_some() && snapshot.peers.len() == 1).then_some(())
    });
    wait_for(|| {
        let snapshot = bob_rx.borrow().clone();
        (snapshot.your_user_id.is_some() && snapshot.peers.len() == 1).then_some(())
    });

    alice.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 3, y: 2 },
        ch: 'A',
        fg: test_color(),
    });

    wait_for(|| {
        let snapshot = bob_rx.borrow().clone();
        (snapshot.canvas.get(Pos { x: 3, y: 2 }) == 'A' && snapshot.last_seq >= 1).then_some(())
    });

    let snapshot = bob_rx.borrow().clone();
    assert_eq!(
        snapshot
            .provenance
            .username_at(&snapshot.canvas, Pos { x: 3, y: 2 }),
        Some("alice")
    );
}

#[test]
fn service_emits_peer_join_and_left() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let alice = connected_service(server.clone(), "alice", shared.clone());
    let mut alice_events = alice.subscribe_events();

    wait_for(|| {
        alice
            .subscribe_state()
            .borrow()
            .your_user_id
            .is_some()
            .then_some(())
    });

    let bob = connected_service(server, "bob", shared);

    let joined_peer = wait_for(|| match alice_events.try_recv() {
        Ok(DartboardEvent::PeerJoined { peer }) => Some(peer),
        Ok(_) => None,
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => None,
        Err(err) => panic!("unexpected broadcast error: {err:?}"),
    });
    assert_eq!(joined_peer.name, "bob");

    drop(bob);

    let left_user_id = wait_for(|| match alice_events.try_recv() {
        Ok(DartboardEvent::PeerLeft { user_id }) => Some(user_id),
        Ok(_) => None,
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => None,
        Err(err) => panic!("unexpected broadcast error: {err:?}"),
    });
    assert_eq!(left_user_id, joined_peer.user_id);
}

#[test]
fn eleventh_service_reports_connect_rejected() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();

    let mut clients = Vec::new();
    for i in 0..MAX_PLAYERS {
        let svc = connected_service(server.clone(), &format!("peer{i}"), shared.clone());
        let rx = svc.subscribe_state();
        wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
        clients.push(svc);
    }

    let overflow = connected_service(server, "overflow", shared);
    let rx = overflow.subscribe_state();
    let reason = wait_for(|| rx.borrow().connect_rejected.clone());
    assert!(reason.to_lowercase().contains("full"), "reason: {reason}");
    assert!(rx.borrow().your_user_id.is_none());
}

#[test]
fn unknown_replace_resyncs_provenance_from_shared_state() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let painter = connected_service(server.clone(), "painter", shared.clone());
    let rx = painter.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    painter.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 0, y: 0 },
        ch: 'A',
        fg: test_color(),
    });
    wait_for(|| {
        let snapshot = rx.borrow().clone();
        (snapshot.canvas.get(Pos { x: 0, y: 0 }) == 'A'
            && snapshot
                .provenance
                .username_at(&snapshot.canvas, Pos { x: 0, y: 0 })
                == Some("painter"))
        .then_some(())
    });

    {
        let mut provenance = shared.lock().expect("shared provenance lock");
        *provenance = ArtboardProvenance::default();
    }
    server.submit_op_for(
        0,
        0,
        CanvasOp::Replace {
            canvas: Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT),
        },
    );

    wait_for(|| {
        let snapshot = rx.borrow().clone();
        (snapshot.canvas.get(Pos { x: 0, y: 0 }) == ' ' && snapshot.last_seq >= 2).then_some(())
    });
    let snapshot = rx.borrow().clone();
    assert_eq!(snapshot.provenance, ArtboardProvenance::default());
    assert!(
        snapshot
            .provenance
            .username_at(&snapshot.canvas, Pos { x: 0, y: 0 })
            .is_none()
    );
}

#[tokio::test]
async fn persistent_server_saves_and_restores_snapshot() {
    let test_db = new_test_db().await;
    let shared = shared_provenance();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        None,
        shared.clone(),
        std::time::Duration::from_millis(50),
    );
    let painter = connected_service(server, "painter", shared.clone());
    let rx = painter.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    painter.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 5, y: 4 },
        ch: 'Z',
        fg: test_color(),
    });

    let persisted = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let client = test_db.db.get().await.expect("db client");
            if let Some(snapshot) = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
                .await
                .expect("query snapshot")
            {
                let canvas: dartboard_core::Canvas =
                    serde_json::from_value(snapshot.canvas).expect("deserialize canvas");
                let provenance: late_ssh::app::artboard::provenance::ArtboardProvenance =
                    serde_json::from_value(snapshot.provenance).expect("deserialize provenance");
                if canvas.get(Pos { x: 5, y: 4 }) == 'Z' {
                    break (canvas, provenance);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for artboard snapshot");
    assert_eq!(persisted.0.get(Pos { x: 5, y: 4 }), 'Z');
    assert_eq!(
        persisted.1.username_at(&persisted.0, Pos { x: 5, y: 4 }),
        Some("painter")
    );

    let restored = dartboard::load_persisted_artboard(&test_db.db)
        .await
        .expect("load persisted artboard");
    let restored_server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        restored.as_ref().map(|snapshot| snapshot.canvas.clone()),
        restored
            .as_ref()
            .map(|snapshot| snapshot.provenance.clone())
            .unwrap_or_default()
            .shared(),
        std::time::Duration::from_millis(50),
    );
    let restorer = connected_service(
        restored_server,
        "restorer",
        restored
            .map(|snapshot| snapshot.provenance)
            .unwrap_or_default()
            .shared(),
    );
    let restored_rx = restorer.subscribe_state();

    wait_for(|| {
        let snapshot = restored_rx.borrow().clone();
        (snapshot.your_user_id.is_some() && snapshot.canvas.get(Pos { x: 5, y: 4 }) == 'Z')
            .then_some(())
    });
}

#[tokio::test]
async fn flush_server_snapshot_persists_immediately() {
    let test_db = new_test_db().await;
    let shared = shared_provenance();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        None,
        shared.clone(),
        std::time::Duration::from_secs(60 * 60),
    );
    let painter = connected_service(server.clone(), "painter", shared.clone());
    let rx = painter.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    painter.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 9, y: 6 },
        ch: 'Q',
        fg: test_color(),
    });

    wait_for(|| {
        let snapshot = rx.borrow().clone();
        (snapshot.canvas.get(Pos { x: 9, y: 6 }) == 'Q' && snapshot.last_seq >= 1).then_some(())
    });

    dartboard::flush_server_snapshot(&test_db.db, &server, &shared)
        .await
        .expect("flush artboard snapshot");

    let client = test_db.db.get().await.expect("db client");
    let snapshot = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .expect("query snapshot")
        .expect("snapshot exists");
    let canvas: dartboard_core::Canvas =
        serde_json::from_value(snapshot.canvas).expect("deserialize canvas");
    let provenance: late_ssh::app::artboard::provenance::ArtboardProvenance =
        serde_json::from_value(snapshot.provenance).expect("deserialize provenance");
    assert_eq!(canvas.get(Pos { x: 9, y: 6 }), 'Q');
    assert_eq!(
        provenance.username_at(&canvas, Pos { x: 9, y: 6 }),
        Some("painter")
    );
}

#[tokio::test]
async fn rollover_saves_daily_snapshot_and_prunes_to_newest_seven() {
    let test_db = new_test_db().await;
    let shared = shared_provenance();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        None,
        shared.clone(),
        std::time::Duration::from_secs(60 * 60),
    );
    let painter = connected_service(server.clone(), "painter", shared.clone());
    let rx = painter.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
    painter.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 2, y: 3 },
        ch: 'D',
        fg: test_color(),
    });
    wait_for(|| (rx.borrow().canvas.get(Pos { x: 2, y: 3 }) == 'D').then_some(()));

    let client = test_db.db.get().await.expect("db client");
    for day in 20..=26 {
        let key = format!("daily:2026-04-{day:02}");
        Snapshot::upsert(
            &client,
            &key,
            serde_json::to_value(Canvas::with_size(4, 4)).expect("canvas json"),
            serde_json::to_value(ArtboardProvenance::default()).expect("provenance json"),
        )
        .await
        .expect("insert old daily snapshot");
    }

    dartboard::rollover_daily_snapshot_for_day(
        &test_db.db,
        &server,
        &shared,
        NaiveDate::from_ymd_opt(2026, 4, 28).expect("valid date"),
    )
    .await
    .expect("roll over daily artboard snapshot");

    let daily = Snapshot::list_by_board_key_prefix(&client, "daily:")
        .await
        .expect("list daily snapshots");
    let keys: Vec<_> = daily
        .iter()
        .map(|snapshot| snapshot.board_key.as_str())
        .collect();
    assert_eq!(
        keys,
        vec![
            "daily:2026-04-27",
            "daily:2026-04-26",
            "daily:2026-04-25",
            "daily:2026-04-24",
            "daily:2026-04-23",
            "daily:2026-04-22",
            "daily:2026-04-21",
        ]
    );

    let daily_snapshot = Snapshot::find_by_board_key(&client, "daily:2026-04-27")
        .await
        .expect("query daily snapshot")
        .expect("daily snapshot exists");
    let canvas: Canvas = serde_json::from_value(daily_snapshot.canvas).expect("canvas json");
    assert_eq!(canvas.get(Pos { x: 2, y: 3 }), 'D');
}

#[tokio::test]
async fn monthly_rollover_continues_when_daily_snapshot_already_exists() {
    let test_db = new_test_db().await;
    let shared = shared_provenance();
    let mut archived_canvas = Canvas::with_size(4, 4);
    archived_canvas.set(Pos { x: 1, y: 1 }, 'A');
    let mut archived_provenance = ArtboardProvenance::default();
    archived_provenance.set_username(Pos { x: 1, y: 1 }, "archivist");
    let client = test_db.db.get().await.expect("db client");
    Snapshot::upsert(
        &client,
        "daily:2026-04-30",
        serde_json::to_value(&archived_canvas).expect("canvas json"),
        serde_json::to_value(&archived_provenance).expect("provenance json"),
    )
    .await
    .expect("insert existing daily snapshot");

    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        None,
        shared.clone(),
        std::time::Duration::from_secs(60 * 60),
    );
    let painter = connected_service(server.clone(), "painter", shared.clone());
    let rx = painter.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
    painter.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 2, y: 2 },
        ch: 'L',
        fg: test_color(),
    });
    wait_for(|| (rx.borrow().canvas.get(Pos { x: 2, y: 2 }) == 'L').then_some(()));

    dartboard::rollover_daily_snapshot_for_day(
        &test_db.db,
        &server,
        &shared,
        NaiveDate::from_ymd_opt(2026, 5, 1).expect("valid date"),
    )
    .await
    .expect("roll over monthly artboard snapshot");

    let monthly = Snapshot::find_by_board_key(&client, "monthly:2026-04")
        .await
        .expect("query monthly snapshot")
        .expect("monthly snapshot exists");
    let monthly_canvas: Canvas = serde_json::from_value(monthly.canvas).expect("canvas json");
    let monthly_provenance: ArtboardProvenance =
        serde_json::from_value(monthly.provenance).expect("provenance json");
    assert_eq!(monthly_canvas.get(Pos { x: 1, y: 1 }), 'A');
    assert_eq!(monthly_canvas.get(Pos { x: 2, y: 2 }), ' ');
    assert_eq!(
        monthly_provenance.username_at(&monthly_canvas, Pos { x: 1, y: 1 }),
        Some("archivist")
    );

    let main = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .expect("query main snapshot")
        .expect("main snapshot exists");
    let main_canvas: Canvas = serde_json::from_value(main.canvas).expect("canvas json");
    assert_eq!(main_canvas.get(Pos { x: 2, y: 2 }), ' ');
    assert_eq!(server.canvas_snapshot().get(Pos { x: 2, y: 2 }), ' ');
}
