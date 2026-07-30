//! The `/pomodoro` focus countdown: this session's own timer, and the
//! process-shared directory of who else is currently in one.
//!
//! Distribution copies the `username_effect::NameFlairDirectory` snapshot-swap
//! shape: entries change rarely (a start, a stop, an expiry), so readers clone
//! an `Arc` once a second rather than copying a map under a mutex. Two ways
//! this one is simpler than flair: timers are session-local, so there is no DB
//! table, no migration, and no startup seed; and expiry is read-time only, so
//! nothing has to fire on a schedule to retire an entry.
//!
//! The directory stores `ends_at` and nothing else. Peers see a countdown, and
//! the user's own label is deliberately unrepresentable here rather than
//! filtered out at each read: the label is free text that only ever reaches
//! its author's own terminal, so keeping it out of shared state means it
//! cannot become someone else's moderation problem.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use late_core::MutexRecover;
use uuid::Uuid;

/// A running `/pomodoro` countdown. Deliberately not a Work/Break state
/// machine: `label` is whatever the user typed, and nothing auto-advances to
/// a next phase.
#[derive(Clone, Debug)]
pub(crate) struct PomodoroTimer {
    pub(crate) label: String,
    pub(crate) ends_at: DateTime<Utc>,
}

impl PomodoroTimer {
    /// Whole seconds left, rounded up so a freshly started 25 minute timer
    /// reads `25:00` instead of `24:59`, and floored at zero for the frame
    /// between expiry and the 1Hz edge in `tick.rs` that clears the timer.
    pub(crate) fn remaining_secs(&self, now: DateTime<Utc>) -> u64 {
        remaining_secs(self.ends_at, now)
    }

    /// The owner's status HUD badge: `MM:SS label`. Minutes are not wrapped
    /// into hours because the command caps a timer well under the point where
    /// three-digit minutes would be confusing.
    pub(crate) fn badge(&self, now: DateTime<Utc>) -> String {
        let remaining = self.remaining_secs(now);
        format!("{:02}:{:02} {}", remaining / 60, remaining % 60, self.label)
    }
}

fn remaining_secs(ends_at: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    let millis = (ends_at - now).num_milliseconds().max(0) as u64;
    millis.div_ceil(1_000)
}

/// Snapshot-swap directory of running countdowns, shared process-wide. Values
/// are the end instant only: see the module note on why the label is absent.
pub type PomodoroDirectory = Arc<Mutex<Arc<HashMap<Uuid, DateTime<Utc>>>>>;

pub fn new_directory() -> PomodoroDirectory {
    Arc::new(Mutex::new(Arc::new(HashMap::new())))
}

/// Cheap read: clones the inner `Arc`, never the map.
pub fn snapshot(directory: &PomodoroDirectory) -> Arc<HashMap<Uuid, DateTime<Utc>>> {
    Arc::clone(&directory.lock_recover())
}

/// Publish or clear one user's countdown. Called from every place that changes
/// `App::pomodoro` (the `/pomodoro` command, the tick that expires it) and
/// from session teardown, so a disconnect doesn't leave a peer showing as
/// focusing until their original end time.
pub(crate) fn set_user(
    directory: &PomodoroDirectory,
    user_id: Uuid,
    timer: Option<&PomodoroTimer>,
) {
    let mut guard = directory.lock_recover();
    let entries = Arc::make_mut(&mut *guard);
    match timer {
        Some(timer) => {
            entries.insert(user_id, timer.ends_at);
        }
        None => {
            entries.remove(&user_id);
        }
    }
}

/// The peer-facing badge for the chat author line: whole minutes remaining,
/// rounded up, so a 25 minute block reads `🍅25m` on its first frame and
/// `🍅1m` through its final minute. `None` once the entry is stale, which is
/// how expiry is retired without a sweeper.
pub(crate) fn peer_badge(ends_at: DateTime<Utc>, now: DateTime<Utc>) -> Option<String> {
    let minutes = remaining_secs(ends_at, now).div_ceil(60);
    (minutes > 0).then(|| format!("🍅{minutes}m"))
}

/// Resolve a directory snapshot into the per-peer badge strings a chat frame
/// paints. Own-session badges are included: seeing your own countdown next to
/// your name is the same information the top border already shows.
pub(crate) fn resolve_all(
    entries: &HashMap<Uuid, DateTime<Utc>>,
    now: DateTime<Utc>,
) -> HashMap<Uuid, String> {
    entries
        .iter()
        .filter_map(|(user_id, ends_at)| peer_badge(*ends_at, now).map(|badge| (*user_id, badge)))
        .collect()
}

#[cfg(test)]
#[path = "pomodoro_test.rs"]
mod pomodoro_test;
