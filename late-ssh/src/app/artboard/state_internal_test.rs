use super::*;
use crate::app::artboard::provenance::ArtboardProvenance;
use crate::app::artboard::svc::{ArtboardSnapshotService, DartboardService, DartboardSnapshot};
use dartboard_core::{CanvasOp, CellValue, RgbColor};
use dartboard_editor::Clipboard;

fn test_state() -> State {
    let shared_provenance = ArtboardProvenance::default().shared();
    let snapshot = DartboardSnapshot {
        provenance: ArtboardProvenance::default(),
        your_name: "painter".to_string(),
        your_user_id: Some(1),
        your_color: Some(PAINT_PALETTE[1]),
        ..Default::default()
    };
    let svc = DartboardService::disconnected_for_tests(snapshot);
    let mut state = State::new(
        svc,
        ArtboardSnapshotService::disabled(),
        "painter".to_string(),
        shared_provenance,
    );
    state.set_viewport_for_screen((80, 24));
    state
}

#[test]
fn screen_point_conversion_uses_sgr_one_based_coords() {
    let viewport = Rect::new(1, 1, 50, 22);
    let pos = canvas_pos_for_screen_point(viewport, Pos { x: 0, y: 0 }, 120, 60, 2, 2);
    assert_eq!(pos, Some(Pos { x: 0, y: 0 }));
}

#[test]
fn screen_point_conversion_respects_viewport_origin() {
    let viewport = Rect::new(1, 1, 50, 22);
    let pos = canvas_pos_for_screen_point(viewport, Pos { x: 10, y: 5 }, 120, 60, 12, 8);
    assert_eq!(pos, Some(Pos { x: 20, y: 11 }));
}

#[test]
fn screen_point_conversion_rejects_points_outside_canvas() {
    let viewport = Rect::new(1, 1, 50, 22);
    assert_eq!(
        canvas_pos_for_screen_point(viewport, Pos { x: 0, y: 0 }, 4, 4, 10, 10),
        None
    );
}

#[test]
fn owner_initial_skips_prefix_punctuation_and_defaults_when_missing() {
    assert_eq!(owner_initial("__mat"), 'M');
    assert_eq!(owner_initial("!!!"), '?');
}

#[test]
fn paste_cursor_end_handles_crlf_controls_and_bounds() {
    assert_eq!(
        paste_cursor_end(Pos { x: 2, y: 0 }, "A\r\nB\u{7}C", 4, 2),
        Pos { x: 3, y: 1 }
    );
    assert_eq!(
        paste_cursor_end(Pos { x: 3, y: 1 }, "ZZ", 4, 2),
        Pos { x: 3, y: 1 }
    );
}

#[test]
fn type_char_advances_cursor_right() {
    let mut state = test_state();
    state.type_char('A', (80, 24));
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'A');
    assert_eq!(state.cursor(), Pos { x: 1, y: 0 });
}

#[test]
fn paint_color_cycles_and_typed_glyphs_use_selection() {
    let mut state = test_state();
    assert_eq!(state.active_paint_color_index(), 1);

    state.cycle_paint_color(1);
    assert_eq!(state.active_paint_color_index(), 2);
    assert_eq!(state.active_paint_color(), PAINT_PALETTE[2]);

    state.type_char('C', (80, 24));
    assert_eq!(
        state.snapshot.canvas.fg(Pos { x: 0, y: 0 }),
        Some(PAINT_PALETTE[2])
    );
}

#[test]
fn paint_color_cycle_wraps() {
    let mut state = test_state();
    state.cycle_paint_color(-2);
    assert_eq!(state.active_paint_color_index(), PAINT_PALETTE.len() - 1);
    assert_eq!(
        state.active_paint_color(),
        PAINT_PALETTE[PAINT_PALETTE.len() - 1]
    );
}

#[test]
fn paste_bytes_lays_out_multiline_text_with_wrap() {
    let mut state = test_state();

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

#[test]
fn drag_brush_requires_temp_brush_and_paints_without_advancing() {
    let mut state = test_state();
    state.paint_char('B');
    assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));
    state.begin_drag_brush_from_cursor();
    state.move_right((80, 24));
    assert!(state.paint_drag_brush());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), 'B');
    assert_eq!(state.cursor(), Pos { x: 1, y: 0 });
    state.clear_drag_brush();
    state.move_right((80, 24));
    assert!(!state.paint_drag_brush());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 0 }), ' ');
}

#[test]
fn drag_brush_no_longer_samples_canvas_without_temp_brush() {
    let mut state = test_state();
    state.paint_char('Z');
    state.begin_drag_brush_from_cursor();
    state.move_right((80, 24));
    assert!(!state.paint_drag_brush());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), ' ');
}

#[test]
fn escape_clears_active_and_drag_brushes() {
    let mut state = test_state();
    state.type_char('Q', (80, 24));
    assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));
    state.begin_drag_brush_from_cursor();
    state.begin_selection_from_cursor();
    state.clear_local_state();
    assert_eq!(state.active_brush(), None);
    state.move_right((80, 24));
    assert!(!state.paint_drag_brush());
    assert!(state.selection_view().is_none());
}

#[test]
fn selection_tracks_anchor_and_drag_cursor() {
    let mut state = test_state();
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    state.move_down((80, 24));
    assert!(state.update_selection_to_cursor());
    let selection = state.selection_view().expect("selection should exist");
    assert_eq!(selection.anchor, Pos { x: 0, y: 0 });
    assert_eq!(selection.cursor, Pos { x: 1, y: 1 });
    assert!(matches!(selection.shape, TuiSelectionShape::Rect));
}

#[test]
fn app_key_char_fills_active_selection_via_shared_executor() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(3, 2);
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    state.move_down((80, 24));

    let dispatch = state.handle_app_key(AppKey {
        code: dartboard_editor::AppKeyCode::Char('x'),
        modifiers: Default::default(),
    });

    assert!(dispatch.handled);
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'x');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 1 }), 'x');
    assert_eq!(state.brush_mode(), BrushMode::None);
}

#[test]
fn app_key_alt_c_returns_copy_effect() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(2, 1);
    state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');

    let dispatch = state.handle_app_key(AppKey {
        code: dartboard_editor::AppKeyCode::Char('c'),
        modifiers: dartboard_editor::AppModifiers {
            alt: true,
            ..Default::default()
        },
    });

    assert_eq!(
        dispatch.effects,
        vec![dartboard_editor::HostEffect::CopyToClipboard(
            "A ".to_string()
        )]
    );
}

#[test]
fn app_key_ctrl_c_copies_into_primary_swatch_and_arms_it() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(2, 1);
    state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');

    let dispatch = state.handle_app_key(AppKey {
        code: dartboard_editor::AppKeyCode::Char('c'),
        modifiers: dartboard_editor::AppModifiers {
            ctrl: true,
            ..Default::default()
        },
    });

    assert!(dispatch.handled);
    assert_eq!(state.active_swatch_index(), Some(0));
    assert!(state.has_floating());
    assert!(state.floating_is_transparent());
    assert_eq!(
        state.editor.swatches[0]
            .as_ref()
            .and_then(|swatch| swatch.clipboard.get(0, 0)),
        Some(CellValue::Narrow('A'))
    );
}

#[test]
fn app_key_space_dismisses_temp_brush_back_to_none() {
    let mut state = test_state();
    state.type_char('Q', (80, 24));
    assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));

    let dispatch = state.handle_app_key(AppKey {
        code: dartboard_editor::AppKeyCode::Char(' '),
        modifiers: Default::default(),
    });

    assert!(dispatch.handled);
    assert!(!state.has_floating());
    assert_eq!(state.brush_mode(), BrushMode::None);
}

#[test]
fn app_key_escape_without_selection_or_brush_falls_through() {
    let mut state = test_state();

    let dispatch = state.handle_app_key(AppKey {
        code: dartboard_editor::AppKeyCode::Esc,
        modifiers: Default::default(),
    });

    assert!(!dispatch.handled);
}

#[test]
fn swatch_brush_mode_reports_swatch() {
    let mut state = test_state();
    state.editor.swatches[0] = Some(Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });

    state.activate_swatch(0);

    assert_eq!(state.brush_mode(), BrushMode::Swatch);
    assert!(state.floating_is_transparent());
}

#[test]
fn temp_glyph_brush_mode_reports_canvas_glyph() {
    let mut state = test_state();
    state.type_char('🔥', (80, 24));

    assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));

    assert_eq!(state.brush_mode(), BrushMode::Glyph('🔥'));
    assert!(state.has_floating());
    assert!(state.floating_is_transparent());
}

#[test]
fn register_canvas_click_treats_wide_glyph_halves_as_one_target() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 1);
    let _ = state.snapshot.canvas.put_glyph(Pos { x: 0, y: 0 }, '👍');

    assert!(!state.register_canvas_click(Pos { x: 0, y: 0 }));
    assert!(state.register_canvas_click(Pos { x: 1, y: 0 }));
}

#[test]
fn temp_glyph_brush_from_wide_continuation_captures_full_glyph() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 1);
    let _ = state.snapshot.canvas.put_glyph(Pos { x: 0, y: 0 }, '👍');

    assert!(state.activate_temp_glyph_brush_at(Pos { x: 1, y: 0 }));

    assert_eq!(state.cursor(), Pos { x: 0, y: 0 });
    assert_eq!(state.brush_mode(), BrushMode::Glyph('👍'));
    let floating = state
        .floating_view()
        .expect("temp brush floating preview shown");
    assert_eq!(floating.anchor, Pos { x: 0, y: 0 });
    assert_eq!(floating.width, 2);
    assert_eq!(floating.height, 1);
    assert!(state.floating_is_transparent());
}

#[test]
fn app_key_ctrl_v_stamps_floating_like_reference_client() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(5, 3);
    state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'A');
    state.editor.cursor = Pos { x: 1, y: 1 };
    state.begin_selection_from_cursor();
    assert!(state.lift_selection_to_floating());
    state.editor.cursor = Pos { x: 3, y: 0 };

    let dispatch = state.handle_app_key(AppKey {
        code: dartboard_editor::AppKeyCode::Char('v'),
        modifiers: dartboard_editor::AppModifiers {
            ctrl: true,
            ..Default::default()
        },
    });

    assert!(dispatch.handled);
    assert_eq!(state.snapshot.canvas.get(Pos { x: 3, y: 0 }), 'A');
    assert!(state.has_floating());
}

#[test]
fn swatch_preview_tracks_pointer_after_canvas_reentry() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(40, 20);
    state.editor.swatches[0] = Some(Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });
    state.editor.cursor = Pos { x: 12, y: 7 };

    state.activate_swatch(0);

    assert!(state.has_floating());
    assert!(state.floating_view().is_some());

    let dispatch = state.handle_pointer_event(AppPointerEvent {
        column: 4,
        row: 3,
        kind: dartboard_editor::AppPointerKind::Moved,
        modifiers: Default::default(),
    });

    assert!(dispatch.outcome.is_consumed());
    let floating = state.floating_view().expect("floating preview shown");
    assert_eq!(floating.anchor, Pos { x: 3, y: 2 });
}

#[test]
fn swatch_preview_suppression_hides_canvas_cursor() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(40, 20);
    state.editor.swatches[0] = Some(Swatch {
        clipboard: Clipboard::new(3, 3, vec![Some(CellValue::Narrow('A')); 9]),
        pinned: false,
    });

    state.activate_swatch(0);

    assert!(state.has_floating());
    assert!(state.should_show_canvas_cursor());
}

#[test]
fn primary_swatch_pin_toggle_is_ignored() {
    let mut state = test_state();
    state.editor.swatches[0] = Some(Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });

    state.toggle_swatch_pin(0);

    assert_eq!(
        state.swatches()[0].as_ref().map(|swatch| swatch.pinned),
        Some(false)
    );
}

#[test]
fn system_clipboard_export_uses_selection_when_present() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(3, 2);
    state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');
    state.snapshot.canvas.set(Pos { x: 1, y: 0 }, 'B');
    state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'D');
    state.editor.cursor = Pos { x: 1, y: 0 };
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    state.move_down((80, 24));

    assert_eq!(state.export_system_clipboard_text(), "B \nD ");
}

#[test]
fn system_clipboard_export_uses_full_canvas_without_selection() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(3, 2);
    state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');
    state.snapshot.canvas.set(Pos { x: 1, y: 0 }, 'B');
    state.snapshot.canvas.set(Pos { x: 0, y: 1 }, 'C');
    state.snapshot.canvas.set(Pos { x: 2, y: 1 }, 'D');

    assert_eq!(state.export_system_clipboard_text(), "AB \nC D");
}

#[test]
fn dismissing_floating_restores_original_selection() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 2);
    state.editor.cursor = Pos { x: 1, y: 0 };
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    assert!(state.lift_selection_to_floating());
    state.editor.cursor = Pos { x: 0, y: 1 };

    assert!(state.dismiss_floating());

    let selection = state.selection_view().expect("selection restored");
    assert_eq!(selection.anchor, Pos { x: 1, y: 0 });
    assert_eq!(selection.cursor, Pos { x: 2, y: 0 });
    assert_eq!(state.cursor(), Pos { x: 2, y: 0 });
}

#[test]
fn pointer_dismiss_floating_restores_original_selection() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 2);
    state.editor.cursor = Pos { x: 1, y: 0 };
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    assert!(state.lift_selection_to_floating());
    state.editor.cursor = Pos { x: 0, y: 1 };

    let dispatch = state.handle_pointer_event(AppPointerEvent {
        column: 1,
        row: 2,
        kind: dartboard_editor::AppPointerKind::Down(dartboard_editor::AppPointerButton::Right),
        modifiers: Default::default(),
    });

    assert!(dispatch.outcome.is_consumed());
    assert_eq!(
        dispatch.stroke_hint,
        Some(dartboard_editor::PointerStrokeHint::End)
    );
    assert!(!state.has_floating());
    let selection = state.selection_view().expect("selection restored");
    assert_eq!(selection.anchor, Pos { x: 1, y: 0 });
    assert_eq!(selection.cursor, Pos { x: 2, y: 0 });
    assert_eq!(state.cursor(), Pos { x: 2, y: 0 });
}

#[test]
fn glyph_picker_opens_closes_and_inserts_selected_glyph() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(10, 3);
    state.editor.cursor = Pos { x: 0, y: 0 };

    state.open_glyph_picker();
    assert!(state.is_glyph_picker_open());
    assert!(state.glyph_catalog().is_some());

    // First selectable entry on the emoji tab is the first COMMON_EMOJI
    // ("👍" thumbs up). Confirm insertion paints it at the cursor and
    // closes the picker.
    assert!(state.glyph_picker_insert(false, (80, 24)));
    assert!(!state.is_glyph_picker_open());
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), '👍');
}

#[test]
fn glyph_picker_inserts_full_kaomoji_string() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(20, 3);
    state.editor.cursor = Pos { x: 2, y: 1 };
    state.open_glyph_picker();
    state
        .glyph_picker_state_mut()
        .set_tab(icon_picker::IconPickerTab::Kaomoji);
    for ch in "happy smile".chars() {
        state.glyph_picker_state_mut().search_insert_char(ch);
    }

    assert!(state.glyph_picker_insert(false, (80, 24)));
    assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 1 }), '(');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 3, y: 1 }), '*');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 7, y: 1 }), 'ω');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 10, y: 1 }), ')');
    assert_eq!(state.cursor(), Pos { x: 11, y: 1 });
}

#[test]
fn glyph_picker_keep_open_leaves_picker_visible_after_insert() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(10, 3);
    state.editor.cursor = Pos { x: 0, y: 0 };
    state.open_glyph_picker();
    assert!(state.glyph_picker_insert(true, (80, 24)));
    assert!(state.is_glyph_picker_open());
}

#[test]
fn glyph_picker_open_dismisses_floating_and_selection() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(4, 2);
    state.editor.cursor = Pos { x: 0, y: 0 };
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    assert!(state.lift_selection_to_floating());
    assert!(state.has_floating());

    state.open_glyph_picker();

    assert!(state.is_glyph_picker_open());
    assert!(!state.has_floating());
    assert!(state.selection_view().is_none());
}

#[test]
fn edit_canvas_detects_real_canvas_changes_even_if_helper_reports_false() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(5, 3);

    let changed = state.edit_canvas(|_editor, canvas, color| {
        let _ = canvas.put_glyph_colored(Pos { x: 0, y: 0 }, '👍', color);
        false
    });

    assert!(changed);
    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), '👍');
}

#[test]
fn diff_canvas_op_wide_insert_left_of_filled_cell_replays_cleanly() {
    let mut before = Canvas::with_size(5, 1);
    before.set_colored(Pos { x: 1, y: 0 }, 'A', RgbColor::new(1, 2, 3));

    let mut after = before.clone();
    let _ = after.put_glyph_colored(Pos { x: 0, y: 0 }, '👍', RgbColor::new(4, 5, 6));

    let op = diff_canvas_op(&before, &after, RgbColor::new(4, 5, 6)).expect("wide insert op");
    let mut replay = before.clone();
    replay.apply(&op);

    assert_eq!(
        op,
        CanvasOp::PaintCell {
            pos: Pos { x: 0, y: 0 },
            ch: '👍',
            fg: RgbColor::new(4, 5, 6),
        }
    );
    assert_eq!(replay, after);
    assert_eq!(replay.get(Pos { x: 0, y: 0 }), '👍');
    assert_eq!(replay.cell(Pos { x: 1, y: 0 }), Some(CellValue::WideCont));
}

#[test]
fn commit_floating_moves_selected_region() {
    let mut state = test_state();
    state.snapshot.canvas = Canvas::with_size(5, 3);
    state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'A');
    state.snapshot.canvas.set(Pos { x: 2, y: 1 }, 'B');
    state.editor.cursor = Pos { x: 1, y: 1 };
    state.begin_selection_from_cursor();
    state.move_right((80, 24));
    assert!(state.lift_selection_to_floating());

    state.editor.cursor = Pos { x: 0, y: 0 };
    assert!(state.commit_floating());

    assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'A');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), 'B');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 1 }), ' ');
    assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 1 }), ' ');
    assert!(!state.has_floating());
}
