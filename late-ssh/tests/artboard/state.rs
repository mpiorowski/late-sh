//! State-level integration tests for artboard client behavior.

use dartboard_core::{Canvas, CanvasOp, Pos};
use late_core::models::artboard::Snapshot;
use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::artboard::state::State;
use late_ssh::app::artboard::svc::ArtboardSnapshotService;
use late_ssh::dartboard;

use super::{connected_service, helpers::new_test_db, shared_provenance, test_color, wait_for};

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
async fn snapshot_browser_activates_archive_readonly_and_returns_to_live() {
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

    let mut archive_canvas = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    archive_canvas.set(Pos { x: 0, y: 0 }, 'A');
    let mut archive_provenance = ArtboardProvenance::default();
    archive_provenance.set_username(Pos { x: 0, y: 0 }, "archivist");
    let client = test_db.db.get().await.expect("db client");
    Snapshot::upsert(
        &client,
        "daily:2026-04-23",
        serde_json::to_value(&archive_canvas).expect("canvas json"),
        serde_json::to_value(&archive_provenance).expect("provenance json"),
    )
    .await
    .expect("insert archive snapshot");

    let mut state = State::new(
        svc,
        ArtboardSnapshotService::new(test_db.db.clone()),
        "painter".to_string(),
        shared,
    );
    state.tick();
    state.open_snapshot_browser();
    wait_for_archive_load(&mut state).await;
    assert_eq!(state.snapshot_browser_items().len(), 1);

    state.move_snapshot_browser_selection(1);
    state.activate_snapshot_browser_selection();
    assert!(state.is_archive_view_active());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'A');
    state.type_char('X', (80, 24));
    assert_eq!(
        state.snapshot.canvas.get(Pos { x: 0, y: 0 }),
        'A',
        "historical archive view must stay read-only"
    );

    state.exit_archive_view();
    assert!(!state.is_archive_view_active());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'L');
}

async fn wait_for_archive_load(state: &mut State) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        state.tick();
        if !state.snapshot_browser_loading() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for archive snapshot browser load");
}
