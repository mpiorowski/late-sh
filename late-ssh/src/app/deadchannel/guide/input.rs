//! Keys while the guide is open over the street: `j`/`k` and the arrows
//! scroll a line, PageUp and PageDown a screen, Enter, `q` and `?` close
//! it. Digits and Tab stay global so the page keys keep working under
//! it; everything else is swallowed so the runner does not walk under
//! the box. A lone Esc never arrives here: the root's `dispatch_escape`
//! closes the guide.

use crate::app::input::ParsedInput;
use crate::app::state::App;

/// Lines one PageUp or PageDown moves.
const PAGE: i16 = 10;

/// Whether `event` is the guide's key on the street.
pub fn opens(event: &ParsedInput) -> bool {
    matches!(event, ParsedInput::Byte(b'?') | ParsedInput::Char('?'))
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(b'\r' | b'\n' | b'q' | b'?') | ParsedInput::Char('q' | '?') => {
            app.guide.state.close();
            true
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => {
            app.guide.state.scroll_by(1);
            true
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => {
            app.guide.state.scroll_by(-1);
            true
        }
        ParsedInput::PageDown => {
            app.guide.state.scroll_by(PAGE);
            true
        }
        ParsedInput::PageUp => {
            app.guide.state.scroll_by(-PAGE);
            true
        }
        ParsedInput::Byte(b'0'..=b'9') | ParsedInput::Byte(b'\t') => false,
        _ => true,
    }
}
