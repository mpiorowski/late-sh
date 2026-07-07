use crate::app::{
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

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
        ParsedInput::Mouse(mouse) => {
            handle_mouse(app, mouse);
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

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return;
    };
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            if let Some(topic) = app.help_modal_state.topic_at_point(x, y) {
                // Double-click on a tab is treated as a plain switch — there's
                // no deeper verb here, and the scroll behavior already lives
                // on the wheel.
                let _ = app.help_modal_state.click_topic(topic);
            }
        }
        MouseEventKind::ScrollUp if app.help_modal_state.body_contains(x, y) => {
            app.help_modal_state.scroll(-3);
        }
        MouseEventKind::ScrollDown if app.help_modal_state.body_contains(x, y) => {
            app.help_modal_state.scroll(3);
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
    fn close_keys_include_question_mark_esc_and_q() {
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Char('?')));
        assert!(!is_close_event(&ParsedInput::Char('j')));
    }
}
