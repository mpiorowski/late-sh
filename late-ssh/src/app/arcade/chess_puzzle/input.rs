use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::State;

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
        b'k' | b'K' => state.move_cursor(0, 1),
        b'j' | b'J' => state.move_cursor(0, -1),
        b'h' | b'H' => state.move_cursor(-1, 0),
        b'l' | b'L' => state.move_cursor(1, 0),
        b' ' | b'\r' | b'\n' => state.activate(),
        _ => return false,
    }
    true
}

/// Arrows move the cursor on screen; `move_cursor` turns that into board
/// squares for the player's side.
pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => state.move_cursor(0, 1),
        b'B' => state.move_cursor(0, -1),
        b'C' => state.move_cursor(1, 0),
        b'D' => state.move_cursor(-1, 0),
        _ => return false,
    }
    true
}

/// A left click picks up, puts down or moves, as `Space` on that square.
pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let Some(index) = super::ui::hit_test(area, state.player(), x, y) else {
        return false;
    };
    state.click(index);
    true
}
