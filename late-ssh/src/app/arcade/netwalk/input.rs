use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::{Side, State, Turn};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'n' | b'N' => {
            if state.request_new_personal() {
                state.new_personal_board();
            }
        }
        b'p' | b'P' => state.show_personal(),
        b'd' | b'D' => state.show_daily(),
        b'[' => state.prev_difficulty(),
        b']' => state.next_difficulty(),
        b'r' | b'R' | b'0' => {
            if state.request_reset() {
                state.reset();
            }
        }
        b'k' | b'K' => {
            state.move_cursor(Side::North);
        }
        b'j' | b'J' => {
            state.move_cursor(Side::South);
        }
        b'h' | b'H' => {
            state.move_cursor(Side::West);
        }
        b'l' | b'L' => {
            state.move_cursor(Side::East);
        }
        b' ' | b'\r' | b'\n' => {
            state.rotate_at_cursor(Turn::Clockwise);
        }
        b'x' | b'X' => {
            state.rotate_at_cursor(Turn::CounterClockwise);
        }
        b'f' | b'F' => state.toggle_lock(),
        _ => return false,
    }
    true
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let side = match key {
        b'A' => Side::North,
        b'B' => Side::South,
        b'C' => Side::East,
        b'D' => Side::West,
        _ => return false,
    };
    state.move_cursor(side);
    true
}

/// A left click turns the clicked tile clockwise and puts the cursor on it.
pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let Some(index) = super::ui::hit_test(area, state.dimension(), x, y) else {
        return false;
    };
    state.click(index);
    true
}
