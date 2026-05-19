use crate::app::input::ParsedInput;
use crate::app::state::App;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'f') | ParsedInput::Char('f') => {
            app.goldfish_state.feed();
        }
        ParsedInput::Byte(b'd') | ParsedInput::Char('d') => {
            app.goldfish_state.decorate();
        }
        ParsedInput::Byte(b'l') | ParsedInput::Char('l') => {
            app.goldfish_state.light();
        }
        ParsedInput::Byte(b'w') | ParsedInput::Char('w') => {
            app.goldfish_state.change_water();
        }
        ParsedInput::Byte(b'a') | ParsedInput::Char('a') => {
            app.goldfish_state.add_friend();
        }
        ParsedInput::Byte(0x1B | b'q') | ParsedInput::Char('q') => {
            app.show_goldfish_modal = false;
        }
        _ => {}
    }
}
