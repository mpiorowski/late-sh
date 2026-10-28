use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::{State, Step};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    if state.show_rules() {
        match byte {
            b'!' | 0x1B => state.close_rules(),
            _ => {}
        }
        return true;
    }

    match byte {
        b'!' => {
            state.open_rules();
            true
        }
        b'k' | b'K' => state.move_cursor(Step::Up),
        b'j' | b'J' => state.move_cursor(Step::Down),
        b'h' | b'H' => state.move_cursor(Step::Left),
        b'l' | b'L' => state.move_cursor(Step::Right),
        b' ' => state.toggle_at_cursor(),
        b'\r' | b'\n' => state.submit(),
        b'x' | b'X' => state.clear_picks(),
        b'\t' => state.shuffle(),
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let step = match key {
        b'A' => Step::Up,
        b'B' => Step::Down,
        b'C' => Step::Right,
        b'D' => Step::Left,
        _ => return false,
    };
    state.move_cursor(step);
    true
}

/// A left click on a tile picks or unpicks it and puts the cursor on it.
pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    if state.show_rules() {
        return true;
    }
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    match super::ui::hit_test(area, state, x, y) {
        Some(position) => state.click(position),
        None => false,
    }
}
