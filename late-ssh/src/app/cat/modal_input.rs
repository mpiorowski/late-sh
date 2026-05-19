use crate::app::input::ParsedInput;
use crate::app::state::App;

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'f') | ParsedInput::Char('f') => {
            app.cat_state.feed();
        }
        ParsedInput::Byte(b'w') | ParsedInput::Char('w') => {
            app.cat_state.water();
        }
        ParsedInput::Byte(b'p') | ParsedInput::Char('p') => {
            app.cat_state.play();
        }
        ParsedInput::Byte(b'g') | ParsedInput::Char('g') => {
            app.cat_state.groom();
        }
        ParsedInput::Byte(b't') | ParsedInput::Char('t') => {
            app.cat_state.treat();
        }
        ParsedInput::Byte(0x1B | b'q') | ParsedInput::Char('q') => {
            app.show_cat_modal = false;
        }
        _ => {}
    }
}
