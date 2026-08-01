use crate::app::{input::ParsedInput, state::App};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if crate::app::hub::shop::input::handle_input(app, &event) {
        return;
    }

    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            handle_escape(app)
        }
        // With the Admin tab gone, the Shop is the hub's only surface, so
        // the tab-switch chords repoint at the Shop's own category tabs
        // instead of doing nothing.
        ParsedInput::Byte(b'\t') | ParsedInput::Arrow(b'C') => {
            app.shop_state.select_next_category();
        }
        ParsedInput::BackTab | ParsedInput::Arrow(b'D') => {
            app.shop_state.select_previous_category();
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    // A bare Esc arrives via `dispatch_escape`, not `handle_input`, so the
    // shop's pending picker/confirm must be peeled here too.
    if crate::app::hub::shop::input::handle_escape(app) {
        return;
    }
    app.show_hub_modal = false;
}
