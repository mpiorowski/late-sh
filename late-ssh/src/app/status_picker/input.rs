//! Key handling for the `/status` picker. Every arm ends the modal's job in
//! one place: Enter commits through `App::set_status`, everything else moves
//! the highlight.

use crate::app::{
    chat::input::status_set_message, common::primitives::Banner, common::status::SessionStatus,
    input::ParsedInput, state::App,
};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.status_picker.close(),
        ParsedInput::Byte(b'\r') => commit(app),
        ParsedInput::Arrow(b'A') => app.status_picker.move_selection(-1),
        ParsedInput::Arrow(b'B') => app.status_picker.move_selection(1),
        ParsedInput::Arrow(b'D') => app.status_picker.cycle_duration(-1),
        ParsedInput::Arrow(b'C') => app.status_picker.cycle_duration(1),
        // A digit both jumps and commits: picking a status by number is the
        // regular's path, and making them press Enter after would only add a
        // keystroke to the one flow that exists to avoid them.
        ParsedInput::Byte(byte @ b'1'..=b'9')
            if app.status_picker.select_row(usize::from(byte - b'0')) =>
        {
            commit(app);
        }
        _ => {}
    }
}

fn commit(app: &mut App) {
    let status = app.status_picker.selected_status();
    let minutes = app.status_picker.selected_minutes();
    let ends_at =
        minutes.map(|minutes| chrono::Utc::now() + chrono::Duration::minutes(i64::from(minutes)));
    app.status_picker.close();
    app.set_status(Some(SessionStatus { status, ends_at }));
    app.banner = Some(Banner::success(&status_set_message(status, minutes)));
}
