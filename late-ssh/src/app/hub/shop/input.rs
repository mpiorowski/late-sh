use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.shop_state.move_selection(-1);
            true
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.shop_state.move_selection(1);
            true
        }
        ParsedInput::Byte(b'[') | ParsedInput::Char('[') => {
            app.shop_state.select_previous_category();
            true
        }
        ParsedInput::Byte(b']') | ParsedInput::Char(']') => {
            app.shop_state.select_next_category();
            true
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            if let Some(banner) = app.shop_state.activate_selected() {
                app.banner = Some(banner);
            }
            true
        }
        _ => false,
    }
}
