use crate::app::common::primitives::Banner;
use crate::app::state::App;

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.notifications.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.notifications.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' | b'J' => {
            app.chat.notifications.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.notifications.move_selection(-1);
            true
        }
        b'\r' | b'\n' => {
            jump_to_selected(app);
            true
        }
        _ => false,
    }
}

fn jump_to_selected(app: &mut App) {
    let Some(item) = app.chat.notifications.selected_item() else {
        return;
    };
    let room_id = item.room_id;
    let message_id = item.message_id;
    let room_label = item
        .room_slug
        .as_deref()
        .map(|s| format!("#{s}"))
        .unwrap_or_else(|| "room".to_string());

    app.chat.notifications_selected = false;
    app.chat.selected_room_id = Some(room_id);
    app.chat.selected_message_id = Some(message_id);
    app.chat.highlighted_message_id = Some(message_id);
    app.sync_visible_chat_room();
    app.chat.request_list();
    app.banner = Some(Banner::success(&format!("Jumped to {room_label}")));
}
