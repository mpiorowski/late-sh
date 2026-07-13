//! Input for the full-screen daily board. Keyboard mirrors table chess
//! (arrows/wasd + Space/Enter, `r` resign, `p` piece graphics); clicks map
//! through the geometry the last render recorded. The embedded match chat
//! owns its keys first, exactly like the active-room split: `i`/`j`/`k`
//! always route to chat, and the message-action keys route to chat while a
//! match-chat message is selected.

use uuid::Uuid;

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
    if byte == b'`' {
        return crate::app::dashboard::input::cycle_game_workspace(app);
    }
    if let Some(chat_room_id) = app.daily.board_chat_room_id() {
        if byte == 0x1B
            && app
                .chat
                .selected_message_body_in_room(chat_room_id)
                .is_some()
        {
            app.chat.clear_message_selection();
            return true;
        }
        if chat_priority_key(app, byte)
            && crate::app::chat::input::handle_message_action_in_room(app, chat_room_id, byte)
        {
            return true;
        }
        if selected_chat_key(app, chat_room_id, byte)
            && crate::app::chat::input::handle_message_action_in_room(app, chat_room_id, byte)
        {
            return true;
        }
    }
    match byte {
        // `j`/`k` belong to chat message selection (routed above); the board
        // cursor keeps wasd + arrows, same as table chess.
        b'w' | b'W' => app.daily.board_move_cursor(0, 1),
        b's' | b'S' => app.daily.board_move_cursor(0, -1),
        b'a' | b'A' => app.daily.board_move_cursor(-1, 0),
        b'd' | b'D' => app.daily.board_move_cursor(1, 0),
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

/// Keys chat always wins, mirroring the active-room split: reaction-leader
/// digits, `i` compose, `j`/`k` selection, Ctrl+D/Ctrl+U half-page.
fn chat_priority_key(app: &App, byte: u8) -> bool {
    if app.chat.is_reaction_leader_active() {
        return true;
    }
    matches!(byte, b'i' | b'I' | b'j' | b'J' | b'k' | b'K' | 0x04 | 0x15)
}

/// Message-action keys that route to chat only while a match-chat message is
/// selected; otherwise the board keeps them (`r` resign, `p` pieces, Enter
/// play).
fn selected_chat_key(app: &App, chat_room_id: Uuid, byte: u8) -> bool {
    let selected_in_room = app
        .chat
        .selected_message_body_in_room(chat_room_id)
        .is_some();
    selected_in_room
        && matches!(
            byte,
            b'd' | b'D'
                | b'r'
                | b'R'
                | b'e'
                | b'E'
                | b'p'
                | b'c'
                | b'f'
                | b'F'
                | b'g'
                | b'G'
                | b'\r'
                | b'\n'
                | 0x10
        )
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) {
    // The board renders a cursor only while it's your move; every other
    // moment (waiting, finished, spectating) arrows are chat selection.
    if board_wants_arrows(app) {
        match key {
            b'A' => app.daily.board_move_cursor(0, 1),
            b'B' => app.daily.board_move_cursor(0, -1),
            b'C' => app.daily.board_move_cursor(1, 0),
            b'D' => app.daily.board_move_cursor(-1, 0),
            _ => {}
        }
        return;
    }
    if let Some(chat_room_id) = app.daily.board_chat_room_id() {
        let _ = crate::app::chat::input::handle_message_arrow_in_room(app, chat_room_id, key);
    }
}

fn board_wants_arrows(app: &App) -> bool {
    let Some(board) = &app.daily.board else {
        return false;
    };
    if board.spectating {
        return false;
    }
    board.detail.as_ref().is_some_and(|detail| {
        detail.is_active() && detail.row.turn_user_id == Some(app.daily.user_id())
    })
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
    leave_board(app, return_screen);
    app.show_daily_modal = true;
    app.daily.mark_lobby_seen();
}

/// Shared board teardown: ack + drop the board, clear any lingering match
/// chat selection, land on `target`. The backtick cycle uses this directly
/// (back to Home chat, no modal); `close_board` layers the modal reopen on
/// top.
pub(crate) fn leave_board(app: &mut App, target: crate::app::common::primitives::Screen) {
    // Don't let a selected match-chat message follow the user off-screen.
    if let Some(chat_room_id) = app.daily.board_chat_room_id()
        && app
            .chat
            .selected_message_body_in_room(chat_room_id)
            .is_some()
    {
        app.chat.clear_message_selection();
    }
    app.daily.close_board();
    app.set_screen(target);
}
