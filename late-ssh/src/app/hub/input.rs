use crate::app::{hub::state::HubTab, input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            handle_escape(app)
        }
        ParsedInput::Byte(b'\t') => app.hub_state.select_next_tab(),
        ParsedInput::BackTab => app.hub_state.select_previous_tab(),
        ParsedInput::Arrow(b'C') => app.hub_state.select_next_tab(),
        ParsedInput::Arrow(b'D') => app.hub_state.select_previous_tab(),
        ParsedInput::Char('1') | ParsedInput::Byte(b'1') => {
            app.hub_state.open(HubTab::Leaderboard);
        }
        ParsedInput::Char('2') | ParsedInput::Byte(b'2') => {
            app.hub_state.open(HubTab::Dailies);
        }
        ParsedInput::Char('3') | ParsedInput::Byte(b'3') => {
            app.hub_state.open(HubTab::Shop);
        }
        ParsedInput::Char('4') | ParsedInput::Byte(b'4') => {
            app.hub_state.open(HubTab::Events);
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_hub_modal = false;
}
