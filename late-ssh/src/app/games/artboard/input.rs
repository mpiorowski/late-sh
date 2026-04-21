use dartboard_editor::{
    AppKey, AppKeyCode, AppModifiers, AppPointerButton, AppPointerEvent, AppPointerKind, HostEffect,
};

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};

use super::state::State;
use super::ui::{SwatchHit, swatch_hit};

pub enum InputAction {
    Ignored,
    Handled,
    Copy(String),
    Leave,
}

pub fn handle_byte(state: &mut State, screen_size: (u16, u16), byte: u8) -> InputAction {
    state.set_viewport_for_screen(screen_size);
    match byte {
        0x11 => InputAction::Leave, // Ctrl+Q
        0x1B => handle_app_key(
            state,
            AppKey {
                code: AppKeyCode::Esc,
                modifiers: AppModifiers::default(),
            },
        ),
        b'\r' => {
            if state.commit_floating() {
                return InputAction::Handled;
            }
            handle_app_key(
                state,
                AppKey {
                    code: AppKeyCode::Enter,
                    modifiers: AppModifiers::default(),
                },
            )
        }
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

pub(crate) fn handle_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> InputAction {
    state.set_viewport_for_screen(screen_size);
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
        ParsedInput::CtrlShiftArrow(_) => InputAction::Handled,
        ParsedInput::Mouse(mouse) => handle_mouse(state, screen_size, mouse),
        ParsedInput::Paste(bytes) => {
            state.paste_bytes(bytes, screen_size);
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
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
    if let Some(hit) = swatch_hit(screen_size, state, mouse.x, mouse.y) {
        state.clear_pending_canvas_click();
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
    use crate::app::games::artboard::svc::DartboardService;
    use dartboard_core::{Canvas, CellValue, RgbColor};
    use dartboard_editor::Clipboard;
    use dartboard_server::{InMemStore, ServerHandle};
    use uuid::Uuid;

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
            crate::app::games::artboard::state::BrushMode::Glyph('x')
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

    fn test_state() -> State {
        let server = ServerHandle::spawn_local(InMemStore);
        let svc = DartboardService::new(server, Uuid::now_v7(), "painter");
        let mut state = State::new(svc);
        state.snapshot.your_color = Some(RgbColor::new(255, 196, 64));
        state
    }
}
