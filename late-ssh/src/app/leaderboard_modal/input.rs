use crate::app::{input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        handle_escape(app);
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_leaderboard_modal = false;
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}
