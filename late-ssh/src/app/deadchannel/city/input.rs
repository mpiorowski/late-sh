//! City input: roguelike walking (arrows/hjkl, Shift+arrow or HJKL to
//! run), Enter at a landmark (a shop panel, a street line, or the wire
//! out), Esc or Enter to close a panel.
//! Returns `false` for anything it does not own so global keys (page
//! digits, Tab, `q`, `?`) keep working. While a panel is open the walk keys
//! are swallowed so the runner does not wander under the box.

use crate::app::common::primitives::Screen;
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::data;
use super::state::Enter;

/// Who may go under: runners, the sessions with a `deadchannel_runners`
/// row (a look in the shared table). Everyone else pressing `0` on the
/// clubhouse stays on the clubhouse.
pub fn allowed(app: &App) -> bool {
    app.runner_looks.contains_key(&app.user_id)
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.city.panel().is_some() {
        return handle_panel(app, event);
    }

    if let Some(byte) = event_byte(event)
        && matches!(byte, b'\r' | b'\n')
    {
        let Some(landmark) = app.city.nearby() else {
            return true;
        };
        match landmark.on_enter() {
            Enter::Panel(landmark) => app.city.open_panel(landmark),
            Enter::Line(landmark) => {
                let pool = data::lines(landmark);
                let index = (app.city.anim_tick / 7) as usize % pool.len().max(1);
                app.city.say(landmark, index);
            }
            Enter::Leave => app.set_screen(Screen::Clubhouse),
        }
        return true;
    }

    handle_walk(app, event)
}

/// A panel takes Esc and Enter to close, and eats the walk keys. Everything
/// else (digits, Tab, `q`) falls through to the globals.
fn handle_panel(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.city.close_panel();
            true
        }
        _ => walk_delta(event).is_some() || run_delta(event).is_some(),
    }
}

/// Arrow keys and lowercase hjkl move the runner one step; Shift+arrow
/// and uppercase HJKL run. Consumes the key even when the step is blocked
/// so walking into a wall never triggers a global.
fn handle_walk(app: &mut App, event: &ParsedInput) -> bool {
    if let Some((dx, dy)) = run_delta(event) {
        app.music_prefix_armed = false;
        app.city.run(dx, dy);
        return true;
    }
    let Some((dx, dy)) = walk_delta(event) else {
        return false;
    };
    app.music_prefix_armed = false;
    app.city.walk(dx, dy);
    true
}

fn run_delta(event: &ParsedInput) -> Option<(i32, i32)> {
    match event {
        ParsedInput::ShiftArrow(b'A') => Some((0, -1)),
        ParsedInput::ShiftArrow(b'B') => Some((0, 1)),
        ParsedInput::ShiftArrow(b'C') => Some((1, 0)),
        ParsedInput::ShiftArrow(b'D') => Some((-1, 0)),
        ParsedInput::Byte(b'K') | ParsedInput::Char('K') => Some((0, -1)),
        ParsedInput::Byte(b'J') | ParsedInput::Char('J') => Some((0, 1)),
        ParsedInput::Byte(b'L') | ParsedInput::Char('L') => Some((1, 0)),
        ParsedInput::Byte(b'H') | ParsedInput::Char('H') => Some((-1, 0)),
        _ => None,
    }
}

fn walk_delta(event: &ParsedInput) -> Option<(i32, i32)> {
    match event {
        ParsedInput::Arrow(b'A') => Some((0, -1)),
        ParsedInput::Arrow(b'B') => Some((0, 1)),
        ParsedInput::Arrow(b'C') => Some((1, 0)),
        ParsedInput::Arrow(b'D') => Some((-1, 0)),
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') => Some((0, -1)),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') => Some((0, 1)),
        ParsedInput::Byte(b'l') | ParsedInput::Char('l') => Some((1, 0)),
        ParsedInput::Byte(b'h') | ParsedInput::Char('h') => Some((-1, 0)),
        _ => None,
    }
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
