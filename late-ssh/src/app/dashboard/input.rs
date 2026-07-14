use crate::app::{chat, state::App};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    chat::input::handle_arrow(app, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.music_prefix_armed {
        app.music_prefix_armed = false;
        if crate::app::audio::input::handle_music_suffix(app, byte, true) {
            return true;
        }
    }

    if byte == b'`' {
        return crate::app::lobby::workspace::cycle_game_workspace(app);
    }

    if matches!(byte, b'v' | b'V') {
        app.music_prefix_armed = true;
        return true;
    }

    chat::input::handle_byte(app, byte)
}
