use crate::app::{
    input::{MouseEventKind, ParsedInput},
    state::App,
};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
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
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.profile_modal_state.scroll_by(-3),
            MouseEventKind::ScrollDown => app.profile_modal_state.scroll_by(3),
            _ => {}
        },
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

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(b'q' | b'Q' | 0x1B) | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_profile_modal = false;
    app.profile_modal_state.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_keys_include_printable_q_variants() {
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Byte(b'q')));
        assert!(is_close_event(&ParsedInput::Byte(b'Q')));
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(!is_close_event(&ParsedInput::Char('j')));
    }
}
