use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B)
        | ParsedInput::Char('?')
        | ParsedInput::Char('q')
        | ParsedInput::Char('Q') => app.show_help = false,
        ParsedInput::Char('h') | ParsedInput::Char('H') | ParsedInput::Arrow(b'D') => {
            app.help_modal_state.move_topic(-1)
        }
        ParsedInput::Char('l') | ParsedInput::Char('L') | ParsedInput::Arrow(b'C') => {
            app.help_modal_state.move_topic(1)
        }
        ParsedInput::Char('j') | ParsedInput::Char('J') | ParsedInput::Arrow(b'B') => {
            app.help_modal_state.scroll(1)
        }
        ParsedInput::Char('k') | ParsedInput::Char('K') | ParsedInput::Arrow(b'A') => {
            app.help_modal_state.scroll(-1)
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_help = false;
}
