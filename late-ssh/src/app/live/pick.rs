//! Which source the live strip shows, and for how long. Pure rules over the
//! stamps every session and every replica shares, so two sessions looking at
//! the same room see the same thing in the same order.
//!
//! One queue and one overlay:
//! - **The queue.** Everything waits in one queue, first come first served.
//!   Whatever is up stays at least its minimum (`min_for`); after that it
//!   hands over as soon as anything is waiting, and with nothing waiting it
//!   comes down `LIVE_MAX_UP` after it went up. Anything that waits longer
//!   than `LIVE_MAX_WAIT` is dropped.
//! - **The aim.** While a pool shooter moves the cue, that table is drawn
//!   over whatever the queue has up. The queue's clock keeps running
//!   underneath.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::app::door::spectate::state::LiveGameKey;

/// What the live strip and the sidebar's Live panel can point at. Closed:
/// a new source has to say its minimum (`min_for`), what it looks like
/// (`ui.rs`), and what the keys do to it (`input.rs`) before it builds. The
/// order of the variants only breaks a tie between two stamps of the same
/// instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LiveSource {
    /// A daily match in play, by match id.
    DailyMatch(Uuid),
    /// A daily match that just ended, by match id: its final board and the
    /// result.
    DailyResult(Uuid),
    /// A track somebody put in the YouTube booth queue, by queue item id.
    BoothTrack(Uuid),
    /// A link somebody shared to News, by article id: a Live panel row only.
    /// The strip never queues one, since a share is its card in #lounge.
    NewsArticle(Uuid),
    /// A stream that went live, by the streamer's user id (one stream per
    /// user).
    Stream(Uuid),
    /// A door game somebody started that others can watch (DCSS, NetHack or
    /// Brogue), by its door and the player's handle (one live game per
    /// player per door).
    DoorGame(LiveGameKey),
    /// An event on the board an hour before it starts, and again when it
    /// starts, by event id. The viewer's own personal events ride it too,
    /// for the viewer alone.
    BoardEvent(Uuid),
    /// The house's standing world, by game id: its last moment (a rise, an
    /// arrival, a take, a fall, the end), stamped by the write that made it.
    Realm(Uuid),
}

/// An aim older than this is a player who stopped, not one lining up.
pub const LIVE_AIM_WINDOW: Duration = Duration::from_secs(8);
/// A match move or result stays up at least this long.
pub const LIVE_MATCH_MIN: Duration = Duration::from_secs(60);
/// A booth track stays up at least this long: long enough to hear what it
/// is and tune in.
pub const LIVE_TRACK_MIN: Duration = Duration::from_secs(2 * 60);
/// A stream that just went live stays up at least this long, the same as a
/// booth track: long enough to see who it is and open the watch page.
pub const LIVE_STREAM_MIN: Duration = Duration::from_secs(2 * 60);
/// A door game that just started stays up at least this long, the same as a
/// stream: long enough to see who is playing and hop in to watch.
pub const LIVE_DOOR_GAME_MIN: Duration = Duration::from_secs(2 * 60);
/// A board event stays up at least this long, the same as a stream: long
/// enough to read what it is and press the key.
pub const LIVE_BOARD_EVENT_MIN: Duration = Duration::from_secs(2 * 60);
/// With nothing waiting, whatever is up comes down this long after it went
/// up: the strip is for what just happened.
pub const LIVE_MAX_UP: Duration = Duration::from_secs(5 * 60);
/// Anything that waited this long in the queue is stale and is dropped,
/// so a busy evening never builds a backlog the strip cannot catch up on.
pub const LIVE_MAX_WAIT: Duration = Duration::from_secs(10 * 60);
/// The oldest stamp the strip can still show: one that waited its longest,
/// then stayed up its longest. A source that keeps a candidate alive after
/// the thing itself is gone (a daily result) keeps it this long.
pub const LIVE_STAMP_HORIZON: Duration =
    Duration::from_secs(LIVE_MAX_WAIT.as_secs() + LIVE_MAX_UP.as_secs());

fn chrono_of(duration: Duration) -> chrono::Duration {
    chrono::Duration::from_std(duration).expect("strip clocks fit chrono")
}

/// How long a source stays up at least, once it goes up.
fn min_for(source: LiveSource) -> chrono::Duration {
    chrono_of(match source {
        LiveSource::DailyMatch(_) | LiveSource::DailyResult(_) | LiveSource::Realm(_) => {
            LIVE_MATCH_MIN
        }
        LiveSource::BoothTrack(_) => LIVE_TRACK_MIN,
        LiveSource::Stream(_) => LIVE_STREAM_MIN,
        LiveSource::DoorGame(_) => LIVE_DOOR_GAME_MIN,
        LiveSource::BoardEvent(_) => LIVE_BOARD_EVENT_MIN,
        LiveSource::NewsArticle(_) => unreachable!("a News share never queues on the strip"),
    })
}

/// One thing the strip could show.
#[derive(Clone, Copy, Debug)]
pub struct LiveCandidate {
    pub source: LiveSource,
    /// When it joined the queue: a match row's `updated` (the claim, every
    /// move), when a match finished, when a track was queued, when a stream
    /// went live, when a door game started. A match that moves again joins
    /// again at the back.
    pub updated: DateTime<Utc>,
    /// When somebody last acted on it right now, if they have: a pool
    /// shooter moving their cue, the one source of it today.
    pub aimed_at: Option<Instant>,
}

/// What the queue has up, and since when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Featured {
    pub source: LiveSource,
    pub since: DateTime<Utc>,
}

/// What the queue has up now (`replay`), or `None` when the strip is down.
///
/// `current` is what this session had up. It keeps it for its minimum from
/// when this session put it up, as long as it is still a candidate: the
/// replay reads only each candidate's latest stamp, so a match that moves
/// again rewrites the history, and the session holds on rather than cut a
/// minute short. Past that it follows the replay, which is also where a
/// session with nothing up starts, so one that connects mid-queue sees what
/// the room sees.
pub fn pick_queued(
    current: Option<Featured>,
    candidates: &[LiveCandidate],
    now_utc: DateTime<Utc>,
) -> Option<Featured> {
    if let Some(current) = current
        && now_utc < current.since + min_for(current.source)
        && candidates
            .iter()
            .any(|candidate| candidate.source == current.source)
    {
        return Some(current);
    }
    let (source, shown_at) = replay(candidates, now_utc)?;
    let since = match current {
        Some(current) if current.source == source => current.since,
        Some(_) => now_utc,
        None => shown_at,
    };
    Some(Featured { source, since })
}

/// The table somebody is lining up a shot on, drawn over what the queue has
/// up, the freshest aim if several.
pub fn aim_overlay(candidates: &[LiveCandidate], now: Instant) -> Option<LiveSource> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate
                .aimed_at
                .is_some_and(|at| now.saturating_duration_since(at) < LIVE_AIM_WINDOW)
        })
        .max_by_key(|candidate| candidate.aimed_at)
        .map(|candidate| candidate.source)
}

/// Run the queue from the stamps up to `now_utc`, from data every session
/// and every replica shares. Returns what is up and when it went up, or
/// `None` when the strip is down.
///
/// Walks from one moment that matters to the next: a stamp joining the
/// queue, the end of the up source's minimum, the end of its `LIVE_MAX_UP`.
/// At each, waiting entries past `LIVE_MAX_WAIT` are dropped; when the up
/// source may hand over (its minimum is done, or nothing is up) the oldest
/// waiting entry goes up; with nothing waiting it stays until
/// `LIVE_MAX_UP`, then the strip comes down.
fn replay(
    candidates: &[LiveCandidate],
    now_utc: DateTime<Utc>,
) -> Option<(LiveSource, DateTime<Utc>)> {
    let max_up = chrono_of(LIVE_MAX_UP);
    let max_wait = chrono_of(LIVE_MAX_WAIT);
    let mut arrivals: Vec<&LiveCandidate> = candidates.iter().collect();
    // The source breaks a tie between two stamps of the same instant, so
    // the order is the same on every session.
    arrivals.sort_by_key(|candidate| (candidate.updated, candidate.source));
    let mut arrivals = arrivals.into_iter().peekable();
    let mut waiting: Vec<&LiveCandidate> = Vec::new();
    let mut up: Option<(LiveSource, DateTime<Utc>)> = None;
    let mut at = arrivals.peek()?.updated;

    while at <= now_utc {
        while let Some(arrival) = arrivals.next_if(|arrival| arrival.updated <= at) {
            waiting.push(arrival);
        }
        waiting.retain(|entry| at.signed_duration_since(entry.updated) < max_wait);

        let may_hand_over = match up {
            Some((source, since)) => at >= since + min_for(source),
            None => true,
        };
        if may_hand_over && !waiting.is_empty() {
            up = Some((waiting.remove(0).source, at));
        } else if let Some((_, since)) = up
            && waiting.is_empty()
            && at >= since + max_up
        {
            up = None;
        }

        // The next moment anything can change.
        let next_arrival = arrivals.peek().map(|arrival| arrival.updated);
        let next = match up {
            Some((source, since)) => {
                let min_end = since + min_for(source);
                let max_end = since + max_up;
                if at < min_end {
                    min_end
                } else {
                    match next_arrival {
                        Some(arrival) if arrival < max_end => arrival,
                        Some(_) | None => max_end,
                    }
                }
            }
            None => match next_arrival {
                Some(arrival) => arrival,
                None => break,
            },
        };
        assert!(next > at, "the live strip replay always moves forward");
        at = next;
    }
    up
}

#[cfg(test)]
#[path = "pick_test.rs"]
mod pick_test;
