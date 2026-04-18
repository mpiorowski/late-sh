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
        ParsedInput::Scroll(delta) => app.help_modal_state.scroll((-delta * 3) as i16),
        ParsedInput::PageDown => app.help_modal_state.page_scroll(1, visible_height(app)),
        ParsedInput::PageUp => app.help_modal_state.page_scroll(-1, visible_height(app)),
        ParsedInput::Byte(0x04) => app.help_modal_state.page_scroll(1, visible_height(app)),
        ParsedInput::Byte(0x15) => app.help_modal_state.page_scroll(-1, visible_height(app)),
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_help = false;
}

fn visible_height(app: &App) -> u16 {
    app.size.1.saturating_sub(10).max(6)
}
