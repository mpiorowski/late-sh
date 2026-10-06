//! Keys while the fight scene is open over the street: `1` to `5` play the
//! card in that slot, `e` (or space, or Enter) ends the turn, `a` plays the
//! obvious turn and ends it, `r` runs, Enter leaves a finished scene for
//! the road. Everything else is swallowed so the runner does not walk
//! under the panel; a lone Esc never arrives through `handle_event` (the
//! root's `dispatch_escape` calls `handle_escape`: a run while the fight
//! is on, a close once it is over).
//!
//! Before it, the road (`handle_picker`): up and down move the cursor
//! over the lanes the next step reaches, Enter does the plain thing with
//! what waits there, and each call has its own key: `f` the glyph, `g`
//! the one below it, `b` the bright one, `h` mend at a rest, `c` clear the
//! deck at one, `t` take a cache. A key the node does not answer does
//! nothing. With a draft owed the panel shows the two cards instead of a
//! step, and `1` and `2` take one. With the road over for the day any of those closes it, and
//! `s` copies the day's card. Esc closes it from the root, the same way.

use crate::app::arcade::share::{self, ShareCardKind, ShareFormat};
use crate::app::common::primitives::Banner;
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::share::road_card;
use super::state::{Call, Command, Pick};

/// Keys while the road is open. The row decides what a step meets;
/// `FightSession::call` only keeps a key from asking for something the
/// node under the cursor does not answer.
pub fn handle_picker(app: &mut App, event: &ParsedInput) -> bool {
    if app.fight.picker.is_none() {
        return false;
    }
    let option = match event {
        ParsedInput::Byte(b'1') | ParsedInput::Char('1') => Some(0),
        ParsedInput::Byte(b'2') | ParsedInput::Char('2') => Some(1),
        _ => None,
    };
    if let Some(option) = option
        && app.fight.take_card(option)
    {
        return true;
    }
    let call = match event {
        ParsedInput::Byte(b'f') | ParsedInput::Char('f') => Some(Call::Fight(Pick::Fair)),
        ParsedInput::Byte(b'g') | ParsedInput::Char('g') => Some(Call::Fight(Pick::Lower)),
        ParsedInput::Byte(b'b') | ParsedInput::Char('b') => Some(Call::Fight(Pick::Bright)),
        ParsedInput::Byte(b'h') | ParsedInput::Char('h') => Some(Call::Mend),
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => Some(Call::Clear),
        ParsedInput::Byte(b't') | ParsedInput::Char('t') => Some(Call::Take),
        _ => None,
    };
    if let Some(call) = call {
        app.fight.call(call);
        return true;
    }
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.fight.enter();
            true
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => {
            app.fight.pick_up();
            true
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => {
            app.fight.pick_down();
            true
        }
        ParsedInput::Byte(b's') | ParsedInput::Char('s') => {
            share_road(app);
            true
        }
        // Digits, Tab, `q` stay global, as in the scene.
        ParsedInput::Byte(b'0'..=b'9')
        | ParsedInput::Byte(b'\t')
        | ParsedInput::Byte(b'q')
        | ParsedInput::Byte(b'?') => false,
        _ => true,
    }
}

/// Copy the day's road card, once the road is over for the day. The one
/// place the card leaves the session, so the banner and the metric live
/// here (the arcade's cards do the same in `arcade/input.rs`).
fn share_road(app: &mut App) {
    let Some(sheet) = &app.fight.sheet else {
        return;
    };
    let Some(card) = road_card(sheet) else {
        return;
    };
    app.pending_clipboard = Some(share::render(&card, ShareFormat::Emoji));
    app.banner = Some(Banner::success("Card copied. Paste it anywhere."));
    crate::metrics::record_share_card(ShareCardKind::Road);
}

/// The hand slot a card key names: `1` is the first.
fn slot_of(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(digit @ b'1'..=b'5') => Some(digit - b'1'),
        ParsedInput::Char(digit @ '1'..='5') => Some(*digit as u8 - b'1'),
        _ => None,
    }
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let Some(scene) = &app.fight.scene else {
        return false;
    };
    let over = scene.over;
    let live = !scene.over && !scene.waiting;
    // The cards take their digits while the fight is on: a page switch
    // under a live hand is a misplay, never a wish.
    if let Some(slot) = slot_of(event)
        && !over
    {
        if live {
            app.fight.request(Command::Play { slot });
        }
        return true;
    }
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            match (over, live) {
                (true, _) => app.fight.leave_scene(),
                (false, true) => {
                    app.fight.request(Command::EndTurn);
                }
                (false, false) => {}
            }
            true
        }
        ParsedInput::Byte(b'e' | b' ') | ParsedInput::Char('e' | ' ') if live => {
            app.fight.request(Command::EndTurn);
            true
        }
        ParsedInput::Byte(b'a') | ParsedInput::Char('a') if live => {
            app.fight.request(Command::Auto);
            true
        }
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') if live => {
            app.fight.request(Command::Run);
            true
        }
        // The other digits, Tab, `q` stay global (`?` is the guide's,
        // taken in `city/input.rs` before the scene sees it); the rest is
        // the scene's.
        ParsedInput::Byte(b'0'..=b'9')
        | ParsedInput::Byte(b'\t')
        | ParsedInput::Byte(b'q')
        | ParsedInput::Byte(b'?') => false,
        _ => true,
    }
}

/// Esc over the scene. There is no stepping out of a fight: while it is on,
/// Esc is the run, through whatever the glyph meant to do this turn; a
/// finished scene closes; an answer already pending is left to land. A
/// scene whose last answer was the service failing closes too, the fight
/// left on the row: an outage is not a fight to be trapped in.
pub fn handle_escape(app: &mut App) {
    let Some(scene) = &app.fight.scene else {
        return;
    };
    match (scene.over, scene.waiting, scene.failed) {
        (true, _, _) => app.fight.close(),
        (false, true, _) => {}
        (false, false, true) => app.fight.close(),
        (false, false, false) => {
            app.fight.request(Command::Run);
        }
    }
}
