//! Input for the full-screen daily board. Keyboard mirrors table chess
//! (arrows/wasd + Space/Enter, `r` resign, `p` piece graphics); clicks map
//! through the geometry the last render recorded.

use crate::app::daily::games::DailyGame;
use crate::app::games::chess_core::{board_ui, types::ChessPieceRenderMode};
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};
use crate::app::state::App;

/// Route one event to the board. Returns true when consumed.
pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(byte) => handle_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_key(app, *ch as u8),
        ParsedInput::Arrow(key) => {
            handle_arrow(app, *key);
            true
        }
        ParsedInput::Mouse(mouse) => handle_mouse(app, mouse),
        _ => false,
    }
}

pub(crate) fn handle_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'w' | b'W' | b'k' | b'K' => app.daily.board_move_cursor(0, 1),
        b's' | b'S' | b'j' | b'J' => app.daily.board_move_cursor(0, -1),
        b'a' | b'A' | b'h' | b'H' => app.daily.board_move_cursor(-1, 0),
        b'd' | b'D' | b'l' | b'L' => app.daily.board_move_cursor(1, 0),
        b' ' | b'\r' | b'\n' => app.daily.board_select_or_move(),
        b'r' | b'R' => app.daily.board_resign(),
        b'p' | b'P' => {
            if let Some(board) = &mut app.daily.board {
                board.piece_render_mode = match board.piece_render_mode {
                    ChessPieceRenderMode::Graphics => ChessPieceRenderMode::Ascii,
                    ChessPieceRenderMode::Ascii => ChessPieceRenderMode::Graphics,
                };
            }
        }
        b'q' | b'Q' | 0x1B => close_board(app),
        _ => return false,
    }
    true
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) {
    match key {
        b'A' => app.daily.board_move_cursor(0, 1),
        b'B' => app.daily.board_move_cursor(0, -1),
        b'C' => app.daily.board_move_cursor(1, 0),
        b'D' => app.daily.board_move_cursor(-1, 0),
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: &MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let Some(board) = &app.daily.board else {
        return false;
    };
    // Mouse coordinates are 1-based; the frame buffer is 0-based.
    let x = mouse.x.saturating_sub(1);
    let y = mouse.y.saturating_sub(1);

    // Battleship / connect4: hit-test the render-recorded target grid.
    // Battleship clicks resolve to a cell, connect4 clicks to a column.
    if let Some(grid) = board.target_geometry.get() {
        if x < grid.x || y < grid.y || x >= grid.x + grid.width || y >= grid.y + grid.height {
            return false;
        }
        let target = match board.detail.as_ref().map(|detail| detail.game.kind()) {
            Some(DailyGame::Battleship) => {
                let col = ((x - grid.x) / crate::app::daily::battleship_ui::CELL_W) as usize;
                let row = (y - grid.y) as usize;
                row * crate::app::daily::battleship::GRID + col
            }
            Some(DailyGame::ConnectFour) => {
                ((x - grid.x) / crate::app::daily::connect4_ui::CELL_W) as usize
            }
            _ => return false,
        };
        app.daily.board_click_target(target);
        return true;
    }

    let Some((board_area, tier)) = board.board_geometry.get() else {
        return false;
    };
    let orientation = app.daily.board_orientation();
    let Some(index) = board_ui::square_at(board_area, tier, orientation, x, y) else {
        return false;
    };
    app.daily.board_click_square(index);
    true
}

/// Leave the board: restore the screen the modal was opened from and reopen
/// the modal so multi-match move-making stays one keypress per hop.
pub(crate) fn close_board(app: &mut App) {
    let return_screen = app
        .daily
        .board
        .as_ref()
        .map(|board| board.return_screen)
        .unwrap_or(crate::app::common::primitives::Screen::Dashboard);
    app.daily.close_board();
    app.set_screen(return_screen);
    app.show_daily_modal = true;
    app.daily.mark_lobby_seen();
}
