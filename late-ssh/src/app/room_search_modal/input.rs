use crate::app::{common::primitives::Screen, input::ParsedInput, state::App};

use super::state::filtered_items;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    let len = filtered_items(&app.chat, app.user_id, app.room_search_modal_state.query()).len();
    app.room_search_modal_state.clamp(len);

    match event {
        ParsedInput::Byte(0x1B) => app.room_search_modal_state.close(),
        ParsedInput::Byte(b'\r' | b'\n') => submit(app),
        ParsedInput::Byte(0x7F) => app.room_search_modal_state.backspace(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => {
            app.room_search_modal_state.delete_word_left();
        }
        ParsedInput::Arrow(b'B') => {
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

    let len = filtered_items(&app.chat, app.user_id, app.room_search_modal_state.query()).len();
    app.room_search_modal_state.clamp(len);
}

fn submit(app: &mut App) {
    let items = filtered_items(&app.chat, app.user_id, app.room_search_modal_state.query());
    let Some(item) = items.get(app.room_search_modal_state.selected()).cloned() else {
        return;
    };

    app.chat.reset_composer();
    app.chat.feeds.stop_processing();
    app.chat.news.stop_composing();
    app.chat.showcase.stop_composing();
    app.chat.work.stop_composing();
    app.chat.close_news_modal();
    app.chat.select_room_slot(item.slot);
    app.room_search_modal_state.close();
    app.set_screen(Screen::Chat);
    app.sync_visible_chat_room();
}
