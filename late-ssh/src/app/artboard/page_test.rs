use dartboard_core::{Canvas, Pos};

use super::*;
use crate::app::artboard::{
    provenance::ArtboardProvenance,
    state::{PAINT_PALETTE, State},
    svc::{ArtboardSnapshotService, DartboardService, DartboardSnapshot},
};

#[test]
fn view_mode_right_drag_reuses_editor_pan_behavior() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(200, 200);
    state.set_viewport_for_screen((80, 24));
    state.editor.viewport_origin = Pos { x: 20, y: 10 };

    assert!(handle_view_mode_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Right),
            x: 10,
            y: 10,
            modifiers: Default::default(),
        },
    ));
    assert_eq!(
        state.editor.pan_drag.expect("pan drag").origin,
        Pos { x: 20, y: 10 }
    );

    assert!(handle_view_mode_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Drag,
            button: Some(MouseButton::Right),
            x: 6,
            y: 7,
            modifiers: Default::default(),
        },
    ));
    assert_eq!(state.viewport_origin(), Pos { x: 24, y: 13 });

    assert!(handle_view_mode_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Up,
            button: Some(MouseButton::Right),
            x: 6,
            y: 7,
            modifiers: Default::default(),
        },
    ));
    assert!(state.editor.pan_drag.is_none());
}

#[test]
fn view_mode_right_click_ignores_non_canvas_hits() {
    let mut state = test_state();

    assert!(!handle_view_mode_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Right),
            x: 80,
            y: 1,
            modifiers: Default::default(),
        },
    ));
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 0, y: 0 });
}

#[test]
fn view_mode_alt_arrow_pans_viewport_without_moving_cursor() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(200, 200);
    state.set_viewport_for_screen((80, 24));
    state.editor.viewport_origin = Pos { x: 20, y: 10 };
    state.editor.cursor = Pos { x: 25, y: 12 };

    let event = ParsedInput::AltArrow(b'C');
    match event {
        ParsedInput::AltArrow(key) => match key {
            b'A' => state.pan_viewport_by((80, 24), 0, -VIEW_MODE_ALT_PAN_STEP),
            b'B' => state.pan_viewport_by((80, 24), 0, VIEW_MODE_ALT_PAN_STEP),
            b'C' => state.pan_viewport_by((80, 24), VIEW_MODE_ALT_PAN_STEP, 0),
            b'D' => state.pan_viewport_by((80, 24), -VIEW_MODE_ALT_PAN_STEP, 0),
            _ => {}
        },
        _ => unreachable!(),
    }

    assert_eq!(state.viewport_origin(), Pos { x: 24, y: 10 });
    assert_eq!(state.cursor(), Pos { x: 25, y: 12 });
}

fn test_state() -> State {
    let shared_provenance = ArtboardProvenance::default().shared();
    let snapshot = DartboardSnapshot {
        provenance: ArtboardProvenance::default(),
        your_user_id: Some(1),
        your_color: Some(PAINT_PALETTE[1]),
        ..Default::default()
    };
    let svc = DartboardService::disconnected_for_tests(snapshot);
    State::new(
        svc,
        ArtboardSnapshotService::disabled(),
        "viewer".to_string(),
        shared_provenance,
    )
}
