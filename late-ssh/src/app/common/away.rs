//! Automatic away: a user reads as away once every session they have open
//! has been quiet for [`AWAY_AFTER`], or was sent away by hand (`/brb` in the
//! TUI, `AWAY :msg` over IRC). There is nothing to set and nothing to clear:
//! any input into a TUI session brings it back.
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

/// Whether one TUI session is away: sent away by hand, or quiet for
/// [`AWAY_AFTER`]. `idle` is how long ago its keyboard last moved.
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
