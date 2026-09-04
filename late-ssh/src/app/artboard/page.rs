use crate::app::{
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

use super::gallery::input::GalleryAction;
use super::ui::{info_hit, swatch_hit};

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

    // The rail, a listing, an archive list, or the hang flow sees the key
    // first; what it ignores (`i`, `?`, the Ctrl keys) still means what it
    // means on the board.
    if state.gallery_claims_input() {
        match super::gallery::input::handle_key(state, size, byte) {
            GalleryAction::Ignored => {}
            action => return handle_gallery_action(app, action),
        }
    }
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    match byte {
        0x1C => {
            state.toggle_ownership_overlay();
            true
        }
        0x1B => {
            // Esc on the board goes back to the rail; the rail unfolds.
            state.gallery_mut().focus_rail();
            true
        }
        b'i' | b'I' | b'\r' | b'\n' => {
            if state.is_archive_view_active() {
                return true;
            }
            app.activate_artboard_interaction();
            true
        }
        0x10 => {
            let action = super::input::handle_byte(state, size, byte);
            handle_action(app, action)
        }
        0x15 | 0x19 => {
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

    if state.gallery_claims_input() {
        match super::gallery::input::handle_arrow(state, size, key) {
            GalleryAction::Ignored => {}
            action => return handle_gallery_action(app, action),
        }
    }
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

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

    // The gallery sees every event while it has focus, and every mouse
    // event while the rail is up (a click on the rail must win over the
    // board under it). `Ignored` falls through to the board.
    if state.gallery_claims_input()
        || (matches!(event, ParsedInput::Mouse(_)) && state.gallery().rail_visible())
    {
        match super::gallery::input::handle_event(state, size, event) {
            GalleryAction::Ignored => {}
            action => return handle_gallery_action(app, action),
        }
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
        ParsedInput::Mouse(mouse)
            if matches!(mouse.kind, MouseEventKind::Down)
                && matches!(mouse.button, Some(MouseButton::Left))
                && !mouse.modifiers.shift
                && !mouse.modifiers.alt
                && !mouse.modifiers.ctrl
                && !state.is_archive_view_active() =>
        {
            if swatch_hit(size, state, mouse.x, mouse.y).is_some()
                || info_hit(size, state, mouse.x, mouse.y)
            {
                return true;
            }
            if !state.move_to_screen_point(size, mouse.x, mouse.y) {
                return false;
            }
            app.activate_artboard_interaction();
            true
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

fn handle_gallery_action(app: &mut App, action: GalleryAction) -> bool {
    match action {
        GalleryAction::Ignored => false,
        GalleryAction::Handled => true,
        GalleryAction::FocusBoard => {
            if let Some(state) = app.dartboard_state.as_mut()
                && state.is_archive_view_active()
            {
                state.exit_archive_view();
            }
            true
        }
        GalleryAction::BeginHang => {
            app.begin_artboard_hang();
            true
        }
        GalleryAction::OpenArchive(kind) => {
            if let Some(state) = app.dartboard_state.as_mut() {
                state.open_archive_list(kind);
            }
            true
        }
    }
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
#[path = "page_test.rs"]
mod page_test;
