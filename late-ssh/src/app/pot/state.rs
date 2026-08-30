//! What one session knows about the pot: a plain projection of the
//! process-shared snapshot, resolved once a second in `tick.rs` so no render
//! ever touches the service or the clock.
//!
//! Pure and comparable on purpose: the tick only marks the frame dirty when
//! the rendered values actually move, so a countdown ticking inside the same
//! minute repaints nothing.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::svc::PotSnapshot;

/// The pot as this session draws it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PotView {
    /// What the tickets have paid in so far.
    pub size: i64,
    pub ticket_count: i64,
    /// How many the viewer holds. Their own count only: the map behind it
    /// never leaves the service.
    pub my_tickets: i64,
    /// `4d12h`, `3h12m`, `12m`, `45s`, or `soon` once the draw hour has passed and the
    /// sweeper has not settled it yet. Already rounded, so an equal string
    /// means an equal frame.
    pub draws_in: String,
    /// False before the first refresh, and in a process with no pot service.
    pub open: bool,
}

impl PotView {
    /// Project the shared snapshot for one viewer.
    pub(crate) fn resolve(snapshot: &Arc<PotSnapshot>, user_id: Uuid, now: DateTime<Utc>) -> Self {
        let Some(draws_at) = snapshot.draws_at else {
            return Self::default();
        };
        Self {
            size: snapshot.size(),
            ticket_count: snapshot.ticket_count,
            my_tickets: snapshot.holding_for(user_id).tickets,
            draws_in: countdown(draws_at, now),
            open: true,
        }
    }
}

/// `4d12h`, `3h12m`, `12m`, `45s`, `soon`: the one countdown format the
/// pot's copy uses, in the HUD badge and the `/pot` line alike.
pub(crate) fn countdown(draws_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    short_duration((draws_at - now).num_seconds())
}

/// The same thing from a plain second count, for callers that already have
/// one. A draw that is due but not yet settled reads `soon` rather than a
/// negative or a zero: the sweeper is on a 60 second tick, so "now" would be
/// a promise it cannot keep.
pub(crate) fn short_duration(secs: i64) -> String {
    if secs <= 0 {
        return "soon".to_string();
    }
    let (days, hours, minutes) = (secs / 86_400, (secs % 86_400) / 3_600, (secs % 3_600) / 60);
    match (days, hours, minutes) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, minutes) => format!("{minutes}m"),
        (0, hours, minutes) => format!("{hours}h{minutes:02}m"),
        // Past a day the minutes are noise; the hours are zero-padded for
        // the same reason the minutes are below a day.
        (days, hours, _) => format!("{days}d{hours:02}h"),
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
