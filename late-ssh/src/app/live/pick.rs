//! Which source the live strip features, and for how long. Pure rules over
//! the stamps every session and every replica shares, so two sessions looking
//! at the same room see the same thing in the same order.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// What the live strip can feature. Closed: a new source has to say what it
/// looks like (`ui.rs`) and what opening it does (`input.rs`) before it
/// builds. The order of the variants only breaks a tie between two stamps
/// of the same instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LiveSource {
    /// A daily match in play, by match id.
    DailyMatch(Uuid),
    /// A track somebody put in the YouTube booth queue, by queue item id.
    BoothTrack(Uuid),
}

/// An aim older than this is a player who stopped, not one lining up.
pub const LIVE_AIM_WINDOW: Duration = Duration::from_secs(8);
/// A featured source stays up at least this long once it goes up, unless
/// somebody starts aiming elsewhere: newer news waits its turn, so a busy
/// room does not flip the strip before anyone has looked at it.
pub const LIVE_HOLD: Duration = Duration::from_secs(60);
/// The strip stays up this long after the featured source's last write:
/// long enough for a regular glancing back to catch it, short enough that it
/// leaves and can come back. A correspondence match sits active for days
/// between moves, so "active" alone would keep the strip up for good.
pub const LIVE_STRIP_LINGER: Duration = Duration::from_secs(5 * 60);

/// Whether the strip is up for the featured match: its row was
/// written inside `LIVE_STRIP_LINGER`, or its shooter is lining up a shot.
/// `now_utc` is the clock the row's `updated` was stamped with.
pub fn strip_is_fresh(
    updated: DateTime<Utc>,
    aimed_at: Option<Instant>,
    now_utc: DateTime<Utc>,
    now: Instant,
) -> bool {
    let aiming = aimed_at.is_some_and(|at| now.saturating_duration_since(at) < LIVE_AIM_WINDOW);
    let linger = chrono::Duration::from_std(LIVE_STRIP_LINGER).expect("linger fits chrono");
    aiming || now_utc.signed_duration_since(updated) < linger
}


/// One thing the strip could feature.
#[derive(Clone, Copy, Debug)]
pub struct LiveCandidate {
    pub source: LiveSource,
    /// The row's last write: newest is what just happened.
    pub updated: DateTime<Utc>,
    /// When somebody last acted on it right now, if they have: a pool
    /// shooter moving their cue, the one source of it today.
    pub aimed_at: Option<Instant>,
}

/// What the strip features, and when it took the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Featured {
    pub source: LiveSource,
    pub since: DateTime<Utc>,
}

/// Pick what the strip features, from the rows' `updated`
/// stamps, the wall clock, and the match this session is showing.
///
/// A fresh aim wins outright, the freshest if several. Otherwise the match
/// on the strip keeps it for `LIVE_HOLD` from the moment it took it, as long
/// as it is still in the lobby and its last write is inside
/// `LIVE_STRIP_LINGER`. Past that the writes are replayed in order
/// (`replay`), which is also where a session with nothing up starts, so one
/// that connects mid-hold sees what the room sees.
///
/// The replay reads only each row's latest stamp, so a match that moves
/// again loses its old place in it. The hold is what keeps that from
/// flipping the board early: a session follows the replay one match behind
/// rather than cut a minute short.
pub fn pick_featured(
    current: Option<Featured>,
    candidates: &[LiveCandidate],
    now_utc: DateTime<Utc>,
    now: Instant,
) -> Option<Featured> {
    let since_for = |source: LiveSource, fresh_start: DateTime<Utc>| match current {
        Some(current) if current.source == source => current.since,
        Some(_) => now_utc,
        None => fresh_start,
    };
    let aiming = |candidate: &&LiveCandidate| {
        candidate
            .aimed_at
            .is_some_and(|at| now.saturating_duration_since(at) < LIVE_AIM_WINDOW)
    };
    if let Some(candidate) = candidates
        .iter()
        .filter(aiming)
        .max_by_key(|candidate| candidate.aimed_at)
    {
        return Some(Featured {
            source: candidate.source,
            since: since_for(candidate.source, now_utc),
        });
    }
    let hold = chrono::Duration::from_std(LIVE_HOLD).expect("hold fits chrono");
    let linger = chrono::Duration::from_std(LIVE_STRIP_LINGER).expect("linger fits chrono");
    if let Some(current) = current
        && now_utc < current.since + hold
        && candidates.iter().any(|candidate| {
            candidate.source == current.source
                && now_utc.signed_duration_since(candidate.updated) < linger
        })
    {
        return Some(current);
    }
    let (source, shown_at) = replay(candidates, now_utc)?;
    Some(Featured {
        source,
        since: since_for(source, shown_at),
    })
}

/// Replay the writes in order, from data every session and every replica
/// shares: a match that takes the strip keeps it for `LIVE_HOLD`, and the
/// next write in line takes over at its own stamp or the end of that hold,
/// whichever is later. So two moves a minute apart each get their minute, in
/// order. A match whose turn would come after its own `LIVE_STRIP_LINGER`
/// ran out is skipped: the strip could not show it, and its minute would
/// keep a move that just landed waiting behind nothing. Returns the match
/// and when it took the strip.
fn replay(
    candidates: &[LiveCandidate],
    now_utc: DateTime<Utc>,
) -> Option<(LiveSource, DateTime<Utc>)> {
    let hold = chrono::Duration::from_std(LIVE_HOLD).expect("hold fits chrono");
    let linger = chrono::Duration::from_std(LIVE_STRIP_LINGER).expect("linger fits chrono");
    let mut ordered: Vec<&LiveCandidate> = candidates.iter().collect();
    // The source breaks a tie between two rows stamped the same instant, so
    // the order is the same on every session.
    ordered.sort_by_key(|candidate| (candidate.updated, candidate.source));
    let mut ordered = ordered.into_iter();
    let first = ordered.next()?;
    let (mut shown, mut shown_at) = (first.source, first.updated);
    for next in ordered {
        let takeover = next.updated.max(shown_at + hold);
        if takeover > now_utc {
            break;
        }
        if takeover.signed_duration_since(next.updated) >= linger {
            continue;
        }
        shown = next.source;
        shown_at = takeover;
    }
    Some((shown, shown_at))
}


#[cfg(test)]
#[path = "pick_test.rs"]
mod pick_test;
