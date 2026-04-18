use crate::app::state::App;

pub fn handle_byte(app: &mut App, byte: u8) {
    match byte {
        b'e' | b'E' | b' ' | b'\r' => {
            app.welcome_modal_state.open_from_profile(
                app.profile_state.profile(),
                crate::app::welcome_modal::ui::MODAL_WIDTH,
            );
            app.show_welcome = true;
        }
        b'j' | b'J' => app.profile_state.scroll_by(1),
        b'k' | b'K' => app.profile_state.scroll_by(-1),
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.profile_state.scroll_by(-1);
            true
        }
        b'B' => {
            app.profile_state.scroll_by(1);
            true
        }
        _ => false,
    }
}

pub fn handle_composer_input(_app: &mut App, _byte: u8) {}
