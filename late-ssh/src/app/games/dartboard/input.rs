use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};

use super::state::State;

pub enum InputAction {
    Ignored,
    Handled,
    Leave,
}

pub fn handle_byte(state: &mut State, screen_size: (u16, u16), byte: u8) -> InputAction {
    state.set_viewport_for_screen(screen_size);
    match byte {
        0x11 => InputAction::Leave, // Ctrl+Q
        // Ctrl+C and Ctrl+X are swallowed: left unbound they bubble up to
        // the global input dispatcher and tear down the SSH session. Match
        // standalone dartboard, which also treats them as no-ops in
        // drawing mode.
        0x03 | 0x18 => InputAction::Handled,
        b'\r' | b'\n' => {
            state.move_down(screen_size);
            InputAction::Handled
        }
        0x08 | 0x7f => {
            state.backspace(screen_size);
            InputAction::Handled
        }
        _ if byte.is_ascii_graphic() || byte == b' ' => {
            state.paint_char(byte as char);
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}

pub fn handle_arrow(state: &mut State, screen_size: (u16, u16), key: u8) -> bool {
    state.set_viewport_for_screen(screen_size);
    match key {
        b'A' => {
            state.move_up(screen_size);
            true
        }
        b'B' => {
            state.move_down(screen_size);
            true
        }
        b'C' => {
            state.move_right(screen_size);
            true
        }
        b'D' => {
            state.move_left(screen_size);
            true
        }
        _ => false,
    }
}

const BIG_STEP: usize = 10;

pub(crate) fn handle_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> InputAction {
    state.set_viewport_for_screen(screen_size);
    match event {
        ParsedInput::Home => {
            state.move_home(screen_size);
            InputAction::Handled
        }
        ParsedInput::End => {
            state.move_end(screen_size);
            InputAction::Handled
        }
        ParsedInput::PageUp => {
            state.move_page_up(screen_size);
            InputAction::Handled
        }
        ParsedInput::PageDown => {
            state.move_page_down(screen_size);
            InputAction::Handled
        }
        ParsedInput::Delete => {
            state.clear_at_cursor();
            InputAction::Handled
        }
        ParsedInput::ShiftArrow(key) => {
            for _ in 0..BIG_STEP {
                move_arrow(state, screen_size, *key);
            }
            InputAction::Handled
        }
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

fn move_arrow(state: &mut State, screen_size: (u16, u16), key: u8) {
    match key {
        b'A' => state.move_up(screen_size),
        b'B' => state.move_down(screen_size),
        b'C' => state.move_right(screen_size),
        b'D' => state.move_left(screen_size),
        _ => {}
    }
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
    match mouse.kind {
        MouseEventKind::Moved => {
            if state.move_to_screen_point(screen_size, mouse.x, mouse.y) {
                InputAction::Handled
            } else {
                InputAction::Ignored
            }
        }
        MouseEventKind::Down | MouseEventKind::Drag
            if matches!(mouse.button, Some(MouseButton::Left)) =>
        {
            if state.move_to_screen_point(screen_size, mouse.x, mouse.y) {
                InputAction::Handled
            } else {
                InputAction::Ignored
            }
        }
        _ => InputAction::Ignored,
    }
}
