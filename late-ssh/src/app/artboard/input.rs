use dartboard_editor::{
    AppKey, AppKeyCode, AppModifiers, AppPointerButton, AppPointerEvent, AppPointerKind, HostEffect,
};

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};

use super::state::State;
use super::ui::{SwatchHit, help_tab_hit, info_hit, swatch_hit};

pub enum InputAction {
    Ignored,
    Handled,
    Copy(String),
    Leave,
}

pub fn handle_byte(state: &mut State, screen_size: (u16, u16), byte: u8) -> InputAction {
    state.set_viewport_for_screen(screen_size);
    if state.is_glyph_picker_open() {
        return handle_picker_byte(state, screen_size, byte);
    }
    if byte == 0x1C {
        state.toggle_ownership_overlay();
        state.clear_pending_canvas_click();
        return InputAction::Handled;
    }
    if byte == 0x10 {
        state.toggle_help();
        state.clear_pending_canvas_click();
        return InputAction::Handled;
    }
    if state.is_help_open() {
        return handle_help_byte(state, byte);
    }
    match byte {
        // Ctrl+] / Ctrl+5 / raw GS — open the glyph picker.
        0x1D => {
            state.open_glyph_picker();
            InputAction::Handled
        }
        0x1B => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Esc,
                modifiers: AppModifiers::default(),
            },
        ),
        b'\r' => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Enter,
                modifiers: AppModifiers::default(),
            },
        ),
        0x7f => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Backspace,
                modifiers: AppModifiers::default(),
            },
        ),
        _ => {
            if let Some(key) = app_key_from_raw_control_byte(byte) {
                handle_app_key(state, key)
            } else if byte.is_ascii_graphic() || byte == b' ' {
                handle_app_key(
                    state,
                    AppKey {
                        code: AppKeyCode::Char(byte as char),
                        modifiers: AppModifiers::default(),
                    },
                )
            } else {
                InputAction::Ignored
            }
        }
    }
}

fn handle_help_byte(state: &mut State, byte: u8) -> InputAction {
    match byte {
        0x1B | b'q' | b'Q' | b'?' => state.close_help(),
        b'\t' => state.select_next_help_tab(),
        b'j' | b'J' => state.scroll_help(1),
        b'k' | b'K' => state.scroll_help(-1),
        _ => return InputAction::Ignored,
    }
    InputAction::Handled
}

fn handle_picker_byte(state: &mut State, screen_size: (u16, u16), byte: u8) -> InputAction {
    match byte {
        0x1B => {
            // Esc closes the picker without inserting.
            state.close_glyph_picker();
        }
        b'\r' => {
            state.glyph_picker_insert(false, screen_size);
        }
        b'\t' => state.glyph_picker_next_tab(),
        0x7f => state.glyph_picker_state_mut().search_delete_char(),
        0x01 => state.glyph_picker_state_mut().search_cursor_home(),
        0x05 => state.glyph_picker_state_mut().search_cursor_end(),
        0x19 => state.glyph_picker_state_mut().search_paste(),
        0x1F => state.glyph_picker_state_mut().search_undo(),
        // Ctrl+] / Ctrl+5 again while open closes it (toggle).
        0x1D => state.close_glyph_picker(),
        _ => {
            if byte.is_ascii_graphic() || byte == b' ' {
                state
                    .glyph_picker_state_mut()
                    .search_insert_char(byte as char);
            } else {
                return InputAction::Ignored;
            }
        }
    }
    InputAction::Handled
}

fn handle_picker_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => state.glyph_picker_move_selection(-1),
        b'B' => state.glyph_picker_move_selection(1),
        b'C' => state.glyph_picker_state_mut().search_cursor_right(),
        b'D' => state.glyph_picker_state_mut().search_cursor_left(),
        _ => return false,
    }
    true
}

fn handle_picker_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> InputAction {
    match event {
        ParsedInput::BackTab => state.glyph_picker_prev_tab(),
        ParsedInput::PageUp => {
            let page = state.glyph_picker_state().visible_height.get().max(1) as isize;
            state.glyph_picker_move_selection(-page);
        }
        ParsedInput::PageDown => {
            let page = state.glyph_picker_state().visible_height.get().max(1) as isize;
            state.glyph_picker_move_selection(page);
        }
        ParsedInput::Home => state.glyph_picker_state_mut().search_cursor_home(),
        ParsedInput::End => state.glyph_picker_state_mut().search_cursor_end(),
        ParsedInput::Delete => state.glyph_picker_state_mut().search_delete_next_char(),
        ParsedInput::CtrlDelete => state.glyph_picker_state_mut().search_delete_word_right(),
        ParsedInput::ShiftArrow(key) => match key {
            b'A' => {
                let half = (state.glyph_picker_state().visible_height.get() / 2).max(1) as isize;
                state.glyph_picker_move_selection(-half);
            }
            b'B' => {
                let half = (state.glyph_picker_state().visible_height.get() / 2).max(1) as isize;
                state.glyph_picker_move_selection(half);
            }
            _ => return InputAction::Ignored,
        },
        ParsedInput::CtrlArrow(key) => match key {
            b'A' => state.glyph_picker_move_selection(-1),
            b'B' => state.glyph_picker_move_selection(1),
            b'C' => state.glyph_picker_state_mut().search_cursor_word_right(),
            b'D' => state.glyph_picker_state_mut().search_cursor_word_left(),
            _ => return InputAction::Ignored,
        },
        ParsedInput::AltEnter => {
            state.glyph_picker_insert(true, screen_size);
        }
        ParsedInput::Mouse(mouse) => return handle_picker_mouse(state, screen_size, mouse),
        ParsedInput::Paste(bytes) => {
            if let Ok(text) = std::str::from_utf8(bytes) {
                for ch in text.chars() {
                    if !ch.is_control() {
                        state.glyph_picker_state_mut().search_insert_char(ch);
                    }
                }
            }
        }
        _ => return InputAction::Ignored,
    }
    InputAction::Handled
}

fn handle_picker_mouse(
    state: &mut State,
    screen_size: (u16, u16),
    mouse: &MouseEvent,
) -> InputAction {
    match mouse.kind {
        MouseEventKind::ScrollUp => state.glyph_picker_move_selection(-3),
        MouseEventKind::ScrollDown => state.glyph_picker_move_selection(3),
        MouseEventKind::Down if matches!(mouse.button, Some(MouseButton::Left)) => {
            // SGR coords are 1-based; glyph_picker hit-testing uses 0-based.
            let Some(col) = mouse.x.checked_sub(1) else {
                return InputAction::Handled;
            };
            let Some(row) = mouse.y.checked_sub(1) else {
                return InputAction::Handled;
            };
            if state.glyph_picker_click_tab(col, row) {
                return InputAction::Handled;
            }
            if state.glyph_picker_click_list(col, row) {
                state.glyph_picker_insert(true, screen_size);
            }
        }
        _ => {}
    }
    InputAction::Handled
}

fn app_key_from_raw_control_byte(byte: u8) -> Option<AppKey> {
    let ctrl = AppModifiers {
        ctrl: true,
        ..Default::default()
    };
    let code = match byte {
        0x00 => AppKeyCode::Char(' '),
        0x01..=0x1A => match byte {
            0x09 => AppKeyCode::Tab,
            0x0D => return None,
            _ => AppKeyCode::Char((b'a' + (byte - 1)) as char),
        },
        _ => return None,
    };
    Some(AppKey {
        code,
        modifiers: ctrl,
    })
}

pub fn handle_arrow(state: &mut State, screen_size: (u16, u16), key: u8) -> bool {
    state.set_viewport_for_screen(screen_size);
    if state.is_glyph_picker_open() {
        return handle_picker_arrow(state, key);
    }
    if state.is_help_open() {
        return handle_help_arrow(state, key);
    }
    let Some(code) = arrow_key_code(key) else {
        return false;
    };
    matches!(
        handle_app_key(
            state,
            AppKey {
                code,
                modifiers: AppModifiers::default(),
            },
        ),
        InputAction::Handled | InputAction::Copy(_)
    )
}

fn handle_help_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => state.scroll_help(-1),
        b'B' => state.scroll_help(1),
        _ => return false,
    }
    true
}

pub(crate) fn handle_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> InputAction {
    state.set_viewport_for_screen(screen_size);
    if state.is_glyph_picker_open() {
        return handle_picker_event(state, screen_size, event);
    }
    if state.is_help_open() {
        return handle_help_event(state, screen_size, event);
    }
    match event {
        ParsedInput::Home => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Home,
                modifiers: AppModifiers::default(),
            },
        ),
        ParsedInput::End => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::End,
                modifiers: AppModifiers::default(),
            },
        ),
        ParsedInput::PageUp => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::PageUp,
                modifiers: AppModifiers::default(),
            },
        ),
        ParsedInput::PageDown => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::PageDown,
                modifiers: AppModifiers::default(),
            },
        ),
        ParsedInput::AltC => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Char('c'),
                modifiers: AppModifiers {
                    alt: true,
                    ..Default::default()
                },
            },
        ),
        ParsedInput::Delete => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Delete,
                modifiers: AppModifiers::default(),
            },
        ),
        ParsedInput::ShiftArrow(key) => handle_app_key(
            state,
            AppKey {
                code: match arrow_key_code(*key) {
                    Some(code) => code,
                    None => return InputAction::Ignored,
                },
                modifiers: AppModifiers {
                    shift: true,
                    ..Default::default()
                },
            },
        ),
        ParsedInput::AltArrow(key) => {
            jump_to_edge(state, screen_size, *key);
            InputAction::Handled
        }
        ParsedInput::CtrlShiftArrow(key) => handle_app_key(
            state,
            AppKey {
                code: match arrow_key_code(*key) {
                    Some(code) => code,
                    None => return InputAction::Ignored,
                },
                modifiers: AppModifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            },
        ),
        ParsedInput::Mouse(mouse) => handle_mouse(state, screen_size, mouse),
        ParsedInput::Paste(bytes) => {
            state.paste_bytes(bytes, screen_size);
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}

fn handle_help_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> InputAction {
    match event {
        ParsedInput::BackTab => state.select_prev_help_tab(),
        ParsedInput::Home => state.reset_help_scroll(),
        ParsedInput::PageUp => state.scroll_help(-5),
        ParsedInput::PageDown => state.scroll_help(5),
        ParsedInput::Mouse(mouse) => return handle_help_mouse(state, screen_size, mouse),
        _ => return InputAction::Ignored,
    }
    InputAction::Handled
}

fn handle_help_mouse(
    state: &mut State,
    screen_size: (u16, u16),
    mouse: &MouseEvent,
) -> InputAction {
    if matches!(mouse.kind, MouseEventKind::Down)
        && matches!(mouse.button, Some(MouseButton::Left))
        && let Some(tab) = help_tab_hit(screen_size, state, mouse.x, mouse.y)
    {
        state.select_help_tab(tab);
    }
    state.clear_pending_canvas_click();
    InputAction::Handled
}

fn handle_app_key(state: &mut State, key: AppKey) -> InputAction {
    let dispatch = state.handle_app_key(key);
    if !dispatch.handled {
        return InputAction::Ignored;
    }

    if let Some(effect) = dispatch.effects.into_iter().next() {
        match effect {
            HostEffect::CopyToClipboard(text) => return InputAction::Copy(text),
            HostEffect::RequestQuit => return InputAction::Leave,
        }
    }

    InputAction::Handled
}

fn arrow_key_code(key: u8) -> Option<AppKeyCode> {
    Some(match key {
        b'A' => AppKeyCode::Up,
        b'B' => AppKeyCode::Down,
        b'C' => AppKeyCode::Right,
        b'D' => AppKeyCode::Left,
        _ => return None,
    })
}

fn jump_to_edge(state: &mut State, screen_size: (u16, u16), key: u8) {
    match key {
        b'A' => state.move_page_up(screen_size),
        b'B' => state.move_page_down(screen_size),
        b'C' => state.move_end(screen_size),
        b'D' => state.move_home(screen_size),
        _ => {}
    }
}

fn handle_mouse(state: &mut State, screen_size: (u16, u16), mouse: &MouseEvent) -> InputAction {
    state.set_hover_screen_point(screen_size, mouse.x, mouse.y);

    if let Some(hit) = swatch_hit(screen_size, state, mouse.x, mouse.y) {
        state.clear_pending_canvas_click();
        state.clear_hover();
        if matches!(mouse.kind, MouseEventKind::Down)
            && matches!(mouse.button, Some(MouseButton::Left))
        {
            match hit {
                SwatchHit::Body(idx) => {
                    if mouse.modifiers.ctrl {
                        state.clear_swatch(idx);
                    } else {
                        state.activate_swatch(idx);
                    }
                }
                SwatchHit::Pin(idx) => state.toggle_swatch_pin(idx),
            }
        }
        return InputAction::Handled;
    }

    if info_hit(screen_size, state, mouse.x, mouse.y) {
        state.clear_pending_canvas_click();
        state.clear_hover();
        return InputAction::Handled;
    }

    if matches!(mouse.kind, MouseEventKind::Down)
        && matches!(mouse.button, Some(MouseButton::Left))
        && !mouse.modifiers.shift
        && !mouse.modifiers.alt
        && !mouse.modifiers.ctrl
    {
        if let Some(pos) = state.canvas_pos_for_screen_point(screen_size, mouse.x, mouse.y) {
            if state.is_in_normal_brush_mode()
                && state.register_canvas_click(pos)
                && state.activate_temp_glyph_brush_at(pos)
            {
                return InputAction::Handled;
            }
        } else {
            state.clear_pending_canvas_click();
        }
    } else if matches!(mouse.kind, MouseEventKind::Down | MouseEventKind::Drag) {
        state.clear_pending_canvas_click();
    }

    if state.has_floating() {
        return handle_floating_mouse(state, screen_size, mouse);
    }

    handle_shared_pointer(state, mouse)
}

fn handle_floating_mouse(
    state: &mut State,
    _screen_size: (u16, u16),
    mouse: &MouseEvent,
) -> InputAction {
    handle_shared_pointer(state, mouse)
}

fn handle_shared_pointer(state: &mut State, mouse: &MouseEvent) -> InputAction {
    let Some(pointer) = app_pointer_event_from_mouse(mouse) else {
        return InputAction::Ignored;
    };
    let dispatch = state.handle_pointer_event(pointer);
    if dispatch.outcome.is_consumed() {
        InputAction::Handled
    } else {
        InputAction::Ignored
    }
}

fn app_pointer_event_from_mouse(mouse: &MouseEvent) -> Option<AppPointerEvent> {
    let column = mouse.x.checked_sub(1)?;
    let row = mouse.y.checked_sub(1)?;
    let kind = match mouse.kind {
        MouseEventKind::Moved => AppPointerKind::Moved,
        MouseEventKind::Down => AppPointerKind::Down(map_button(mouse.button?)?),
        MouseEventKind::Up => AppPointerKind::Up(map_button(mouse.button?)?),
        MouseEventKind::Drag => AppPointerKind::Drag(map_button(mouse.button?)?),
        MouseEventKind::ScrollUp => AppPointerKind::ScrollUp,
        MouseEventKind::ScrollDown => AppPointerKind::ScrollDown,
        MouseEventKind::ScrollLeft => AppPointerKind::ScrollLeft,
        MouseEventKind::ScrollRight => AppPointerKind::ScrollRight,
    };
    Some(AppPointerEvent {
        column,
        row,
        kind,
        modifiers: AppModifiers {
            shift: mouse.modifiers.shift,
            alt: mouse.modifiers.alt,
            ctrl: mouse.modifiers.ctrl,
            meta: false,
        },
    })
}

fn map_button(button: MouseButton) -> Option<AppPointerButton> {
    Some(match button {
        MouseButton::Left => AppPointerButton::Left,
        MouseButton::Middle => AppPointerButton::Middle,
        MouseButton::Right => AppPointerButton::Right,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::artboard::provenance::ArtboardProvenance;
    use crate::app::artboard::svc::{DartboardService, DartboardSnapshot};
    use dartboard_core::{Canvas, CellValue, RgbColor};
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
        assert_eq!(
            app_key_from_raw_control_byte(0x09),
            Some(AppKey {
                code: AppKeyCode::Tab,
                modifiers: AppModifiers {
                    ctrl: true,
                    ..Default::default()
                },
            })
        );
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
    fn swatch_overlay_pointer_events_do_not_reveal_hidden_preview() {
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
                x: 11,
                y: 17,
                modifiers: Default::default(),
            },
        );
        assert!(matches!(down, InputAction::Handled));
        assert!(state.floating_view().is_none());

        let up = handle_mouse(
            &mut state,
            (80, 24),
            &MouseEvent {
                kind: MouseEventKind::Up,
                button: Some(MouseButton::Left),
                x: 11,
                y: 17,
                modifiers: Default::default(),
            },
        );
        assert!(matches!(up, InputAction::Handled));
        assert!(state.floating_view().is_none());

        let moved_over_swatch = handle_mouse(
            &mut state,
            (80, 24),
            &MouseEvent {
                kind: MouseEventKind::Moved,
                button: None,
                x: 11,
                y: 17,
                modifiers: Default::default(),
            },
        );
        assert!(matches!(moved_over_swatch, InputAction::Handled));
        assert!(state.floating_view().is_none());

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
                x: 11,
                y: 17,
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
                x: 11,
                y: 17,
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
    fn raw_lf_maps_to_ctrl_j_instead_of_enter() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(3, 3);
        state
            .snapshot
            .canvas
            .set(dartboard_core::Pos { x: 0, y: 0 }, 'A');

        let action = handle_byte(&mut state, (80, 24), b'\n');

        assert!(matches!(action, InputAction::Handled));
        assert_eq!(
            state
                .snapshot
                .canvas
                .get(dartboard_core::Pos { x: 0, y: 1 }),
            'A'
        );
        assert_eq!(
            state
                .snapshot
                .canvas
                .get(dartboard_core::Pos { x: 0, y: 0 }),
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
                x: 30,
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
            your_color: Some(RgbColor::new(255, 196, 64)),
            ..Default::default()
        };
        let svc = DartboardService::disconnected_for_tests(snapshot);
        State::new(svc, "painter".to_string(), shared_provenance)
    }
}
