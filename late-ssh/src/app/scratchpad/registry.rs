//! The process-global pairing registry: who is paired with whom, and the
//! shared live text buffer each pairing edits together.
//!
//! Pairing is a mutual handshake. `/pair @b` from a records a one-sided
//! intent and posts a notice to b; nothing else happens to b's session. The
//! pairing exists only once b answers with `/pair @a` inside
//! [`PAIR_INTENT_TTL`]. Nobody can push state onto a session that did not ask
//! for it, so there is no accept/decline prompt and nothing that owns input.
//!
//! Single-replica by design: this is an in-process `Arc<Mutex<..>>` like the
//! clubhouse lobby (`clubhouse/lobby.rs`) and the active-users map. Pairings
//! are inherently ephemeral (no DB row), so losing them on a replica restart
//! is an accepted trade-off, not a gap to fix later.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use late_core::MutexRecover;
use uuid::Uuid;

use super::highlight::Language;

/// How long a one-sided `/pair` intent (and its notice) stays live before it
/// is pruned. Long enough to read the banner, finish what you were doing and
/// still answer; short enough that a forgotten intent cannot pair you with
/// someone an hour later.
pub const PAIR_INTENT_TTL: Duration = Duration::from_secs(10 * 60);

/// How long before the same asker can put another "wants to pair" banner in
/// front of the same target. Deliberately equal to [`PAIR_INTENT_TTL`]: you
/// may nudge someone again exactly when your previous ask has lapsed, so the
/// spam guard never outlives the thing it guards and a genuine retry (they
/// were in a door game and missed it) is never refused.
pub const PAIR_NOTICE_COOLDOWN: Duration = PAIR_INTENT_TTL;

/// The session that ran `/pair @other`. A pairing is bound to the session
/// token, not just the user: one human can hold several concurrent SSH
/// sessions, and only the one that asked should be pulled into the editor.
#[derive(Clone, Debug)]
pub struct PairSide {
    pub user_id: Uuid,
    pub username: String,
    pub session_token: String,
}

/// A one-sided `/pair @other` intent, keyed by the user who ran it first.
/// Becomes a pairing only when `to` mirrors it before `at + PAIR_INTENT_TTL`.
#[derive(Clone, Debug)]
struct PairIntent {
    from: PairSide,
    to: Uuid,
    at: Instant,
}

/// A "someone wants to pair with you" banner waiting to be drained by the
/// target's next tick. Purely informational: it captures no input.
#[derive(Clone, Debug)]
struct PairNotice {
    from_username: String,
    at: Instant,
}

/// A live pairing as the registry sees it for one participant.
#[derive(Clone, Debug)]
struct PairedSession {
    session_token: String,
    shared: SharedScratchpad,
}

/// The live buffer two paired users edit together. No persistence, no
/// operational-transform merge: every publish replaces the whole buffer and
/// bumps `revision`, which is the only conflict "resolution" v1 needs.
#[derive(Debug)]
pub struct ScratchpadBuffer {
    pub content: String,
    pub revision: u64,
    pub user_a: (Uuid, String),
    pub user_b: (Uuid, String),
    /// Last-known cursor per side, presence-only (never used to merge edits).
    cursor_a: (usize, usize),
    cursor_b: (usize, usize),
    /// Set once a side's session actually opened the editor. A pairing whose
    /// other side never joined (their session died between `/pair` and the
    /// mirror command) is torn down whole on the first leave, so the absent
    /// user is not left permanently marked as paired.
    joined_a: bool,
    joined_b: bool,
    /// Set to the user_id of whichever side left first. Once both sides have
    /// left, the registry drops the pairing entirely.
    pub left: Option<Uuid>,
    /// The highlighting language, shared so both sides always see the same
    /// one. Lives here rather than on a `/pair @user <language>` command arg
    /// because pairing is a mutual handshake between two independent
    /// commands: a per-command language could have the two sides disagree,
    /// where a shared field on the buffer they already both read cannot.
    pub(crate) language: Language,
}

impl ScratchpadBuffer {
    fn new(user_a: (Uuid, String), user_b: (Uuid, String)) -> Self {
        Self {
            content: String::new(),
            revision: 0,
            user_a,
            user_b,
            cursor_a: (0, 0),
            cursor_b: (0, 0),
            joined_a: false,
            joined_b: false,
            left: None,
            language: Language::default(),
        }
    }

    /// The other participant's id and display name, given one side.
    pub fn partner_of(&self, user_id: Uuid) -> Option<(Uuid, &str)> {
        if self.user_a.0 == user_id {
            Some((self.user_b.0, self.user_b.1.as_str()))
        } else if self.user_b.0 == user_id {
            Some((self.user_a.0, self.user_a.1.as_str()))
        } else {
            None
        }
    }

    pub fn cursor_for(&self, user_id: Uuid) -> (usize, usize) {
        if self.user_a.0 == user_id {
            self.cursor_a
        } else {
            self.cursor_b
        }
    }

    pub fn set_cursor_for(&mut self, user_id: Uuid, pos: (usize, usize)) {
        if self.user_a.0 == user_id {
            self.cursor_a = pos;
        } else {
            self.cursor_b = pos;
        }
    }

    /// Called by `ScratchpadState::new`: this side has the editor open.
    pub fn mark_joined(&mut self, user_id: Uuid) {
        if self.user_a.0 == user_id {
            self.joined_a = true;
        } else {
            self.joined_b = true;
        }
    }

    /// Advance the shared highlighting language and bump `revision` so the
    /// partner's next `sync_from_shared` picks it up promptly, same as the
    /// `left` bump above.
    pub fn cycle_language(&mut self) {
        self.language = self.language.next();
        self.revision += 1;
    }

    fn joined(&self, user_id: Uuid) -> bool {
        if self.user_a.0 == user_id {
            self.joined_a
        } else {
            self.joined_b
        }
    }
}

pub(crate) type SharedScratchpad = Arc<Mutex<ScratchpadBuffer>>;

/// What `/pair @other` did. Every arm maps to exactly one banner at the call
/// site, so the whole command reads as a list of outcomes.
#[derive(Debug)]
pub enum PairOutcome {
    /// Intent recorded and the target notified. They have
    /// [`PAIR_INTENT_TTL`] to mirror it.
    Waiting,
    /// Intent recorded (or refreshed), but the target was not notified again:
    /// this asker pinged them less than [`PAIR_NOTICE_COOLDOWN`] ago. The
    /// handshake still completes if they mirror it.
    AlreadyAsked,
    /// Both sides have now asked for each other; the editor is live.
    Paired {
        shared: SharedScratchpad,
        partner_id: Uuid,
        partner_username: String,
    },
    /// This user is already in a pairing.
    AlreadyPaired,
    /// The target is already paired with someone else.
    TargetBusy,
}

/// What one session's per-tick registry poll turned up. Both fields are
/// answered under a single lock, so an idle session pays one acquisition per
/// tick rather than two.
#[derive(Debug)]
pub struct PairPoll {
    /// The shared buffer for a pairing this session owns but has not opened
    /// yet (the side that ran `/pair` first waits here for the mirror).
    pub pairing: Option<SharedScratchpad>,
    /// Username of someone who wants to pair, shown once as a banner.
    pub notice: Option<String>,
}

#[derive(Debug, Default)]
struct RegistryInner {
    /// One entry per participant, indexed under their user_id so either
    /// side's lookup is O(1).
    pairings: HashMap<Uuid, PairedSession>,
    /// One-sided intents, keyed by the user who ran `/pair` first.
    intents: HashMap<Uuid, PairIntent>,
    /// Undrained "wants to pair" banners, keyed by the target.
    notices: HashMap<Uuid, PairNotice>,
    /// When each `(asker, target)` pair was last notified, so re-running
    /// `/pair` in a loop cannot spam someone's banner slot. Keyed by both
    /// ends: one asker must not be able to mute anyone else's ask.
    notified_at: HashMap<(Uuid, Uuid), Instant>,
}

impl RegistryInner {
    /// Drop intents, notices and cooldowns past their TTL. Called from
    /// `try_pair`, the only place any of the three maps grows.
    fn prune(&mut self, now: Instant) {
        self.intents
            .retain(|_, intent| now.saturating_duration_since(intent.at) < PAIR_INTENT_TTL);
        self.notices
            .retain(|_, notice| now.saturating_duration_since(notice.at) < PAIR_INTENT_TTL);
        self.notified_at
            .retain(|_, at| now.saturating_duration_since(*at) < PAIR_NOTICE_COOLDOWN);
    }
}

#[derive(Clone, Debug)]
pub struct SharedScratchpadRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

impl Default for SharedScratchpadRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedScratchpadRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistryInner::default())),
        }
    }

    /// Half of the mutual `/pair` handshake: record `from`'s intent toward
    /// `to`, or complete the pairing when `to` already asked for `from`.
    /// Every busy check happens under the same lock that creates the
    /// pairing, so two simultaneous `/pair` commands cannot both win.
    pub fn try_pair(&self, from: PairSide, to: Uuid, now: Instant) -> PairOutcome {
        let mut inner = self.inner.lock_recover();
        inner.prune(now);

        if inner.pairings.contains_key(&from.user_id) {
            return PairOutcome::AlreadyPaired;
        }
        if inner.pairings.contains_key(&to) {
            return PairOutcome::TargetBusy;
        }

        // Only take their intent when it names us: an intent aimed at a
        // third party has to survive our request untouched.
        let reciprocal = inner
            .intents
            .get(&to)
            .is_some_and(|intent| intent.to == from.user_id);
        let theirs = if reciprocal {
            inner.intents.remove(&to)
        } else {
            None
        };

        let Some(theirs) = theirs else {
            // The intent is always recorded, cooldown or not: suppressing the
            // banner must never stop the handshake from completing when they
            // mirror it. Only the ping is rate limited.
            let notify = inner
                .notified_at
                .get(&(from.user_id, to))
                .is_none_or(|at| now.saturating_duration_since(*at) >= PAIR_NOTICE_COOLDOWN);
            if notify {
                inner.notified_at.insert((from.user_id, to), now);
                inner.notices.insert(
                    to,
                    PairNotice {
                        from_username: from.username.clone(),
                        at: now,
                    },
                );
            }
            inner
                .intents
                .insert(from.user_id, PairIntent { from, to, at: now });
            return if notify {
                PairOutcome::Waiting
            } else {
                PairOutcome::AlreadyAsked
            };
        };

        inner.intents.remove(&from.user_id);
        inner.notices.remove(&from.user_id);
        inner.notices.remove(&to);
        // A pairing answers the ask, so neither side has to wait out a
        // cooldown to reach the other again after they both leave.
        inner.notified_at.remove(&(from.user_id, to));
        inner.notified_at.remove(&(to, from.user_id));

        let partner_id = theirs.from.user_id;
        let partner_username = theirs.from.username;
        let shared: SharedScratchpad = Arc::new(Mutex::new(ScratchpadBuffer::new(
            (partner_id, partner_username.clone()),
            (from.user_id, from.username),
        )));
        inner.pairings.insert(
            partner_id,
            PairedSession {
                session_token: theirs.from.session_token,
                shared: shared.clone(),
            },
        );
        inner.pairings.insert(
            from.user_id,
            PairedSession {
                session_token: from.session_token,
                shared: shared.clone(),
            },
        );

        PairOutcome::Paired {
            shared,
            partner_id,
            partner_username,
        }
    }

    /// One session's per-tick read: a pairing bound to this exact session,
    /// plus any undrained notice. The notice is removed as it is handed out,
    /// so it shows once.
    pub fn poll(&self, user_id: Uuid, session_token: &str) -> PairPoll {
        let mut inner = self.inner.lock_recover();
        let pairing = inner
            .pairings
            .get(&user_id)
            .filter(|paired| paired.session_token == session_token)
            .map(|paired| paired.shared.clone());
        let notice = inner
            .notices
            .remove(&user_id)
            .map(|notice| notice.from_username);
        PairPoll { pairing, notice }
    }

    /// Record that `user_id` left the pairing. The buffer normally survives
    /// until both sides have left, so the other side's next
    /// `sync_from_shared` sees `left` and can say the partner is gone.
    /// Bumps `revision` on the first departure so that sync fires promptly:
    /// `left` alone is otherwise invisible to `sync_from_shared`'s
    /// revision-gated check, and the survivor's screen would only pick it up
    /// on their next unrelated keystroke instead of right away.
    ///
    /// The exception is a partner who never opened the editor at all: there
    /// is nobody to read `left`, so the whole pairing goes at once rather
    /// than stranding that user as permanently paired.
    pub fn leave(&self, user_id: Uuid) {
        let mut inner = self.inner.lock_recover();
        let Some(paired) = inner.pairings.remove(&user_id) else {
            return;
        };
        let (both_left, partner_id, partner_never_joined) = {
            let mut buffer = paired.shared.lock_recover();
            let partner_id = buffer.partner_of(user_id).map(|(id, _)| id);
            let partner_never_joined = partner_id.is_some_and(|id| !buffer.joined(id));
            let both_left = match buffer.left {
                Some(other) if other != user_id => true,
                Some(_) => false,
                None => {
                    buffer.left = Some(user_id);
                    buffer.revision += 1;
                    false
                }
            };
            (both_left, partner_id, partner_never_joined)
        };
        if let Some(partner_id) = partner_id
            && (both_left || partner_never_joined)
        {
            inner.pairings.remove(&partner_id);
        }
    }
}

#[cfg(test)]
#[path = "registry_test.rs"]
mod registry_test;
