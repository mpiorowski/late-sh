use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::State;
use super::ui::KeyboardKey;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    if state.show_rules {
        match byte {
            b'!' | b'q' | b'Q' | 0x1B => state.close_rules(),
            _ => {}
        }
        return true;
    }

    match byte {
        b'!' => {
            state.open_rules();
            true
        }
        b'\r' | b'\n' => state.submit_guess(),
        0x08 | 0x7F => state.pop_letter(),
        b'a'..=b'z' | b'A'..=b'Z' => state.push_letter(byte as char),
        _ => false,
    }
}

pub fn handle_arrow(_state: &mut State, key: u8) -> bool {
    matches!(key, b'A' | b'B' | b'C' | b'D')
}

pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    if state.show_rules {
        return true;
    }

    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let Some(x) = mouse.x.checked_sub(1) else {
        return false;
    };
    let Some(y) = mouse.y.checked_sub(1) else {
        return false;
    };

    let content_area = crate::app::arcade::ui::game_content_area(area, true, true);
    match super::ui::keyboard_hit_test(content_area, x, y) {
        Some(KeyboardKey::Letter(ch)) => state.push_letter(ch),
        Some(KeyboardKey::Backspace) => state.pop_letter(),
        Some(KeyboardKey::Enter) => state.submit_guess(),
        None => false,
    }
}
