//! Service integration tests for artboard flows against the in-proc server and DB.

use dartboard_core::{CanvasOp, Pos};
use dartboard_local::MAX_PLAYERS;
use late_core::models::artboard::Snapshot;
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
