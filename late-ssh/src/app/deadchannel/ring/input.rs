//! Keys while the ring's panel is open. With nobody in the ring: up and
//! down walk the marked runners on the wire, `c` calls the one picked,
//! `a` takes a call and `x` ends one (yours or to you), and `1` to `4`
//! flip a draft pick of the deck you bring. With a bout set: left and
//! right pick a corner and `b` puts one press on it (a fighter's goes on
//! their own). For a fighter, from the accept on: `1` to `5` lay that
//! hand card in the next empty slot or take it back, and `e` or space
//! locks the round. Enter leaves. A digit stays the page key it always
//! is but for a fighter from the accept until the ring clears (after the
//! bout ends it does nothing, so a card key pressed as the last round
//! comes out never turns the page) and a marked runner at an empty
//! ring; everything else is swallowed so the runner does not walk under
//! the panel. A lone Esc never arrives here (the root's `dispatch_escape`
//! closes the panel).

use chrono::Utc;

use super::session::{Phase, phase, roster};
use crate::app::input::ParsedInput;
use crate::app::state::App;

/// What one key at the ring asks for.
enum Key {
    Up,
    Down,
    Side,
    Call,
    Accept,
    Decline,
    Bet,
    Lock,
    /// A digit, as the zero-based thing it names.
    Digit(u8),
    Leave,
    Global,
    Unbound,
}

fn key(event: &ParsedInput) -> Key {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => Key::Leave,
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => Key::Up,
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => Key::Down,
        ParsedInput::Arrow(b'C' | b'D')
        | ParsedInput::Byte(b'h' | b'l')
        | ParsedInput::Char('h' | 'l') => Key::Side,
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => Key::Call,
        ParsedInput::Byte(b'a') | ParsedInput::Char('a') => Key::Accept,
        ParsedInput::Byte(b'x') | ParsedInput::Char('x') => Key::Decline,
        ParsedInput::Byte(b'b') | ParsedInput::Char('b') => Key::Bet,
        ParsedInput::Byte(b'e' | b' ') | ParsedInput::Char('e' | ' ') => Key::Lock,
        ParsedInput::Byte(digit @ b'1'..=b'9') => Key::Digit(digit - b'1'),
        ParsedInput::Char(digit @ '1'..='9') => Key::Digit(*digit as u8 - b'1'),
        // `0`, Tab and `q` stay global (`?` is the guide's, taken in
        // `city/input.rs` before the ring sees it).
        ParsedInput::Byte(b'0' | b'\t' | b'q' | b'?') | ParsedInput::Char('0' | 'q') => Key::Global,
        _ => Key::Unbound,
    }
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let now = Utc::now();
    let key = key(event);
    let board = app.ring.board.clone();
    let fighting = app.ring.my_corner().is_some();
    let marked = app
        .runner_looks
        .get(&app.user_id)
        .is_some_and(|entry| entry.marks >= 1);
    match (key, phase(board.live.as_ref(), now)) {
        (Key::Leave, _) => app.city.dismiss(),
        (Key::Global, _) => return false,
        // A digit is a card under a fighter's hand and a deck pick at an
        // empty ring; anywhere else it is the page it always was.
        (Key::Digit(hand), Phase::Set { .. } | Phase::On { .. }) if fighting => {
            app.ring.card(hand);
        }
        // The last round came out under a fighter's fingers: a card key
        // a moment late is still the ring's, not the page it names.
        (Key::Digit(_), Phase::Over { live, .. }) if live.bout.side_of(app.user_id).is_some() => {}
        (Key::Digit(pick), Phase::Empty) if marked => app.ring.flip_pick(usize::from(pick)),
        (Key::Digit(_), _) => return false,
        (Key::Lock, Phase::Set { .. } | Phase::On { .. }) => app.ring.lock(),
        (Key::Bet, Phase::Set { .. }) => app.ring.bet(now),
        (Key::Side, Phase::Set { .. }) => app.ring.swap_corner(),
        (Key::Up, Phase::Empty) => app.ring.cursor_up(),
        (Key::Down, Phase::Empty) => {
            let roster = roster(&app.presence.records, &app.runner_looks, app.user_id);
            app.ring.cursor_down(roster.len());
        }
        (Key::Call, Phase::Empty) => {
            let roster = roster(&app.presence.records, &app.runner_looks, app.user_id);
            app.ring.call(&roster);
        }
        (Key::Accept, Phase::Empty) => app.ring.accept(),
        (Key::Decline, Phase::Empty) => app.ring.decline(),
        (
            Key::Up
            | Key::Down
            | Key::Side
            | Key::Call
            | Key::Accept
            | Key::Decline
            | Key::Bet
            | Key::Lock
            | Key::Unbound,
            _,
        ) => {}
    }
    true
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
