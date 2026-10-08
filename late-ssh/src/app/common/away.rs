//! Automatic away: a user reads as away once every session they have open
//! has been quiet for [`AWAY_AFTER`], or was sent away by hand (`/brb` in the
//! TUI, `AWAY :msg` over IRC). There is nothing to set and nothing to clear:
//! any input from a person into a TUI session brings it back. What is not a
//! person is told apart by [`presence_input`]: with any-event mouse
//! tracking on, the pointer merely crossing the terminal reports, and so
//! does the terminal gaining or losing focus; neither holds off the clock
//! or brings a session back. A report cut in two by a chunk boundary is
//! read whole, once the next chunk completes it (`App::presence_held`).
//!
//! While a session is away its screensaver covers the screen (Settings,
//! Tweaks, `Screensaver`, off unless chosen; `App::screensaver`), and the
//! one thing a person did that brings it back is swallowed rather than
//! acted on, a whole paste included; what they typed after it is theirs.
//!
//! The flag lives on each [`ActiveSession`] in the active-users roster, the
//! one map every surface already reads for "online". Readers resolve the
//! whole roster into one set of away user ids on the 1Hz presence edge
//! (`tick.rs`), so the chat author line, the sidebar friends row, the Zen
//! friends tile, the DM list, `/active`, and @-completion all agree.
//!
//! The per-session AFK line (`chat/state.rs`, `AFK_LINE_IDLE`) is a different
//! thing on the same clock: it marks where *you* stopped reading, and only
//! you see it. Away is what everyone else sees about you.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use late_core::MutexRecover;
use uuid::Uuid;

use crate::state::{ActiveUser, ActiveUsers};

/// How long a TUI session's keyboard stays quiet before it reads as away.
pub const AWAY_AFTER: Duration = Duration::from_secs(30 * 60);

/// The one away badge, painted with no word beside it. Checked against the
/// shop catalog in `away_test.rs`: nobody may be able to buy it, or a buyer
/// would look away while they are here.
pub const AWAY_GLYPH: &str = "💤";

/// What a chunk of terminal input says about who is at the terminal, and
/// how far into it the first thing a person did reaches. Anything but the
/// reports a terminal sends on its own is a person. Those are a bare mouse
/// move (SGR `ESC [ < Cb ; Cx ; Cy M` with the motion bit and no button
/// held) and a focus report (`ESC [ I`, `ESC [ O`). Keys, clicks, drags,
/// scrolls and pastes are a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenceInput {
    /// Nothing a person did: an empty chunk, or unattended reports alone.
    Nobody,
    /// A person. The first thing they did ends at `event_end` (exclusive);
    /// what follows is theirs too. `paste_open` says that first thing is a
    /// bracketed paste the chunk does not close, so it runs on into the
    /// chunks after it (`paste_end` finds the close).
    Person { event_end: usize, paste_open: bool },
    /// Unattended reports, if any, then a report the chunk cuts off at
    /// `held_from`. Who sent it is decided by the rest of it, in the next
    /// chunk: a mouse move split in two is still nobody, and its second
    /// half must not read as a person's keys.
    Partial { held_from: usize },
}

pub fn presence_input(data: &[u8]) -> PresenceInput {
    let mut at = 0;
    while at < data.len() {
        match unattended_report_len(&data[at..]) {
            Some(len) => at += len,
            None => {
                return match event_len(&data[at..]) {
                    Event::Complete(len) => PresenceInput::Person {
                        event_end: at + len,
                        paste_open: false,
                    },
                    Event::PasteOpen => PresenceInput::Person {
                        event_end: data.len(),
                        paste_open: true,
                    },
                    Event::CutOff => PresenceInput::Partial { held_from: at },
                };
            }
        }
    }
    PresenceInput::Nobody
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// Where a bracketed paste closes in `data`: just past its `ESC [ 2 0 1 ~`.
pub fn paste_end(data: &[u8]) -> Option<usize> {
    data.windows(PASTE_END.len())
        .position(|window| window == PASTE_END)
        .map(|at| at + PASTE_END.len())
}

/// The length of the report `data` starts with, when that report is one a
/// terminal sends with nobody at it.
fn unattended_report_len(data: &[u8]) -> Option<usize> {
    let body = data.strip_prefix(b"\x1b[")?;
    match body.first()? {
        b'I' | b'O' => Some(3),
        b'<' => {
            let end = body.iter().position(|b| matches!(b, b'M' | b'm'))?;
            let params = std::str::from_utf8(&body[1..end]).ok()?;
            let code: u16 = params.split(';').next()?.parse().ok()?;
            let bare_move = code & 32 != 0 && code & 3 == 3 && code & 64 == 0;
            bare_move.then_some(2 + end + 1)
        }
        _ => None,
    }
}

/// The event at the start of a non-empty chunk.
enum Event {
    /// A whole event, this many bytes.
    Complete(usize),
    /// A bracketed paste the chunk opens and does not close.
    PasteOpen,
    /// An escape sequence the chunk ends in the middle of.
    CutOff,
}

fn event_len(data: &[u8]) -> Event {
    if data.starts_with(PASTE_START) {
        return match paste_end(data) {
            Some(end) => Event::Complete(end),
            None => Event::PasteOpen,
        };
    }
    let Some(body) = data.strip_prefix(b"\x1b") else {
        // A plain byte, or a UTF-8 character.
        return Event::Complete(utf8_len(data[0]).min(data.len()));
    };
    match body.first() {
        // ESC alone is the Esc key.
        None => Event::Complete(1),
        // CSI: parameters and intermediates, then one final byte.
        Some(b'[') => match body[1..].iter().position(|b| (0x40..=0x7E).contains(b)) {
            Some(at) => Event::Complete(2 + at + 1),
            None => Event::CutOff,
        },
        // SS3: one byte.
        Some(b'O') => match body.len() >= 2 {
            true => Event::Complete(3),
            false => Event::CutOff,
        },
        // OSC and DCS (a terminal's reply to a query): to BEL or ST.
        Some(b']' | b'P') => {
            let bel = body.iter().position(|b| *b == 0x07).map(|at| at + 1);
            let st = body.windows(2).position(|w| w == b"\x1b\\").map(|at| at + 2);
            match (bel, st) {
                (Some(a), Some(b)) => Event::Complete(1 + a.min(b)),
                (Some(a), None) | (None, Some(a)) => Event::Complete(1 + a),
                (None, None) => Event::CutOff,
            }
        }
        // Alt and a key.
        Some(_) => Event::Complete(2),
    }
}

/// How many bytes the UTF-8 character starting with `lead` takes.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F | 0x80..=0xBF => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xFF => 4,
    }
}

/// Whether one TUI session is away: sent away by hand, or quiet for
/// [`AWAY_AFTER`]. `idle` is how long ago a person last gave it input
/// ([`presence_input`]).
pub fn session_is_away(idle: Duration, sent_away: bool) -> bool {
    sent_away || idle >= AWAY_AFTER
}

/// Whether a user reads as away: at least one session, and every one of them
/// away. A user with no sessions (the always-on ghost bots) is never away, and
/// one live terminal keeps a user here however many others sit idle.
pub fn user_is_away(user: &ActiveUser) -> bool {
    !user.sessions.is_empty() && user.sessions.iter().all(|session| session.away)
}

/// Every away user in a roster snapshot. The one resolution every surface
/// reads, taken under the lock the presence edge already holds.
pub fn away_user_ids(users: &HashMap<Uuid, ActiveUser>) -> HashSet<Uuid> {
    users
        .iter()
        .filter(|(_, user)| user_is_away(user))
        .map(|(user_id, _)| *user_id)
        .collect()
}

/// Write one session's away flag onto the roster. A session that is no
/// longer listed (it disconnected between the read and the write) is left
/// alone: there is nothing left to mark.
pub fn set_session_away(active_users: &ActiveUsers, user_id: Uuid, token: &str, away: bool) {
    let mut guard = active_users.lock_recover();
    let Some(user) = guard.get_mut(&user_id) else {
        return;
    };
    if let Some(session) = user
        .sessions
        .iter_mut()
        .find(|session| session.token == token)
    {
        session.away = away;
    }
}

#[cfg(test)]
#[path = "away_test.rs"]
mod away_test;
