//! State-level integration tests for artboard client behavior.

use dartboard_core::Pos;
use late_ssh::app::artboard::state::State;
use late_ssh::dartboard;

use super::{connected_service, shared_provenance, wait_for};

#[test]
fn paste_bytes_lays_out_multiline_text_with_wrap() {
    let server = dartboard::spawn_server();
    let shared = shared_provenance();
    let svc = connected_service(server, "painter", shared.clone());

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
