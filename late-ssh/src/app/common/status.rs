//! The `/status` presence state: this session's own status, and the
//! process-shared directory of what everyone else is doing.
//!
//! Distribution copies the `username_effect::NameFlairDirectory`
//! snapshot-swap shape: entries change rarely (a set, a clear, an expiry), so
//! readers clone an `Arc` once a second rather than copying a map under a
//! mutex. Two ways this one is simpler than flair: statuses are
//! session-local, so there is no DB table, no migration, and no startup seed;
//! and expiry is read-time only, so nothing has to fire on a schedule to
//! retire an entry.
//!
//! [`Status`] is closed on purpose. It used to be free text, which made every
//! author label a place to write anything at all; a rented title costs chips
//! and goes through an AI screen for exactly that reason, and a status that
//! costs nothing cannot carry the same risk. A new variant is a code change
//! and must not collide with a shop badge glyph, which `status_test.rs`
//! pins.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use late_core::MutexRecover;
use uuid::Uuid;

/// What a session is doing, as a closed set. The glyphs are disjoint from
/// every purchasable badge, every special role badge, the bonsai ladder, and
/// the crown, so a status can never be mistaken for something someone owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Focus,
    Working,
    Building,
    Reading,
    Vibing,
    Gaming,
    Eating,
    Away,
}

impl Status {
    /// Every status, in picker and help order. `Away` sits last because
    /// `/brb` is the shortcut that reaches it.
    pub const ALL: [Self; 8] = [
        Self::Focus,
        Self::Working,
        Self::Building,
        Self::Reading,
        Self::Vibing,
        Self::Gaming,
        Self::Eating,
        Self::Away,
    ];

    /// The word the user types and the label every badge paints.
    pub fn word(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Working => "working",
            Self::Building => "building",
            Self::Reading => "reading",
            Self::Vibing => "vibing",
            Self::Gaming => "gaming",
            Self::Eating => "eating",
            Self::Away => "away",
        }
    }

    /// The badge glyph. Checked against the shop catalog in `status_test.rs`:
    /// none of these may be purchasable, or a buyer would look like they are
    /// on a status they never set.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Focus => "🍅",
            Self::Working => "💻",
            Self::Building => "🛠️",
            Self::Reading => "📖",
            Self::Vibing => "🎶",
            Self::Gaming => "👾",
            Self::Eating => "🍽️",
            Self::Away => "💤",
        }
    }

    /// Parse one typed word. `None` is a usage banner at the call site, never
    /// a silent fallback to some default status.
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.word().eq_ignore_ascii_case(word))
    }

    /// `focus, working, building, …`, for the usage banner and the help
    /// modal, so the valid set is never written out by hand twice.
    pub fn word_list() -> String {
        Self::ALL
            .into_iter()
            .map(Self::word)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One session's live status. `ends_at` carries the whole behavioral
/// difference between the two ways to set one, so it is the only field
/// besides the status itself:
///
/// - `Some(_)`: a countdown. It survives the owner posting and retires when
///   the clock runs out.
/// - `None`: open-ended. Nothing expires it; the owner's next chat message
///   clears it. This is what `/brb` sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionStatus {
    pub status: Status,
    pub ends_at: Option<DateTime<Utc>>,
}

impl SessionStatus {
    /// Whole seconds left, rounded up so a freshly started 25 minute timer
    /// reads `25:00` instead of `24:59`, and floored at zero for the frame
    /// between expiry and the 1Hz edge in `tick.rs` that clears it. `None`
    /// for an open-ended status, which has nothing to count down.
    pub fn remaining_secs(self, now: DateTime<Utc>) -> Option<u64> {
        self.ends_at.map(|ends_at| remaining_secs(ends_at, now))
    }

    /// Whether the countdown has run out. Always false for an open-ended
    /// status: only a chat message clears one of those.
    pub fn is_expired(self, now: DateTime<Utc>) -> bool {
        self.remaining_secs(now) == Some(0)
    }

    /// Whether the owner's next chat message clears this. The inverse of
    /// carrying a countdown, named so call sites read as the rule rather
    /// than as an `is_none` check on a timestamp.
    pub fn clears_on_post(self) -> bool {
        self.ends_at.is_none()
    }

    /// The owner's status HUD badge: `MM:SS word` while a countdown runs,
    /// `glyph word` when open-ended. Minutes are not wrapped into hours
    /// because the command caps a countdown well under the point where
    /// three-digit minutes would be confusing.
    pub fn hud_badge(self, now: DateTime<Utc>) -> String {
        match self.remaining_secs(now) {
            Some(remaining) => format!(
                "{:02}:{:02} {}",
                remaining / 60,
                remaining % 60,
                self.status.word()
            ),
            None => format!("{} {}", self.status.glyph(), self.status.word()),
        }
    }

    /// The peer-facing badge for the chat author line: `glyph Nm word` while
    /// a countdown runs (whole minutes, rounded up, so a 25 minute block
    /// reads `🍅 25m focus` on its first frame and `🍅 1m focus` through its
    /// final minute), `glyph word` when open-ended. `None` once a countdown
    /// is stale, which is how expiry is retired without a sweeper.
    pub fn peer_badge(self, now: DateTime<Utc>) -> Option<String> {
        let (glyph, word) = (self.status.glyph(), self.status.word());
        match self.remaining_secs(now) {
            Some(remaining) => {
                let minutes = remaining.div_ceil(60);
                (minutes > 0).then(|| format!("{glyph} {minutes}m {word}"))
            }
            None => Some(format!("{glyph} {word}")),
        }
    }
}

fn remaining_secs(ends_at: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    let millis = (ends_at - now).num_milliseconds().max(0) as u64;
    millis.div_ceil(1_000)
}

/// Snapshot-swap directory of live statuses, shared process-wide.
pub type StatusDirectory = Arc<Mutex<Arc<HashMap<Uuid, SessionStatus>>>>;

pub fn new_directory() -> StatusDirectory {
    Arc::new(Mutex::new(Arc::new(HashMap::new())))
}

/// Cheap read: clones the inner `Arc`, never the map.
pub fn snapshot(directory: &StatusDirectory) -> Arc<HashMap<Uuid, SessionStatus>> {
    Arc::clone(&directory.lock_recover())
}

/// Publish or clear one user's status. Called from every place that changes
/// `App::status` (the `/status` command, the picker, the message that clears
/// an open-ended one, the tick that expires a countdown) and from session
/// teardown, so a disconnect doesn't leave a peer showing as focusing until
/// their original end time.
///
/// Readers retain their snapshot `Arc`, so `make_mut` always clones and swaps
/// the pointer. The chat row cache epoch depends on that: never mutate the
/// map in place.
pub fn set_user(directory: &StatusDirectory, user_id: Uuid, status: Option<SessionStatus>) {
    let mut guard = directory.lock_recover();
    if guard.get(&user_id).copied() == status {
        return;
    }
    let entries = Arc::make_mut(&mut *guard);
    match status {
        Some(status) => {
            entries.insert(user_id, status);
        }
        None => {
            entries.remove(&user_id);
        }
    }
}

/// Resolve a directory snapshot into the per-peer badge strings a chat frame
/// paints. Own-session badges are included: seeing your own status next to
/// your name is the same information the top border already shows.
pub fn resolve_all(
    entries: &HashMap<Uuid, SessionStatus>,
    now: DateTime<Utc>,
) -> HashMap<Uuid, String> {
    entries
        .iter()
        .filter_map(|(user_id, status)| status.peer_badge(now).map(|badge| (*user_id, badge)))
        .collect()
}

#[cfg(test)]
#[path = "status_test.rs"]
mod status_test;
