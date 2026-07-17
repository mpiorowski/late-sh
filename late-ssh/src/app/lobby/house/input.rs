//! Input for the full-screen house table (`Screen::HouseTable`). The
//! embedded table chat owns its keys first, exactly like the daily board
//! and the old active-room split: `i`/`j`/`k`/Ctrl+D/Ctrl+U always route to
//! chat, message-action keys route to chat while a message is selected,
//! everything else goes to the game. `q`/Esc drop back to the Lobby modal.

use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::state::App;
use crate::app::{common::primitives::Screen, lobby::house::types::InputAction};

/// Route one event to the table. Returns true when consumed.
pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(byte) => handle_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_key(app, *ch as u8),
        ParsedInput::Arrow(key) => {
            handle_arrow(app, *key);
            true
        }
        ParsedInput::PageUp => handle_scroll(app, page_step(app)),
        ParsedInput::PageDown => handle_scroll(app, -page_step(app)),
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => handle_scroll(app, 1),
            MouseEventKind::ScrollDown => handle_scroll(app, -1),
            _ => false,
        },
        _ => false,
    }
}

pub(crate) fn handle_key(app: &mut App, byte: u8) -> bool {
    if byte == b'`' {
        return crate::app::lobby::workspace::cycle_game_workspace(app);
    }
    app.house.client().inspect(|client| client.touch_activity());
    if let Some(chat_room_id) = app.house.chat_room_id() {
        if byte == 0x1B
            && app
                .chat
                .selected_message_body_in_room(chat_room_id)
                .is_some()
        {
            app.chat.clear_message_selection();
            return true;
        }
        if crate::app::chat::input::chat_priority_key(app, byte)
            && crate::app::chat::input::handle_message_action_in_room(app, chat_room_id, byte)
        {
            return true;
        }
        if crate::app::chat::input::selected_chat_key(app, chat_room_id, byte)
            && crate::app::chat::input::handle_message_action_in_room(app, chat_room_id, byte)
        {
            return true;
        }
    }
    let action = match app.house.client_mut() {
        Some(client) => client.handle_key(byte),
        None => InputAction::Leave,
    };
    match action {
        InputAction::Handled => true,
        InputAction::Leave => {
            close_table(app);
            true
        }
        InputAction::Ignored => match byte {
            b'q' | b'Q' | 0x1B => {
                close_table(app);
                true
            }
            _ => false,
        },
    }
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) {
    app.house.client().inspect(|client| client.touch_activity());
    if let Some(client) = app.house.client_mut()
        && client.handle_arrow(key)
    {
        return;
    }
    if let Some(chat_room_id) = app.house.chat_room_id() {
        let _ = crate::app::chat::input::handle_message_arrow_in_room(app, chat_room_id, key);
    }
}

fn handle_scroll(app: &mut App, delta: isize) -> bool {
    let Some(chat_room_id) = app.house.chat_room_id() else {
        return false;
    };
    crate::app::chat::input::handle_scroll_in_room(app, chat_room_id, delta);
    true
}

fn page_step(app: &App) -> isize {
    (app.size.1 / 6).max(1) as isize
}

/// Leave the table: restore the screen the modal was opened from and reopen
/// the modal, one keypress per hop — same shape as the daily board.
pub(crate) fn close_table(app: &mut App) {
    let return_screen = app.house.return_screen;
    leave_table(app, return_screen);
    app.show_lobby_modal = true;
    app.lobby.mark_seen(&app.daily);
}

/// Shared teardown: clear any lingering table-chat selection, close the
/// table, land on `target`. The backtick cycle uses this directly (no
/// modal); `close_table` layers the modal reopen on top.
pub(crate) fn leave_table(app: &mut App, target: Screen) {
    if let Some(chat_room_id) = app.house.chat_room_id()
        && app
            .chat
            .selected_message_body_in_room(chat_room_id)
            .is_some()
    {
        app.chat.clear_message_selection();
    }
    app.house.close();
    app.set_screen(target);
}
