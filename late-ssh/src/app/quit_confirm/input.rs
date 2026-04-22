use crate::app::{input::ParsedInput, state::App};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuitAction {
    OpenConfirm,
    QuitNow,
}

pub(crate) fn action_for(showing_confirm: bool) -> QuitAction {
    if showing_confirm {
        QuitAction::QuitNow
    } else {
        QuitAction::OpenConfirm
    }
}

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.running = false;
        }
        ParsedInput::Byte(0x1B) => app.show_quit_confirm = false,
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    app.show_quit_confirm = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_q_confirms_and_escape_dismisses() {
        assert_eq!(action_for(false), QuitAction::OpenConfirm);
        assert_eq!(action_for(true), QuitAction::QuitNow);
    }
}
