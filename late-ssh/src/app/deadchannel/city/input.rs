//! City input: roguelike walking (arrows/hjkl, Shift+arrow or HJKL to
//! run), Enter at a landmark (a shop panel, a street line, the road at
//! the screen, or the wire out), `f` to walk up to the static and `p` to
//! open patch from anywhere on the street, Enter to close a panel. The
//! armorer's panel takes the till keys (`fight/state.rs`,
//! `Command::Outfit`), the lockers `d` and `w`, the bits machine `b` and
//! `r`, Dead Air `s`, `d`, and `t` (a glass each), the blade shop `w` and
//! `a`, the ledge `r` twice to step off; the tailor's hands every key to
//! `tailor/input.rs`.
//! While the guide is open every key goes to `guide/input.rs` first, and
//! `?` anywhere on the page opens it (the site guide's key, taken over
//! down here: the street has its own). While the road or the fight scene
//! is open every key goes to `fight/input.rs`. Returns `false` for anything it
//! does not own so global keys (page digits, Tab, `q`) keep working. While a panel is open, or the
//! runner is looking over the ledge, every other typed key is swallowed:
//! the walk keys, so the runner does not wander under the box, and the
//! letters a global would spend (`w`, `m`, `v`). A lone Esc never arrives here:
//! the root flushes it to `dispatch_escape`, which calls
//! `State::dismiss` for this screen.

use crate::app::common::primitives::Screen;
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::data;
use super::map::Landmark;
use super::state::Enter;
use crate::app::deadchannel::fight::state::{Command, Drink, Slot};

/// Go down into the city: `0` on the Lounge, or Enter on Night City's card
/// in the Games hub. Callers hold the runner gate (`App::is_runner`). A
/// descent always lands on the street: a panel or the ledge left open on
/// the way up does not carry over.
pub fn descend(app: &mut App) {
    app.city.dismiss();
    app.fight.close();
    app.tailor.close();
    app.guide.state.close();
    // The descent is a touch: the sheet re-reads (and the day rolls if it
    // turned) before the strip shows it.
    app.fight.reload();
    // The first descent opens the guide by itself, once per runner
    // (`app/deadchannel/guide`).
    app.guide.descend();
    // On the shared street from here until the session ends
    // (`deadchannel/street`).
    app.street.descend();
    app.set_screen(Screen::City);
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.guide.state.is_open() {
        return crate::app::deadchannel::guide::input::handle_event(app, event);
    }
    if crate::app::deadchannel::guide::input::opens(event) {
        app.guide.state.open();
        return true;
    }
    if app.fight.scene_open() {
        return crate::app::deadchannel::fight::input::handle_event(app, event);
    }
    if app.fight.picker_open() {
        return crate::app::deadchannel::fight::input::handle_picker(app, event);
    }
    if app.city.panel().is_some() || app.city.at_ledge() {
        return handle_panel(app, event);
    }

    if let Some(byte) = event_byte(event)
        && matches!(byte, b'\r' | b'\n')
    {
        let Some(landmark) = app.city.nearby() else {
            return true;
        };
        match landmark.on_enter() {
            Enter::Panel(landmark) => {
                app.city.open_panel(landmark);
                app.fight.clear_till();
                if landmark == Landmark::Tailor {
                    let runner = app.runner_looks.get(&app.user_id).copied();
                    app.tailor.open(runner);
                }
            }
            Enter::Line(landmark) => {
                let pool = data::lines(landmark);
                let index = (app.city.anim_tick / 7) as usize % pool.len().max(1);
                app.city.say(landmark, index);
            }
            Enter::Leave => app.set_screen(Screen::Clubhouse),
            Enter::Ledge => {
                app.city.look_over();
                app.fight.clear_till();
            }
            Enter::Fight => app.fight.step_up(),
        }
        return true;
    }

    // `f` walks up to the static from anywhere on the street, not only at
    // one of its three screens. Same path as Enter there: the road, or a
    // waiting fight straight back in.
    if let Some(b'f' | b'F') = event_byte(event) {
        app.music_prefix_armed = false;
        app.fight.step_up();
        return true;
    }

    // `p` opens patch from anywhere on the street, the same panel Enter at
    // the counter opens: the price and the refusal show first, `p` again
    // pays. The armorer and the tailor stay a walk away on purpose.
    if let Some(b'p' | b'P') = event_byte(event) {
        app.music_prefix_armed = false;
        app.city.open_panel(Landmark::Repairs);
        app.fight.clear_till();
        return true;
    }

    handle_walk(app, event)
}

/// A panel, or the ledge view, takes Enter to close, and eats the walk
/// keys. At the armorer the up and down keys walk the wall instead, `w`
/// buys the picked weapon and `a` the picked armor. Digits, Tab, and `q`
/// fall through to the globals; any other typed key is eaten.
fn handle_panel(app: &mut App, event: &ParsedInput) -> bool {
    if app.city.panel() == Some(Landmark::Tailor) {
        return crate::app::deadchannel::tailor::input::handle_event(app, event);
    }
    if app.city.panel() == Some(Landmark::Armorer) {
        match event {
            ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => {
                app.city.pick_up();
                return true;
            }
            ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => {
                app.city.pick_down();
                return true;
            }
            ParsedInput::Byte(b'w') | ParsedInput::Char('w') => {
                app.fight.request(Command::Outfit {
                    slot: Slot::Weapon,
                    tier: app.city.picked_tier(),
                });
                return true;
            }
            ParsedInput::Byte(b'a') | ParsedInput::Char('a') => {
                app.fight.request(Command::Outfit {
                    slot: Slot::Armor,
                    tier: app.city.picked_tier(),
                });
                return true;
            }
            _ => {}
        }
    }
    if app.city.panel() == Some(Landmark::Repairs)
        && let ParsedInput::Byte(b'p') | ParsedInput::Char('p') = event
    {
        app.fight.request(Command::Patch);
        return true;
    }
    if app.city.panel() == Some(Landmark::Lockers) {
        match event {
            ParsedInput::Byte(b'd') | ParsedInput::Char('d') => {
                app.fight.request(Command::Deposit);
                return true;
            }
            ParsedInput::Byte(b'w') | ParsedInput::Char('w') => {
                app.fight.request(Command::Withdraw);
                return true;
            }
            _ => {}
        }
    }
    if app.city.panel() == Some(Landmark::Bits) {
        match event {
            ParsedInput::Byte(b'b') | ParsedInput::Char('b') => {
                app.fight.request(Command::Borrow);
                return true;
            }
            ParsedInput::Byte(b'r') | ParsedInput::Char('r') => {
                app.fight.request(Command::Repay);
                return true;
            }
            _ => {}
        }
    }
    if app.city.panel() == Some(Landmark::Bar) {
        let drink = match event {
            ParsedInput::Byte(b's') | ParsedInput::Char('s') => Some(Drink::StaticOnIce),
            ParsedInput::Byte(b'd') | ParsedInput::Char('d') => Some(Drink::DeadAirNeat),
            ParsedInput::Byte(b't') | ParsedInput::Char('t') => Some(Drink::TestPattern),
            _ => None,
        };
        if let Some(drink) = drink {
            app.fight.request(Command::Drink { drink });
            return true;
        }
    }
    if app.city.panel() == Some(Landmark::Blades) {
        let slot = match event {
            ParsedInput::Byte(b'w') | ParsedInput::Char('w') => Some(Slot::Weapon),
            ParsedInput::Byte(b'a') | ParsedInput::Char('a') => Some(Slot::Armor),
            _ => None,
        };
        if let Some(slot) = slot {
            app.fight.request(Command::Cart { slot });
            return true;
        }
    }
    // The ledge: `r` once to lean out, `r` again to step off. Any other
    // key leans back in first, so the two presses are one deliberate act.
    if app.city.at_ledge() {
        match event {
            ParsedInput::Byte(b'r') | ParsedInput::Char('r') => {
                match app.city.reset_armed() {
                    true => {
                        app.city.disarm_reset();
                        app.fight.request(Command::Reset);
                    }
                    false => {
                        app.fight.clear_till();
                        app.city.arm_reset();
                    }
                }
                return true;
            }
            _ => app.city.disarm_reset(),
        }
    }
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.city.dismiss();
            true
        }
        // Typed keys arrive as `Char`. Digits and `q` stay global; every
        // other one is the panel's, owned or not, so a letter a global
        // spends (`w` Bonsai Care, `m` the mute) does nothing over a shop.
        // Tab and the control chords are bytes and fall through.
        ParsedInput::Char('0'..='9' | 'q' | 'Q') => false,
        ParsedInput::Char(_) => true,
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
