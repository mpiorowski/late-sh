use crate::app::input::ParsedInput;
use crate::app::state::App;

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if app.cat_state.play_session().is_some() {
        match event {
            ParsedInput::Arrow(b'D')
            | ParsedInput::Byte(b'h' | b'H' | b'a' | b'A')
            | ParsedInput::Char('h' | 'H' | 'a' | 'A') => {
                app.cat_state.move_play_toy_left();
            }
            ParsedInput::Arrow(b'C')
            | ParsedInput::Byte(b'l' | b'L' | b'd' | b'D')
            | ParsedInput::Char('l' | 'L' | 'd' | 'D') => {
                app.cat_state.move_play_toy_right();
            }
            ParsedInput::Arrow(b'A')
            | ParsedInput::Byte(b'k' | b'K' | b'w' | b'W')
            | ParsedInput::Char('k' | 'K' | 'w' | 'W') => {
                app.cat_state.move_play_toy_up();
            }
            ParsedInput::Arrow(b'B')
            | ParsedInput::Byte(b'j' | b'J' | b's' | b'S')
            | ParsedInput::Char('j' | 'J' | 's' | 'S') => {
                app.cat_state.move_play_toy_down();
            }
            ParsedInput::Byte(b' ' | b'\r' | b'\n' | b'p' | b'P')
            | ParsedInput::Char(' ' | 'p' | 'P') => {
                app.cat_state.dash_play_toy();
            }
            ParsedInput::Byte(b'c' | b'C') | ParsedInput::Char('c' | 'C') => {
                app.cat_state.cancel_play();
            }
            ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
                app.cat_state.cancel_play();
                app.show_cat_modal = false;
            }
            _ => {}
        }
        return;
    }

    match event {
        ParsedInput::Byte(b'f' | b'F') | ParsedInput::Char('f' | 'F') => {
            app.cat_state.feed();
        }
        ParsedInput::Byte(b'w' | b'W') | ParsedInput::Char('w' | 'W') => {
            app.cat_state.water();
        }
        ParsedInput::Byte(b'p' | b'P') | ParsedInput::Char('p' | 'P') => {
            app.cat_state.play();
        }
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.show_cat_modal = false;
        }
        _ => {}
    }
}
