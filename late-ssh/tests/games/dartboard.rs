use std::{
    thread,
    time::{Duration, Instant},
};

use dartboard_core::{CanvasOp, Pos, RgbColor};
use dartboard_local::{InMemStore, MAX_PLAYERS, ServerHandle};
use late_ssh::app::games::dartboard::state::State;
use late_ssh::app::games::dartboard::svc::{DartboardEvent, DartboardService};
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

#[test]
fn dartboard_services_share_canvas_updates() {
    let server = ServerHandle::spawn_local(InMemStore);
    let alice = DartboardService::new(server.clone(), Uuid::now_v7(), "alice");
    let bob = DartboardService::new(server, Uuid::now_v7(), "bob");

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
}

#[test]
fn dartboard_service_emits_peer_join_and_left() {
    let server = ServerHandle::spawn_local(InMemStore);
    let alice = DartboardService::new(server.clone(), Uuid::now_v7(), "alice");
    let mut alice_events = alice.subscribe_events();

    wait_for(|| {
        alice
            .subscribe_state()
            .borrow()
            .your_user_id
            .is_some()
            .then_some(())
    });

    let bob = DartboardService::new(server, Uuid::now_v7(), "bob");

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
    let server = ServerHandle::spawn_local(InMemStore);

    let mut clients = Vec::new();
    for i in 0..MAX_PLAYERS {
        let svc = DartboardService::new(server.clone(), Uuid::now_v7(), &format!("peer{i}"));
        let rx = svc.subscribe_state();
        wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
        clients.push(svc);
    }

    let overflow = DartboardService::new(server, Uuid::now_v7(), "overflow");
    let rx = overflow.subscribe_state();
    let reason = wait_for(|| rx.borrow().connect_rejected.clone());
    assert!(reason.to_lowercase().contains("full"), "reason: {reason}");
    assert!(rx.borrow().your_user_id.is_none());
}

#[test]
fn dartboard_paste_bytes_lays_out_multiline_text_with_wrap() {
    let server = ServerHandle::spawn_local(InMemStore);
    let svc = DartboardService::new(server, Uuid::now_v7(), "painter");

    // Wait for Welcome so the snapshot carries the server's canvas + our color.
    let rx = svc.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    let mut state = State::new(svc);
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
