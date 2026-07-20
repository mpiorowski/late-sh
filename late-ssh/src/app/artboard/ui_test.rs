use super::*;
use crate::app::artboard::provenance::ArtboardProvenance;
use crate::app::artboard::state::State;
use dartboard_core::{CellValue, RgbColor};
use dartboard_editor::Clipboard;
use ratatui::buffer::Buffer;

use super::super::svc::{ArtboardSnapshotService, DartboardService, DartboardSnapshot};

#[test]
fn canvas_area_matches_artboard_frame_layout() {
    assert_eq!(canvas_area_for_screen((80, 24)), Rect::new(1, 1, 54, 22));
}

#[test]
fn info_box_overlays_top_right_of_full_canvas_width() {
    let state = test_state();
    assert_eq!(
        artboard_info_area_for_screen((80, 24), &state),
        Some(Rect::new(34, 1, 21, 11))
    );
}

#[test]
fn help_lines_cover_all_tabs_with_title_headings() {
    for tab in HelpTab::ALL {
        let lines = lines_for(tab);
        assert!(!lines.is_empty(), "{:?} should have content", tab);
        assert!(!lines[0].is_empty(), "{:?} should lead with a heading", tab);
    }
    let drawing = lines_for(HelpTab::Drawing).join("\n");
    assert!(drawing.contains("move cursor"));
    assert!(drawing.contains("Shift+arrows"));
}

#[test]
fn clipboard_preview_offset_skips_leading_blank_rows_and_columns() {
    let clipboard = Clipboard::new(
        4,
        3,
        vec![
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow('A')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
            Some(CellValue::Narrow(' ')),
        ],
    );

    assert_eq!(clipboard_preview_offset(&clipboard), (2, 1));
}

#[test]
fn help_tab_hit_uses_overlay_tab_rects() {
    let mut state = test_state();
    state.toggle_help();
    let area = artboard_game_area_for_screen((80, 24));
    let popup = help_popup_area(area);
    let layout = help_layout(popup).expect("help layout");
    let drawing = tab_rects(layout[1])
        .into_iter()
        .find(|(tab, _)| *tab == HelpTab::Drawing)
        .expect("drawing tab hit rect");
    let rect = drawing.1;

    assert_eq!(
        help_tab_hit((80, 24), &state, rect.x + 1, rect.y + 1),
        Some(HelpTab::Drawing)
    );
}

#[test]
fn help_scroll_is_preserved_per_tab() {
    let mut state = test_state();
    state.toggle_help();
    state.scroll_help(3);
    assert_eq!(state.help_scroll(), 3);
    state.select_next_help_tab();
    assert_eq!(state.help_scroll(), 0);
    state.scroll_help(7);
    assert_eq!(state.help_scroll(), 7);
    state.select_prev_help_tab();
    assert_eq!(state.help_scroll(), 3);
}

#[test]
fn info_block_height_tracks_visible_lines() {
    assert_eq!(info_block_height(0), 3);
    assert_eq!(info_block_height(1), 3);
    assert_eq!(info_block_height(2), 4);
}

#[test]
fn info_lines_include_compact_rows_before_users() {
    let state = test_state();
    let lines = artboard_info_lines(&state, false);

    assert_eq!(lines[0].to_string(), "Mode       view");
    assert_eq!(lines[1].to_string(), "Color      #FFEC60");
    assert_eq!(lines[2].to_string().chars().count(), 19);
    assert_eq!(lines[3].to_string().chars().count(), 19);
    assert!(lines[2].to_string().starts_with("Palette"));
    assert_eq!(lines[2].to_string().matches('•').count(), 1);
    assert_eq!(lines[3].to_string().matches('•').count(), 0);
    assert_eq!(lines[4].to_string(), "Cursor     0,0");
    assert_eq!(lines[5].to_string(), "Mouse      0,0");
    assert_eq!(lines[6].to_string(), "Owner      ?");
    assert_eq!(lines[7].to_string(), "Users");
    assert_eq!(lines[8].to_string(), "• painter (you)");
}

#[test]
fn info_lines_show_selection_dimensions() {
    let mut state = test_state();
    state.begin_selection_from_cursor();
    let lines = artboard_info_lines(&state, true);
    assert_eq!(lines[0].to_string(), "Mode       active");
    assert_eq!(lines[4].to_string(), "Cursor     1x1");

    state.move_right((80, 24));
    state.move_right((80, 24));
    state.move_down((80, 24));
    assert!(state.update_selection_to_cursor());

    let lines = artboard_info_lines(&state, true);
    assert_eq!(lines[0].to_string(), "Mode       active");
    assert_eq!(lines[4].to_string(), "Cursor     3x2");
}

#[test]
fn info_mode_reports_active_brush_kind() {
    let mut state = test_state();
    state.type_char('x', (80, 24));
    assert!(state.activate_temp_glyph_brush_at(dartboard_core::Pos { x: 0, y: 0 }));
    assert_eq!(
        artboard_info_lines(&state, true)[0].to_string(),
        "Mode       brush x"
    );

    let mut state = test_state();
    state.editor.swatches[0] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });
    state.activate_swatch(0);
    assert_eq!(
        artboard_info_lines(&state, true)[0].to_string(),
        "Mode       swatch"
    );
}

#[test]
fn render_cursor_pos_uses_wide_origin_for_continuation() {
    let mut state = test_state();
    let _ = state
        .snapshot
        .canvas
        .put_glyph(dartboard_core::Pos { x: 0, y: 0 }, '👍');
    state.editor.cursor = dartboard_core::Pos { x: 1, y: 0 };

    assert_eq!(
        canvas_cursor_render_pos(&state),
        dartboard_core::Pos { x: 0, y: 0 }
    );
    assert_eq!(state.cursor(), dartboard_core::Pos { x: 1, y: 0 });
}

#[test]
fn native_canvas_cursor_is_hidden_in_view_mode() {
    let state = test_state();

    assert!(!should_show_native_canvas_cursor(&state, false));
    assert!(should_show_native_canvas_cursor(&state, true));
}

#[test]
fn swatch_boxes_use_full_artboard_width_below_short_info_block() {
    let state = test_state();
    let rects = swatch_box_rects((80, 26), &state);
    assert_eq!(rects[0], Some(Rect::new(19, 20, 8, 4)));
    assert_eq!(rects[1], Some(Rect::new(26, 20, 8, 4)));
    assert_eq!(rects[2], Some(Rect::new(33, 20, 8, 4)));
    assert_eq!(rects[3], Some(Rect::new(40, 20, 8, 4)));
    assert_eq!(rects[4], Some(Rect::new(47, 20, 8, 4)));
}

#[test]
fn swatch_boxes_fall_back_to_canvas_edge_when_info_block_reaches_them() {
    let mut state = test_state();
    state.snapshot.peers = (0..10)
        .map(|idx| dartboard_core::Peer {
            user_id: idx as u64,
            name: format!("user{idx}"),
            color: RgbColor::new(120, 120, 120),
        })
        .collect();
    let rects = swatch_box_rects((80, 24), &state);
    assert_eq!(rects[0], Some(Rect::new(5, 18, 8, 4)));
    assert_eq!(rects[1], Some(Rect::new(12, 18, 8, 4)));
    assert_eq!(rects[2], Some(Rect::new(19, 18, 8, 4)));
}

#[test]
fn swatch_boxes_raise_above_notice_row() {
    let mut state = test_state();
    state.private_notice = Some("Heads up".to_string());
    let rects = swatch_box_rects((80, 24), &state);
    assert_eq!(rects[0], Some(Rect::new(19, 17, 8, 4)));
}

#[test]
fn swatch_boxes_leave_bottom_canvas_row_visible() {
    let state = test_state();
    let rects = swatch_box_rects((80, 24), &state);
    let canvas = canvas_area_for_screen((80, 24));

    assert!(
        !rects
            .iter()
            .flatten()
            .any(|rect| rect_contains(*rect, 10, canvas.bottom() - 1))
    );
}

#[test]
fn swatch_hit_uses_sgr_coordinates_and_prefers_pin() {
    let mut state = test_state();
    state.editor.swatches[0] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });
    state.editor.swatches[1] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('B'))]),
        pinned: false,
    });
    let screen_size = (80, 26);
    let rects = swatch_box_rects(screen_size, &state);
    let first = rects[0].expect("first swatch visible");
    let second = rects[1].expect("second swatch visible");
    let first_body = swatch_body_rect(first);
    let second_pin = swatch_pin_rect(second);

    assert_eq!(
        swatch_hit(screen_size, &state, first_body.x + 1, first_body.y + 1),
        Some(SwatchHit::Body(0))
    );
    assert_eq!(
        swatch_hit(
            screen_size,
            &state,
            first_body.right(),
            first_body.bottom().saturating_sub(1),
        ),
        Some(SwatchHit::Body(0))
    );
    assert_eq!(
        swatch_hit(screen_size, &state, second_pin.x + 1, second_pin.y + 1),
        Some(SwatchHit::Pin(1))
    );
}

#[test]
fn active_swatch_brightens_both_shared_dividers() {
    let mut state = test_state();
    for swatch in state.editor.swatches.iter_mut().take(3) {
        *swatch = Some(dartboard_editor::Swatch {
            clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
            pinned: false,
        });
    }
    state.activate_swatch(1);

    let rects = swatch_box_rects((120, 24), &state);
    let area = Rect::new(0, 0, 120, 24);
    let mut buf = Buffer::empty(area);
    render_swatch_strip_frame(&mut buf, &rects, &state, state.active_swatch_index());

    let middle = rects[1].expect("middle swatch visible");
    let right = rects[2].expect("right swatch visible");
    let divider_y = middle.y + 1;
    let top_y = middle.y;

    assert_eq!(buf[(middle.x, divider_y)].fg, theme::BORDER_ACTIVE());
    assert_eq!(buf[(right.x, divider_y)].fg, theme::BORDER_ACTIVE());
    assert_eq!(buf[(middle.x, top_y)].symbol(), "┬");
    assert_eq!(buf[(right.x, top_y)].symbol(), "┬");
}

#[test]
fn filled_swatch_divider_beats_empty_neighbor() {
    let mut state = test_state();
    state.editor.swatches[0] = Some(dartboard_editor::Swatch {
        clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
        pinned: false,
    });

    let rects = swatch_box_rects((120, 24), &state);
    let area = Rect::new(0, 0, 120, 24);
    let mut buf = Buffer::empty(area);
    render_swatch_strip_frame(&mut buf, &rects, &state, state.active_swatch_index());

    let divider_x = rects[1].expect("second swatch visible").x;
    let divider_y = rects[1].expect("second swatch visible").y + 1;

    assert_eq!(buf[(divider_x, divider_y)].fg, theme::AMBER_DIM());
}

#[test]
fn divider_priority_is_selected_then_filled_then_empty() {
    let mut state = test_state();
    for swatch in state.editor.swatches.iter_mut().take(2) {
        *swatch = Some(dartboard_editor::Swatch {
            clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
            pinned: false,
        });
    }
    state.activate_swatch(0);

    let rects = swatch_box_rects((160, 24), &state);
    let area = Rect::new(0, 0, 160, 24);
    let mut buf = Buffer::empty(area);
    render_swatch_strip_frame(&mut buf, &rects, &state, state.active_swatch_index());

    let divider_12_x = rects[1].expect("second swatch visible").x;
    let divider_23_x = rects[2].expect("third swatch visible").x;
    let divider_34_x = rects[3].expect("fourth swatch visible").x;
    let _divider_45_x = rects[4].expect("fifth swatch visible").x;
    let divider_y = rects[1].expect("second swatch visible").y + 1;

    assert_eq!(buf[(divider_12_x, divider_y)].fg, theme::BORDER_ACTIVE());
    assert_eq!(buf[(divider_23_x, divider_y)].fg, theme::AMBER_DIM());
    assert_eq!(buf[(divider_34_x, divider_y)].fg, theme::BORDER_DIM());
}

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
    State::new(
        svc,
        ArtboardSnapshotService::disabled(),
        "painter".to_string(),
        shared_provenance,
    )
}
