use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            handle_escape(app)
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_aquarium_modal = false;
}
