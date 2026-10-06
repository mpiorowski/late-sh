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
            preview_selected(app);
            true
        }
        _ => false,
    }
}

/// Enter on a mention always opens the Ctrl+/ modal as a single-message
/// preview: the mention with its surrounding conversation, whatever its age.
/// Enter inside the modal performs the actual room jump, so going to the
/// room is Enter-Enter while reading in place costs nothing.
fn preview_selected(app: &mut App) {
    let Some(item) = app.chat.notifications.selected_item() else {
        return;
    };
    let message_id = item.message_id;
    crate::app::input::open_message_search_modal_globally(app, "");
    app.chat.start_message_preview(message_id);
}
