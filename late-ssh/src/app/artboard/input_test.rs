use super::*;
use crate::app::artboard::provenance::ArtboardProvenance;
use crate::app::artboard::state::PAINT_PALETTE;
use crate::app::artboard::svc::{ArtboardSnapshotService, DartboardService, DartboardSnapshot};
use dartboard_core::{Canvas, CellValue};
use dartboard_editor::Clipboard;

#[test]
fn hover_motion_does_not_move_cursor() {
    let mut state = test_state();
    state.editor.cursor = dartboard_core::Pos { x: 4, y: 3 };

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Moved,
            button: None,
            x: 18,
            y: 12,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(action, InputAction::Ignored));
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 4, y: 3 });
}

#[test]
fn raw_control_bytes_map_to_expected_app_keys() {
    assert_eq!(
        app_key_from_raw_control_byte(0x00),
        Some(AppKey {
            code: AppKeyCode::Char(' '),
            modifiers: AppModifiers {
                ctrl: true,
                ..Default::default()
            },
        })
    );
    for byte in [0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0F, 0x15, 0x19] {
        assert_eq!(app_key_from_raw_control_byte(byte), None);
    }
    assert_eq!(app_key_from_raw_control_byte(0x0D), None);
    assert_eq!(app_key_from_raw_control_byte(0x1B), None);
}

#[test]
fn mouse_pointer_translation_converts_sgr_coords_and_modifiers() {
    let pointer = app_pointer_event_from_mouse(&MouseEvent {
        kind: MouseEventKind::Drag,
        button: Some(MouseButton::Right),
        x: 7,
        y: 5,
        modifiers: crate::app::input::MouseModifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        },
    })
    .expect("pointer event");

    assert_eq!(pointer.column, 6);
    assert_eq!(pointer.row, 4);
    assert_eq!(pointer.kind, AppPointerKind::Drag(AppPointerButton::Right));
    assert!(pointer.modifiers.shift);
    assert!(pointer.modifiers.ctrl);
    assert!(!pointer.modifiers.alt);
}

#[test]
fn floating_hover_motion_tracks_preview_cursor() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(40, 20);
    state.set_viewport_for_screen((80, 24));
    state.editor.cursor = dartboard_core::Pos { x: 1, y: 1 };
    state.editor.floating = Some(dartboard_editor::FloatingSelection {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        transparent: false,
        source_index: Some(0),
    });

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Moved,
            button: None,
            x: 20,
            y: 14,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 18, y: 12 });
}

#[test]
fn swatch_overlay_pointer_events_keep_preview_active_until_canvas_move() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(40, 20);
    state.set_viewport_for_screen((80, 24));
    state.editor.cursor = dartboard_core::Pos { x: 12, y: 7 };
    state.editor.swatches[0] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(3, 3, vec![Some(CellValue::Narrow('A')); 9]),
        pinned: false,
    });

    let down = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 21,
            y: 20,
            modifiers: Default::default(),
        },
    );
    assert!(matches!(down, InputAction::Handled));
    assert!(state.floating_view().is_some());

    let up = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Up,
            button: Some(MouseButton::Left),
            x: 21,
            y: 20,
            modifiers: Default::default(),
        },
    );
    assert!(matches!(up, InputAction::Handled));
    assert!(state.floating_view().is_some());

    let moved_over_swatch = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Moved,
            button: None,
            x: 21,
            y: 20,
            modifiers: Default::default(),
        },
    );
    assert!(matches!(moved_over_swatch, InputAction::Handled));
    assert!(state.floating_view().is_some());

    let moved_over_canvas = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Moved,
            button: None,
            x: 20,
            y: 14,
            modifiers: Default::default(),
        },
    );
    assert!(matches!(moved_over_canvas, InputAction::Handled));
    let floating = state.floating_view().expect("floating preview shown");
    assert_eq!(floating.anchor, dartboard_core::Pos { x: 18, y: 12 });
}

#[test]
fn plain_click_on_wide_continuation_keeps_logical_cursor_on_cell_two() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(10, 4);
    state.set_viewport_for_screen((80, 24));
    let _ = state
        .snapshot
        .canvas
        .put_glyph(dartboard_core::Pos { x: 0, y: 0 }, '👍');

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 3,
            y: 2,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(
        action,
        InputAction::Handled | InputAction::Ignored
    ));
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 1, y: 0 });
}

#[test]
fn ctrl_click_swatch_body_clears_slot() {
    let mut state = test_state();
    state.editor.swatches[0] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 21,
            y: 20,
            modifiers: crate::app::input::MouseModifiers {
                ctrl: true,
                ..Default::default()
            },
        },
    );

    assert!(matches!(action, InputAction::Handled));
    assert!(state.swatches()[0].is_none());
}

#[test]
fn ctrl_click_active_swatch_clears_slot_and_dismisses_floating() {
    let mut state = test_state();
    state.editor.swatches[0] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });
    state.activate_swatch(0);
    assert!(state.has_floating());

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 21,
            y: 20,
            modifiers: crate::app::input::MouseModifiers {
                ctrl: true,
                ..Default::default()
            },
        },
    );

    assert!(matches!(action, InputAction::Handled));
    assert!(state.swatches()[0].is_none());
    assert!(!state.has_floating());
}

#[test]
fn double_click_canvas_glyph_arms_temp_brush() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(10, 4);
    state
        .snapshot
        .canvas
        .set(dartboard_core::Pos { x: 0, y: 0 }, 'x');

    let first_down = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 2,
            y: 2,
            modifiers: Default::default(),
        },
    );
    let first_up = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Up,
            button: Some(MouseButton::Left),
            x: 2,
            y: 2,
            modifiers: Default::default(),
        },
    );
    assert!(matches!(
        first_up,
        InputAction::Handled | InputAction::Ignored
    ));

    let second_down = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 2,
            y: 2,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(
        first_down,
        InputAction::Handled | InputAction::Ignored
    ));
    assert!(matches!(second_down, InputAction::Handled));
    assert_eq!(
        state.brush_mode(),
        crate::app::artboard::state::BrushMode::Glyph('x')
    );
    let floating = state
        .floating_view()
        .expect("temp brush floating preview shown");
    assert_eq!(floating.anchor, dartboard_core::Pos { x: 0, y: 0 });
}

#[test]
fn double_click_canvas_wide_glyph_from_continuation_arms_temp_brush() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(10, 4);
    let _ = state
        .snapshot
        .canvas
        .put_glyph(dartboard_core::Pos { x: 0, y: 0 }, '👍');

    let first_down = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 3,
            y: 2,
            modifiers: Default::default(),
        },
    );
    let first_up = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Up,
            button: Some(MouseButton::Left),
            x: 3,
            y: 2,
            modifiers: Default::default(),
        },
    );
    assert!(matches!(
        first_up,
        InputAction::Handled | InputAction::Ignored
    ));

    let second_down = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::Down,
            button: Some(MouseButton::Left),
            x: 3,
            y: 2,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(
        first_down,
        InputAction::Handled | InputAction::Ignored
    ));
    assert!(matches!(second_down, InputAction::Handled));
    assert_eq!(
        state.brush_mode(),
        crate::app::artboard::state::BrushMode::Glyph('👍')
    );
    let floating = state
        .floating_view()
        .expect("temp brush floating preview shown");
    assert_eq!(floating.anchor, dartboard_core::Pos { x: 0, y: 0 });
    assert_eq!(floating.width, 2);
}

#[test]
fn raw_ctrl_b_draws_selection_border() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 3);
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    state.move_down((80, 24));

    let action = handle_byte(&mut state, (80, 24), 0x02);

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(
        state
            .snapshot
            .canvas
            .cell(dartboard_core::Pos { x: 0, y: 0 }),
        Some(CellValue::Narrow('.'))
    );
    assert_eq!(
        state
            .snapshot
            .canvas
            .cell(dartboard_core::Pos { x: 1, y: 1 }),
        Some(CellValue::Narrow('\''))
    );
}

#[test]
fn raw_ctrl_space_smart_fills_selection() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 3);
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    state.move_down((80, 24));

    let action = handle_byte(&mut state, (80, 24), 0x00);

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 0, y: 0 }),
        '*'
    );
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 1, y: 1 }),
        '*'
    );
}

#[test]
fn raw_lf_is_ignored_after_push_pull_removal() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(3, 3);
    state
        .snapshot
        .canvas
        .set(dartboard_core::Pos { x: 0, y: 0 }, 'A');

    let action = handle_byte(&mut state, (80, 24), b'\n');

    assert!(matches!(action, InputAction::Ignored));
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 0, y: 0 }),
        'A'
    );
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 0, y: 1 }),
        ' '
    );
}

#[test]
fn raw_enter_stamps_floating_without_dismissing_it() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(5, 3);
    state
        .snapshot
        .canvas
        .set(dartboard_core::Pos { x: 1, y: 1 }, 'A');
    state.editor.cursor = dartboard_core::Pos { x: 1, y: 1 };
    state.begin_selection_from_cursor();
    assert!(state.lift_selection_to_floating());
    state.editor.cursor = dartboard_core::Pos { x: 3, y: 0 };

    let action = handle_byte(&mut state, (80, 24), b'\r');

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 3, y: 0 }),
        'A'
    );
    assert!(state.has_floating());
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 1, y: 1 }),
        'A'
    );
}

#[test]
fn ctrl_shift_arrow_strokes_floating_brush_and_keeps_it_active() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(6, 3);
    state
        .snapshot
        .canvas
        .set(dartboard_core::Pos { x: 0, y: 0 }, 'A');
    state.editor.cursor = dartboard_core::Pos { x: 0, y: 0 };
    state.begin_selection_from_cursor();
    assert!(state.lift_selection_to_floating());
    state.editor.cursor = dartboard_core::Pos { x: 2, y: 1 };

    let action = handle_event(&mut state, (80, 24), &ParsedInput::CtrlShiftArrow(b'C'));

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 3, y: 1 });
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 2, y: 1 }),
        'A'
    );
    assert_eq!(
        state
            .snapshot
            .canvas
            .get(dartboard_core::Pos { x: 3, y: 1 }),
        'A'
    );
    assert!(state.has_floating());
}

#[test]
fn ctrl_p_toggles_help_overlay() {
    let mut state = test_state();

    let open = handle_byte(&mut state, (80, 24), 0x10);
    let close = handle_byte(&mut state, (80, 24), 0x10);

    assert!(matches!(open, InputAction::Handled));
    assert!(matches!(close, InputAction::Handled));
    assert!(!state.is_help_open());
}

#[test]
fn ctrl_u_and_ctrl_y_cycle_paint_color() {
    let mut state = test_state();

    let prev = handle_byte(&mut state, (80, 24), 0x15);
    assert!(matches!(prev, InputAction::Handled));
    assert_eq!(state.active_paint_color_index(), 0);

    let next = handle_byte(&mut state, (80, 24), 0x19);
    assert!(matches!(next, InputAction::Handled));
    assert_eq!(state.active_paint_color_index(), 1);
}

#[test]
fn help_overlay_routes_navigation_keys() {
    let mut state = test_state();
    assert!(matches!(
        handle_byte(&mut state, (80, 24), 0x10),
        InputAction::Handled
    ));

    assert!(matches!(
        handle_byte(&mut state, (80, 24), b'\t'),
        InputAction::Handled
    ));
    assert_eq!(
        state.help_tab(),
        crate::app::artboard::state::HelpTab::Drawing
    );

    assert!(matches!(
        handle_event(&mut state, (80, 24), &ParsedInput::PageDown),
        InputAction::Handled
    ));
    assert_eq!(state.help_scroll(), 5);

    assert!(matches!(
        handle_event(&mut state, (80, 24), &ParsedInput::Home),
        InputAction::Handled
    ));
    assert_eq!(state.help_scroll(), 0);
}

#[test]
fn help_overlay_closes_on_parsed_q_event() {
    let mut state = test_state();
    assert!(matches!(
        handle_byte(&mut state, (80, 24), 0x10),
        InputAction::Handled
    ));

    assert!(matches!(
        handle_event(&mut state, (80, 24), &ParsedInput::Char('q')),
        InputAction::Handled
    ));
    assert!(!state.is_help_open());
}

#[test]
fn shift_arrow_starts_selection_and_moves_once() {
    let mut state = test_state();

    let action = handle_event(&mut state, (80, 24), &ParsedInput::ShiftArrow(b'C'));

    assert!(matches!(action, InputAction::Handled));
    let selection = state.selection_view().expect("selection started");
    assert_eq!(selection.anchor, dartboard_core::Pos { x: 0, y: 0 });
    assert_eq!(selection.cursor, dartboard_core::Pos { x: 1, y: 0 });
}

#[test]
fn shift_arrow_extends_existing_selection_anchor() {
    let mut state = test_state();

    assert!(matches!(
        handle_event(&mut state, (80, 24), &ParsedInput::ShiftArrow(b'C')),
        InputAction::Handled
    ));
    assert!(matches!(
        handle_event(&mut state, (80, 24), &ParsedInput::ShiftArrow(b'B')),
        InputAction::Handled
    ));

    let selection = state.selection_view().expect("selection extended");
    assert_eq!(selection.anchor, dartboard_core::Pos { x: 0, y: 0 });
    assert_eq!(selection.cursor, dartboard_core::Pos { x: 1, y: 1 });
}

#[test]
fn page_down_scrolls_half_screen_after_reaching_bottom_edge() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(80, 60);

    let first = handle_event(&mut state, (80, 24), &ParsedInput::PageDown);
    let second = handle_event(&mut state, (80, 24), &ParsedInput::PageDown);

    assert!(matches!(first, InputAction::Handled));
    assert!(matches!(second, InputAction::Handled));
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 0, y: 32 });
    assert_eq!(state.viewport_origin(), dartboard_core::Pos { x: 0, y: 11 });
}

#[test]
fn mouse_wheel_scroll_pans_viewport_via_shared_pointer_handler() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(80, 60);
    state.set_viewport_for_screen((80, 24));

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::ScrollDown,
            button: None,
            x: 10,
            y: 10,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(state.viewport_origin(), dartboard_core::Pos { x: 0, y: 1 });
}

#[test]
fn mouse_wheel_over_info_overlay_does_not_pan_canvas() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(80, 60);
    state.set_viewport_for_screen((80, 24));

    let action = handle_mouse(
        &mut state,
        (80, 24),
        &MouseEvent {
            kind: MouseEventKind::ScrollDown,
            button: None,
            x: 35,
            y: 3,
            modifiers: Default::default(),
        },
    );

    assert!(matches!(action, InputAction::Handled));
    assert_eq!(state.viewport_origin(), dartboard_core::Pos { x: 0, y: 0 });
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
        "painter".to_string(),
        shared_provenance,
    )
}
