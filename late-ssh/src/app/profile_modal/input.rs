use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Byte(0x1B) => close(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            app.profile_modal_state.scroll_by(1);
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            app.profile_modal_state.scroll_by(-1);
        }
        ParsedInput::Scroll(delta) => app.profile_modal_state.scroll_by((-delta * 3) as i16),
        ParsedInput::PageDown => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(step);
        }
        ParsedInput::PageUp => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(-step);
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    close(app);
}

fn close(app: &mut App) {
    app.show_profile_modal = false;
    app.profile_modal_state.close();
}
