use std::{
    thread,
    time::{Duration, Instant},
};

use dartboard_core::{CanvasOp, Pos, RgbColor};
use dartboard_local::MAX_PLAYERS;
use late_core::{models::artboard::Snapshot, test_utils::test_db};
use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::artboard::state::State;
use late_ssh::app::artboard::svc::{DartboardEvent, DartboardService};
use late_ssh::dartboard;
use uuid::Uuid;

fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "condition not met before timeout"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn test_color() -> RgbColor {
    RgbColor::new(255, 110, 64)
}

fn shared_provenance() -> late_ssh::app::artboard::provenance::SharedArtboardProvenance {
    ArtboardProvenance::default().shared()
}

#[test]
fn dartboard_services_share_canvas_updates() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let alice = DartboardService::new(server.clone(), Uuid::now_v7(), "alice", shared.clone());
    let bob = DartboardService::new(server, Uuid::now_v7(), "bob", shared);

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
        snapshot.provenance.username_at(&snapshot.canvas, Pos { x: 3, y: 2 }),
        Some("alice")
    );
}

#[test]
fn dartboard_service_emits_peer_join_and_left() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let alice = DartboardService::new(server.clone(), Uuid::now_v7(), "alice", shared.clone());
    let mut alice_events = alice.subscribe_events();

    wait_for(|| {
        alice
            .subscribe_state()
            .borrow()
            .your_user_id
            .is_some()
            .then_some(())
    });

    let bob = DartboardService::new(server, Uuid::now_v7(), "bob", shared);

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
fn dartboard_eleventh_service_reports_connect_rejected() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();

    let mut clients = Vec::new();
    for i in 0..MAX_PLAYERS {
        let svc = DartboardService::new(
            server.clone(),
            Uuid::now_v7(),
            &format!("peer{i}"),
            shared.clone(),
        );
        let rx = svc.subscribe_state();
        wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
        clients.push(svc);
    }

    let overflow = DartboardService::new(server, Uuid::now_v7(), "overflow", shared);
    let rx = overflow.subscribe_state();
    let reason = wait_for(|| rx.borrow().connect_rejected.clone());
    assert!(reason.to_lowercase().contains("full"), "reason: {reason}");
    assert!(rx.borrow().your_user_id.is_none());
}

#[test]
fn dartboard_paste_bytes_lays_out_multiline_text_with_wrap() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let svc = DartboardService::new(server, Uuid::now_v7(), "painter", shared.clone());

    // Wait for Welcome so the snapshot carries the server's canvas + our color.
    let rx = svc.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    let mut state = State::new(svc, "painter".to_string(), shared);
    state.tick(); // drain the initial snapshot into local state

    // Start paste from (2, 1) so the wrap column is x=2 on the second line.
    state.set_viewport_for_screen((80, 24));
    for _ in 0..2 {
        state.move_right((80, 24));
    }
    state.move_down((80, 24));

    state.paste_bytes(b"hello\nworld", (80, 24));

    let canvas = &state.snapshot.canvas;
    assert_eq!(canvas.get(Pos { x: 2, y: 1 }), 'h');
    assert_eq!(canvas.get(Pos { x: 6, y: 1 }), 'o');
    assert_eq!(canvas.get(Pos { x: 2, y: 2 }), 'w');
    assert_eq!(canvas.get(Pos { x: 6, y: 2 }), 'd');
}

#[tokio::test]
async fn dartboard_persistent_server_saves_and_restores_snapshot() {
    let test_db = test_db().await;
    let shared = shared_provenance();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        None,
        shared.clone(),
        Duration::from_millis(50),
    );
    let painter = DartboardService::new(server, Uuid::now_v7(), "painter", shared.clone());
    let rx = painter.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    painter.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 5, y: 4 },
        ch: 'Z',
        fg: test_color(),
    });

    let persisted = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let client = test_db.db.get().await.expect("db client");
            if let Some(snapshot) = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
                .await
                .expect("query snapshot")
            {
                let canvas: dartboard_core::Canvas =
                    serde_json::from_value(snapshot.canvas).expect("deserialize canvas");
                let provenance: late_ssh::app::artboard::provenance::ArtboardProvenance =
                    serde_json::from_value(snapshot.provenance)
                        .expect("deserialize provenance");
                if canvas.get(Pos { x: 5, y: 4 }) == 'Z' {
                    break (canvas, provenance);
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for artboard snapshot");
    assert_eq!(persisted.0.get(Pos { x: 5, y: 4 }), 'Z');
    assert_eq!(persisted.1.username_at(&persisted.0, Pos { x: 5, y: 4 }), Some("painter"));

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
        Duration::from_millis(50),
    );
    let restorer = DartboardService::new(
        restored_server,
        Uuid::now_v7(),
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
async fn dartboard_flush_server_snapshot_persists_immediately() {
    let test_db = test_db().await;
    let shared = shared_provenance();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        None,
        shared.clone(),
        Duration::from_secs(60 * 60),
    );
    let painter = DartboardService::new(server.clone(), Uuid::now_v7(), "painter", shared.clone());
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
    assert_eq!(provenance.username_at(&canvas, Pos { x: 9, y: 6 }), Some("painter"));
}
