use crate::app::{
    chat::state::RoomSlot, common::primitives::Banner, common::primitives::Screen,
    input::ParsedInput, state::App,
};

use super::state::{ModalQuery, filtered_items, parse_modal_query};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    let message_mode = matches!(
        parse_modal_query(app.room_search_modal_state.query()),
        ModalQuery::Messages(_)
    );
    let len = if message_mode {
        app.chat.message_search.hits.len()
    } else {
        filtered_items(&app.chat, app.user_id, app.room_search_modal_state.query()).len()
    };
    app.room_search_modal_state.clamp(len);

    match event {
        ParsedInput::Byte(0x1B) => {
            app.room_search_modal_state.close();
            app.chat.message_search.clear();
        }
        ParsedInput::Byte(b'\r') => {
            if message_mode {
                submit_message_jump(app);
            } else {
                submit(app);
            }
        }
        // Ctrl+Y: copy the selected search hit's body (plain `y`/`c` are
        // query text while the modal input is focused).
        ParsedInput::Byte(0x19) if message_mode => copy_selected_hit(app),
        ParsedInput::Byte(0x7F | 0x08) => app.room_search_modal_state.backspace(),
        ParsedInput::CtrlBackspace => {
            app.room_search_modal_state.delete_word_left();
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
            app.room_search_modal_state.move_selection(1, len);
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => {
            app.room_search_modal_state.move_selection(-1, len);
        }
        ParsedInput::PageDown => app.room_search_modal_state.move_selection(8, len),
        ParsedInput::PageUp => app.room_search_modal_state.move_selection(-8, len),
        ParsedInput::Char(ch) => app.room_search_modal_state.push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.room_search_modal_state.push(byte as char);
        }
        _ => {}
    }

    let message_mode = matches!(
        parse_modal_query(app.room_search_modal_state.query()),
        ModalQuery::Messages(_)
    );
    let len = if message_mode {
        app.chat.message_search.hits.len()
    } else {
        filtered_items(&app.chat, app.user_id, app.room_search_modal_state.query()).len()
    };
    app.room_search_modal_state.clamp(len);
}

fn submit(app: &mut App) {
    let items = filtered_items(&app.chat, app.user_id, app.room_search_modal_state.query());
    let Some(item) = items.get(app.room_search_modal_state.selected()).cloned() else {
        return;
    };

    close_into_room(app, item.slot);
}

/// Enter on a search hit: land in the hit's room, then select the message if
/// it is (or becomes, once the tail loads) part of the loaded history.
fn submit_message_jump(app: &mut App) {
    let Some(hit) = app
        .chat
        .message_search
        .hits
        .get(app.room_search_modal_state.selected())
    else {
        return;
    };
    let room_id = hit.message.room_id;
    let message_id = hit.message.id;

    // A mention preview can reference a public room the user never joined;
    // there is no room to land in, so keep the modal open instead of
    // selecting a room the rail does not have.
    if !app.chat.rooms.iter().any(|(room, _)| room.id == room_id) {
        app.banner = Some(Banner::error("Join that room from Discover to jump there"));
        return;
    }

    close_into_room(app, RoomSlot::Room(room_id));
    app.chat.message_search.clear();
    if app.chat.message_is_loaded_in_room(room_id, message_id) {
        app.chat.select_message_by_id_in_room(room_id, message_id);
    } else {
        app.chat.set_pending_search_jump(room_id, message_id);
    }
}

fn close_into_room(app: &mut App, slot: RoomSlot) {
    app.chat.reset_composer();
    app.chat.feeds.stop_processing();
    app.chat.news.stop_composing();
    app.chat.showcase.stop_composing();
    app.chat.work.stop_composing();
    app.chat.close_news_modal();
    app.chat.select_room_slot(slot);
    app.room_search_modal_state.close();
    app.set_screen(Screen::Dashboard);
    app.sync_visible_chat_room();
}

fn copy_selected_hit(app: &mut App) {
    let Some(hit) = app
        .chat
        .message_search
        .hits
        .get(app.room_search_modal_state.selected())
    else {
        return;
    };
    app.pending_clipboard = Some(hit.message.body.clone());
    app.banner = Some(Banner::success("Message copied to clipboard!"));
}
