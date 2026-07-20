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
        // Ctrl+U / Ctrl+Y cycle paint color without claiming printable glyphs.
        0x15 => {
            state.cycle_paint_color(-1);
            InputAction::Handled
        }
        0x19 => {
            state.cycle_paint_color(1);
            InputAction::Handled
        }
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
    // These legacy C0 chords used to drive shape push/pull ops; leave them
    // unmapped so the artboard no longer claims them as editor shortcuts.
    if matches!(byte, 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0F | 0x15 | 0x19) {
        return None;
    }

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
        ParsedInput::Char('q' | 'Q' | '?') | ParsedInput::Byte(0x1B | b'q' | b'Q' | b'?') => {
            state.close_help()
        }
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
#[path = "input_test.rs"]
mod input_test;

