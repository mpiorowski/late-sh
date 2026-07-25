//! The process-global pairing registry: who is paired with whom, and the
//! shared live text buffer each pairing edits together.
//!
//! Single-replica by design: this is an in-process `Arc<Mutex<..>>` like the
//! clubhouse lobby (`clubhouse/lobby.rs`) and the active-users map. Pairings
//! are inherently ephemeral (no DB row), so losing them on a replica restart
//! is an accepted trade-off, not a gap to fix later.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use late_core::MutexRecover;
use uuid::Uuid;

/// A directed `/pair @user` invite awaiting accept/decline, keyed by the
/// target's user_id (not session token: a user can have several concurrent
/// SSH sessions, and the registry only cares about the human, not the tty).
#[derive(Debug, Clone)]
pub struct PendingPairInvite {
    pub from_user_id: Uuid,
    pub from_username: String,
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
    /// Set to the user_id of whichever side left first. Once both sides have
    /// left, the registry drops the pairing entirely.
    pub left: Option<Uuid>,
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
            left: None,
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
}

pub(crate) type SharedScratchpad = Arc<Mutex<ScratchpadBuffer>>;

#[derive(Debug, Default)]
struct RegistryInner {
    /// One entry per pairing, indexed under *both* participants' user_ids so
    /// either side's lookup is O(1) (mirrors the single-Arc-per-shared-value
    /// idiom in `artboard/provenance.rs`).
    pairings: HashMap<Uuid, SharedScratchpad>,
    /// Overwritten by a newer invite to the same target (last-one-wins, same
    /// as directed daily challenges).
    pending_invites: HashMap<Uuid, PendingPairInvite>,
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

    pub fn is_paired(&self, user_id: Uuid) -> bool {
        self.inner.lock_recover().pairings.contains_key(&user_id)
    }

    /// Post (or overwrite) a directed invite for `to`.
    pub fn invite(&self, from_user_id: Uuid, from_username: String, to: Uuid) {
        self.inner.lock_recover().pending_invites.insert(
            to,
            PendingPairInvite {
                from_user_id,
                from_username,
            },
        );
    }

    /// Drain this user's pending invite, if any. Polled once per tick, same
    /// cadence as the `session_rx` drain in `tick.rs`.
    pub fn take_invite_for(&self, user_id: Uuid) -> Option<PendingPairInvite> {
        self.inner.lock_recover().pending_invites.remove(&user_id)
    }

    /// Accept an invite: build a fresh shared buffer for the pair and index
    /// it under both user_ids.
    pub fn accept(
        &self,
        user_id: Uuid,
        username: String,
        invite: PendingPairInvite,
    ) -> SharedScratchpad {
        let shared: SharedScratchpad = Arc::new(Mutex::new(ScratchpadBuffer::new(
            (invite.from_user_id, invite.from_username),
            (user_id, username),
        )));
        let mut inner = self.inner.lock_recover();
        inner.pairings.insert(invite.from_user_id, shared.clone());
        inner.pairings.insert(user_id, shared.clone());
        shared
    }

    pub fn lookup(&self, user_id: Uuid) -> Option<SharedScratchpad> {
        self.inner.lock_recover().pairings.get(&user_id).cloned()
    }

    /// Record that `user_id` left the pairing. The buffer is only torn down
    /// once both sides have left; until then the other side's next
    /// `sync_from_shared` sees `left` and can show a "partner left" banner.
    /// Bumps `revision` on the first departure so that sync fires promptly:
    /// `left` alone is otherwise invisible to `sync_from_shared`'s
    /// revision-gated check, and the survivor's screen would only pick it up
    /// on their next unrelated keystroke instead of right away.
    pub fn leave(&self, user_id: Uuid) {
        let mut inner = self.inner.lock_recover();
        let Some(shared) = inner.pairings.get(&user_id).cloned() else {
            return;
        };
        let both_left = {
            let mut buffer = shared.lock_recover();
            match buffer.left {
                Some(other) if other != user_id => true,
                Some(_) => false,
                None => {
                    buffer.left = Some(user_id);
                    buffer.revision += 1;
                    false
                }
            }
        };
        inner.pairings.remove(&user_id);
        if both_left {
            let partner_id = shared.lock_recover().partner_of(user_id).map(|(id, _)| id);
            if let Some(partner_id) = partner_id {
                inner.pairings.remove(&partner_id);
            }
        }
    }
}

#[cfg(test)]
#[path = "registry_test.rs"]
mod registry_test;
