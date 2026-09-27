use ratatui::layout::Position;

use crate::app::{
    input::{MouseButton, MouseEvent, MouseEventKind},
    state::App,
};

use super::state::LeaderboardPageState;

pub(crate) fn handle_key(app: &mut App, byte: u8) {
    match byte {
        b'j' => app.leaderboard_page.select_next(),
        b'k' => app.leaderboard_page.select_previous(),
        b'\n' => app.leaderboard_page.scroll_by(1),
        0x0B => app.leaderboard_page.scroll_by(-1),
        _ => {}
    }
}

pub(crate) fn handle_mouse(state: &mut LeaderboardPageState, mouse: MouseEvent) -> bool {
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let point = Position::new(x, y);
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            if let Some(index) = state.board_at(point) {
                state.select(index);
                return true;
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let delta = if mouse.kind == MouseEventKind::ScrollUp {
                -1
            } else {
                1
            };
            if state.over_rail(point) {
                state.wheel_select(delta);
                return true;
            }
            if state.over_content(point) {
                state.scroll_by(delta * 3);
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Arrow keys mirror j/k. Returns whether the key was consumed.
pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'B' => {
            app.leaderboard_page.select_next();
            true
        }
        b'A' => {
            app.leaderboard_page.select_previous();
            true
        }
        _ => false,
    }
}
