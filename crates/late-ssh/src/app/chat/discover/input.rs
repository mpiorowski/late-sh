use crate::app::state::App;

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.discover.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.discover.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' | b'J' => {
            app.chat.discover.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.discover.move_selection(-1);
            true
        }
        b'\r' | b'\n' => {
            if let Some(banner) = app.chat.join_selected_discover_room() {
                app.banner = Some(banner);
            }
            true
        }
        _ => false,
    }
}
