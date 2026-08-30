use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::{Direction, State};

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
            state.move_blank(Direction::Down);
        }
        b'j' | b'J' => {
            state.move_blank(Direction::Up);
        }
        b'h' | b'H' => {
            state.move_blank(Direction::Right);
        }
        b'l' | b'L' => {
            state.move_blank(Direction::Left);
        }
        _ => return false,
    }
    true
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let direction = match key {
        b'A' => Direction::Down,
        b'B' => Direction::Up,
        b'C' => Direction::Left,
        b'D' => Direction::Right,
        _ => return false,
    };
    state.move_blank(direction);
    true
}

pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let Some(index) = super::ui::hit_test(area, state.difficulty(), x, y) else {
        return false;
    };

    state.move_tile(index);
    true
}
