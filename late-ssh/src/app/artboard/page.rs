use crate::app::{
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

const VIEW_MODE_ALT_PAN_STEP: isize = 4;

pub(crate) fn handle_key(app: &mut App, byte: u8) -> bool {
    let size = app.size;
    let is_interacting = app.artboard_interacting;
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    if state.is_help_open() || state.is_glyph_picker_open() {
        let action = super::input::handle_byte(state, size, byte);
        return handle_action(app, action);
    }

    if is_interacting {
        let action = super::input::handle_byte(state, size, byte);
        return handle_action(app, action);
    }

    match byte {
        0x1C => {
            state.toggle_ownership_overlay();
            true
        }
        b'i' | b'I' | b'\r' | b'\n' => {
            app.activate_artboard_interaction();
            true
        }
        0x10 => {
            let action = super::input::handle_byte(state, size, byte);
            handle_action(app, action)
        }
        _ => false,
    }
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    let size = app.size;
    let is_interacting = app.artboard_interacting;
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    if is_interacting || state.is_help_open() || state.is_glyph_picker_open() {
        return super::input::handle_arrow(state, size, key);
    }

    match key {
        b'A' => {
            state.move_up(size);
            true
        }
        b'B' => {
            state.move_down(size);
            true
        }
        b'C' => {
            state.move_right(size);
            true
        }
        b'D' => {
            state.move_left(size);
            true
        }
        _ => false,
    }
}

pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let size = app.size;
    let is_interacting = app.artboard_interacting;
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    if is_interacting || state.is_help_open() || state.is_glyph_picker_open() {
        let action = super::input::handle_event(state, size, event);
        return handle_action(app, action);
    }

    match event {
        ParsedInput::PageUp => {
            state.move_page_up(size);
            true
        }
        ParsedInput::PageDown => {
            state.move_page_down(size);
            true
        }
        ParsedInput::Home => {
            state.move_home(size);
            true
        }
        ParsedInput::End => {
            state.move_end(size);
            true
        }
        ParsedInput::AltArrow(key) => match key {
            b'A' => {
                state.pan_viewport_by(size, 0, -VIEW_MODE_ALT_PAN_STEP);
                true
            }
            b'B' => {
                state.pan_viewport_by(size, 0, VIEW_MODE_ALT_PAN_STEP);
                true
            }
            b'C' => {
                state.pan_viewport_by(size, VIEW_MODE_ALT_PAN_STEP, 0);
                true
            }
            b'D' => {
                state.pan_viewport_by(size, -VIEW_MODE_ALT_PAN_STEP, 0);
                true
            }
            _ => false,
        },
        ParsedInput::Mouse(mouse)
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            ) =>
        {
            let action = super::input::handle_event(state, size, event);
            handle_action(app, action)
        }
        ParsedInput::Mouse(mouse) => handle_view_mode_mouse(state, size, mouse),
        _ => false,
    }
}

fn handle_view_mode_mouse(
    state: &mut super::state::State,
    size: (u16, u16),
    mouse: &MouseEvent,
) -> bool {
    state.set_hover_screen_point(size, mouse.x, mouse.y);
    if matches!(
        mouse.kind,
        MouseEventKind::Down | MouseEventKind::Drag | MouseEventKind::Up
    ) && matches!(mouse.button, Some(MouseButton::Right))
    {
        let action = super::input::handle_event(state, size, &ParsedInput::Mouse(*mouse));
        return matches!(
            action,
            super::input::InputAction::Handled | super::input::InputAction::Copy(_)
        );
    }

    false
}

fn handle_action(app: &mut App, action: super::input::InputAction) -> bool {
    match action {
        super::input::InputAction::Ignored => false,
        super::input::InputAction::Handled => true,
        super::input::InputAction::Copy(text) => {
            app.pending_clipboard = Some(text);
            true
        }
        super::input::InputAction::Leave => {
            app.deactivate_artboard_interaction();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use dartboard_core::{Canvas, Pos, RgbColor};

    use super::*;
    use crate::app::artboard::{
        provenance::ArtboardProvenance,
        state::State,
        svc::{DartboardService, DartboardSnapshot},
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
            your_color: Some(RgbColor::new(255, 196, 64)),
            ..Default::default()
        };
        let svc = DartboardService::disconnected_for_tests(snapshot);
        State::new(svc, "viewer".to_string(), shared_provenance)
    }
}
