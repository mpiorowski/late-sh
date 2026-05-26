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
        ParsedInput::Byte(b'+' | b'=') | ParsedInput::Char('+' | '=') => {
            if let Some(banner) = app.shop_state.adjust_selected_aquarium_fish(1) {
                app.banner = Some(banner);
                return true;
            }
            false
        }
        ParsedInput::Byte(b'-' | b'_') | ParsedInput::Char('-' | '_') => {
            if let Some(banner) = app.shop_state.adjust_selected_aquarium_fish(-1) {
                app.banner = Some(banner);
                return true;
            }
            false
        }
        _ => false,
    }
}
