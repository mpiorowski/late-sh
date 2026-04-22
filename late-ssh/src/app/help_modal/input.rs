use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x09) => {
            app.help_modal_state.move_topic(1);
            return;
        }
        ParsedInput::BackTab => {
            app.help_modal_state.move_topic(-1);
            return;
        }
        _ => {}
    }

    if is_close_event(&event) {
        app.show_help = false;
        return;
    }

    match event {
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

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'?' | b'q' | b'Q') | ParsedInput::Char('?' | 'q' | 'Q')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_keys_include_esc_q_and_hidden_question_mark() {
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Char('?')));
        assert!(!is_close_event(&ParsedInput::Char('j')));
    }
}
