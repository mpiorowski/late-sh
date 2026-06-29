use ratatui::layout::Rect;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

use super::state::{ResetKind, State};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'n' | b'N' => {
            if state.request_reset(ResetKind::NewBoard) {
                state.new_personal_board();
            }
            true
        }
        b'[' => {
            state.prev_difficulty();
            true
        }
        b']' => {
            state.next_difficulty();
            true
        }
        b'p' | b'P' => {
            state.show_personal();
            true
        }
        b'd' | b'D' => {
            state.show_daily();
            true
        }
        b'{' => {
            state.scroll_up();
            true
        }
        b'}' => {
            state.scroll_down();
            true
        }
        b'r' | b'R' => {
            if state.request_reset(ResetKind::Reset) {
                state.reset_board();
            }
            true
        }
        b'a' | b'A' => state.auto_move(),
        b'f' | b'F' => state.auto_foundation_all(),
        b'u' | b'U' => state.undo(),
        b'h' | b'H' => {
            state.move_horizontal(-1);
            true
        }
        b'l' | b'L' => {
            state.move_horizontal(1);
            true
        }
        b'k' | b'K' => {
            state.move_vertical(-1);
            true
        }
        b'j' | b'J' => {
            state.move_vertical(1);
            true
        }
        b' ' | b'\r' | b'\n' => state.activate(),
        b'c' | b'C' | 0x1B => {
            state.selection = None;
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
            let Some(focus) = super::ui::hit_test(area, state, x, y) else {
                return false;
            };
            state.cursor = focus;
            let _ = state.activate();
            true
        }
        MouseEventKind::Down if mouse.button == Some(MouseButton::Right) => {
            let Some(x) = mouse.x.checked_sub(1) else {
                return false;
            };
            let Some(y) = mouse.y.checked_sub(1) else {
                return false;
            };
            let Some(focus) = super::ui::hit_test(area, state, x, y) else {
                return false;
            };
            state.cursor = focus;
            let _ = state.auto_move();
            true
        }
        MouseEventKind::ScrollUp => {
            if mouse_over_board(area, mouse) {
                state.scroll_up();
                return true;
            }
            false
        }
        MouseEventKind::ScrollDown => {
            if mouse_over_board(area, mouse) {
                state.scroll_down();
                return true;
            }
            false
        }
        _ => false,
    }
}

fn mouse_over_board(area: Rect, mouse: MouseEvent) -> bool {
    let Some(x) = mouse.x.checked_sub(1) else {
        return false;
    };
    let Some(y) = mouse.y.checked_sub(1) else {
        return false;
    };
    let rect = super::ui::hit_area(area);
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => {
            state.move_vertical(-1);
            true
        }
        b'B' => {
            state.move_vertical(1);
            true
        }
        b'C' => {
            state.move_horizontal(1);
            true
        }
        b'D' => {
            state.move_horizontal(-1);
            true
        }
        _ => false,
    }
}
