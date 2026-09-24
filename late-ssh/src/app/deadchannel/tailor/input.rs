//! Keys while the tailor's panel is open: up and down pick the row, left
//! and right walk its rack, `t` the tint, `r` shuffles, `s` wears the
//! draft, Enter leaves. Everything else is swallowed so the runner does
//! not walk under the mirror; a lone Esc never arrives here (the root's
//! `dispatch_escape` closes the panel and the session).

use crate::app::input::ParsedInput;
use crate::app::state::App;

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.city.dismiss();
            app.tailor.close();
            return true;
        }
        // Digits, Tab, `q`, `?` stay global.
        ParsedInput::Byte(b'0'..=b'9')
        | ParsedInput::Byte(b'\t')
        | ParsedInput::Byte(b'q')
        | ParsedInput::Byte(b'?') => return false,
        _ => {}
    }
    let Some(draft) = &mut app.tailor.draft else {
        return true;
    };
    match event {
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => draft.up(),
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => draft.down(),
        ParsedInput::Arrow(b'C') | ParsedInput::Byte(b'l') | ParsedInput::Char('l') => draft.next(),
        ParsedInput::Arrow(b'D') | ParsedInput::Byte(b'h') | ParsedInput::Char('h') => draft.prev(),
        ParsedInput::Byte(b't') | ParsedInput::Char('t') => draft.tint(),
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') => draft.shuffle(&mut rand::thread_rng()),
        ParsedInput::Byte(b's') | ParsedInput::Char('s') => app.tailor.wear(),
        _ => {}
    }
    true
}
