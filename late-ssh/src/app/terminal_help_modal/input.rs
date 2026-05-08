use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x09) => {
            app.terminal_help_modal_state.move_topic(1);
            return;
        }
        ParsedInput::BackTab => {
            app.terminal_help_modal_state.move_topic(-1);
            return;
        }
        _ => {}
    }

    if is_close_event(&event) {
        app.show_terminal_help = false;
        return;
    }

    match event {
        ParsedInput::Char('j') | ParsedInput::Char('J') | ParsedInput::Arrow(b'B') => {
            app.terminal_help_modal_state.scroll(1)
        }
        ParsedInput::Char('k') | ParsedInput::Char('K') | ParsedInput::Arrow(b'A') => {
            app.terminal_help_modal_state.scroll(-1)
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_terminal_help = false;
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}
