//! State-level integration tests for artboard client behavior.

use crate::app::artboard::gallery::svc::GalleryService;
use crate::app::artboard::provenance::ArtboardProvenance;
use crate::app::artboard::state::State;
use crate::app::artboard::svc::{ArtboardSnapshotKind, ArtboardSnapshotService};
use crate::dartboard;
use dartboard_core::{Canvas, CanvasOp, Pos};
use late_core::models::artboard::Snapshot;

use super::test_support::{connected_service, shared_provenance, test_color, wait_for};
use crate::test_helpers::new_test_db;

#[test]
fn paste_bytes_lays_out_multiline_text_with_wrap() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let svc = connected_service(server, "painter", shared.clone());

    // Wait for Welcome so the snapshot carries the server's canvas + our color.
    let rx = svc.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));

    let mut state = State::new(
        svc,
        ArtboardSnapshotService::disabled(),
        GalleryService::disabled(),
        uuid::Uuid::nil(),
        "painter".to_string(),
        shared,
    );
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
async fn the_archive_list_loads_keys_first_and_the_cursor_loads_one_board() {
    let test_db = new_test_db().await;
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let svc = connected_service(server, "painter", shared.clone());
    let rx = svc.subscribe_state();
    wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
    svc.submit_op(CanvasOp::PaintCell {
        pos: Pos { x: 0, y: 0 },
        ch: 'L',
        fg: test_color(),
    });
    wait_for(|| (rx.borrow().canvas.get(Pos { x: 0, y: 0 }) == 'L').then_some(()));

    let mut older = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    older.set(Pos { x: 0, y: 0 }, 'A');
    let mut newer = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    newer.set(Pos { x: 0, y: 0 }, 'B');
    let mut provenance = ArtboardProvenance::default();
    provenance.set_username(Pos { x: 0, y: 0 }, "archivist");
    let client = test_db.db.get().await.expect("db client");
    for (key, canvas) in [("daily:2026-04-22", &older), ("daily:2026-04-23", &newer)] {
        Snapshot::upsert(
            &client,
            key,
            serde_json::to_value(canvas).expect("canvas json"),
            serde_json::to_value(&provenance).expect("provenance json"),
        )
        .await
        .expect("insert daily snapshot");
    }
    let mut curated = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    curated.set(Pos { x: 0, y: 0 }, 'C');
    Snapshot::upsert(
        &client,
        "curated:opening-night",
        serde_json::to_value(&curated).expect("canvas json"),
        serde_json::to_value(&provenance).expect("provenance json"),
    )
    .await
    .expect("insert curated snapshot");

    let mut state = State::new(
        svc,
        ArtboardSnapshotService::new(test_db.db.clone()),
        GalleryService::disabled(),
        uuid::Uuid::nil(),
        "painter".to_string(),
        shared,
    );
    state.tick();

    // Opening Daily lists the keys newest first and puts the newest on
    // the board without being asked.
    state.open_archive_list(ArtboardSnapshotKind::Daily);
    assert_eq!(
        state.browsed_archive_kind(),
        Some(ArtboardSnapshotKind::Daily)
    );
    wait_for_archive(&mut state, |state| {
        state.archive_count(ArtboardSnapshotKind::Daily).is_some()
    })
    .await;
    let labels: Vec<&str> = state
        .archive_entries(ArtboardSnapshotKind::Daily)
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, vec!["2026-04-23", "2026-04-22"]);
    // Every kind's keys are listed on entry, so the rail's numbers are
    // there before a list is opened.
    wait_for_archive(&mut state, |state| {
        state.archive_count(ArtboardSnapshotKind::Curated).is_some()
    })
    .await;
    assert_eq!(state.archive_count(ArtboardSnapshotKind::Curated), Some(1));
    wait_for_archive(&mut state, |state| state.is_archive_view_active()).await;
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'B');

    // The cursor is the time machine: down one day, the board follows.
    state.archive_move(1);
    wait_for_archive(&mut state, |state| {
        state.snapshot.canvas.get(Pos { x: 0, y: 0 }) == 'A'
    })
    .await;
    state.type_char('X', (80, 24));
    assert_eq!(
        state.snapshot.canvas.get(Pos { x: 0, y: 0 }),
        'A',
        "an archive on the board must stay read-only"
    );

    // Back up: the day already seen comes from the cache, no fetch.
    state.archive_move(-1);
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'B');
    assert_eq!(state.archive_loading_key(), None);

    // Esc keeps the archive on the board; the Board row brings live back.
    state.close_archive_list();
    assert!(state.is_archive_view_active());
    assert_eq!(state.browsed_archive_kind(), None);
    state.exit_archive_view();
    assert!(!state.is_archive_view_active());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'L');

    // Curated names are keys too.
    state.open_archive_list(ArtboardSnapshotKind::Curated);
    wait_for_archive(&mut state, |state| {
        state.snapshot.canvas.get(Pos { x: 0, y: 0 }) == 'C'
    })
    .await;
    assert_eq!(
        state.archive_entries(ArtboardSnapshotKind::Curated)[0].label,
        "opening-night"
    );
}

async fn wait_for_archive(state: &mut State, done: impl Fn(&State) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        state.tick();
        if done(state) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for the archive");
}
