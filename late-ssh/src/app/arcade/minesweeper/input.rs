use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::State;
use super::ui;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'n' | b'N' => {
            state.new_personal_board();
            return true;
        }
        b'p' | b'P' => {
            state.show_personal();
            return true;
        }
        b'd' | b'D' => {
            state.show_daily();
            return true;
        }
        b'[' => {
            state.prev_difficulty();
            return true;
        }
        b']' => {
            state.next_difficulty();
            return true;
        }
        b'o' | b'O' => {
            state.use_dot_style = !state.use_dot_style;
            return true;
        }
        b'{' => {
            state.scroll_up();
            return true;
        }
        b'}' => {
            state.scroll_down();
            return true;
        }
        _ => {}
    }

    if state.is_game_over {
        return false;
    }

    match byte {
        b'k' | b'K' => {
            state.move_cursor(-1, 0);
            true
        }
        b'j' | b'J' => {
            state.move_cursor(1, 0);
            true
        }
        b'h' | b'H' => {
            state.move_cursor(0, -1);
            true
        }
        b'l' | b'L' => {
            state.move_cursor(0, 1);
            true
        }
        b' ' | b'\r' | b'\n' => {
            state.reveal();
            true
        }
        b'f' | b'F' | b'x' | b'X' => {
            state.toggle_flag();
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    if state.is_game_over {
        return matches!(key, b'A' | b'B' | b'C' | b'D');
    }

    match key {
        b'A' => {
            state.move_cursor(-1, 0);
            true
        }
        b'B' => {
            state.move_cursor(1, 0);
            true
        }
        b'C' => {
            state.move_cursor(0, 1);
            true
        }
        b'D' => {
            state.move_cursor(0, -1);
            true
        }
        _ => false,
    }
}

pub fn handle_mouse(state: &mut State, area: Rect, mouse: MouseEvent) -> bool {
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            let Some(x) = mouse.x.checked_sub(1) else {
                return false;
            };
            let Some(y) = mouse.y.checked_sub(1) else {
                return false;
            };
            let Some((row, col)) =
                ui::hit_test(area, state.difficulty(), state.scroll_offset, x, y)
            else {
                return false;
            };
            state.cursor = (row, col);
            state.reveal();
            true
        }
        MouseEventKind::Down if mouse.button == Some(MouseButton::Right) => {
            let Some(x) = mouse.x.checked_sub(1) else {
                return false;
            };
            let Some(y) = mouse.y.checked_sub(1) else {
                return false;
            };
            let Some((row, col)) =
                ui::hit_test(area, state.difficulty(), state.scroll_offset, x, y)
            else {
                return false;
            };
            state.cursor = (row, col);
            state.toggle_flag();
            true
        }
        MouseEventKind::ScrollUp => {
            if mouse_over_board(area, state, mouse) {
                state.scroll_up();
                return true;
            }
            false
        }
        MouseEventKind::ScrollDown => {
            if mouse_over_board(area, state, mouse) {
                state.scroll_down();
                return true;
            }
            false
        }
        _ => false,
    }
}

fn mouse_over_board(area: Rect, state: &State, mouse: MouseEvent) -> bool {
    let Some(x) = mouse.x.checked_sub(1) else {
        return false;
    };
    let Some(y) = mouse.y.checked_sub(1) else {
        return false;
    };
    let rect = ui::hit_area(area, state.difficulty());
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}
