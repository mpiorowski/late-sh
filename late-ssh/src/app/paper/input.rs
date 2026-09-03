//! Keys while The Late Edition is up: scroll, close. Everything else is
//! swallowed, so the paper reads like a modal and not a transparency.

use crate::app::input::ParsedInput;
use crate::app::state::App;

pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'\r' | b'\n' | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.paper.close_modal();
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(1);
            }
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(-1);
            }
        }
        ParsedInput::PageDown => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(10);
            }
        }
        ParsedInput::PageUp => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(-10);
            }
        }
        ParsedInput::Home => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll_offset = 0;
            }
        }
        _ => {}
    }
}
