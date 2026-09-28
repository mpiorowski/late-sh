//! The street as one replica knows it: its own sessions' runners (the
//! source of what it publishes) and every other replica's, heard off the
//! wire. Pure: no I/O, no clock reads (every `now_ms` is handed in), so
//! the whole exchange between replicas is a pair of these driven by hand.
//!
//! Local stands are this replica's truth about its own sessions. Remote
//! stands are only as good as their last heartbeat: one not heard from in
//! [`HEARD_TTL_MS`] is dropped, which is how a replica that died (and
//! could say nothing on the way out) leaves everyone else's street.

use std::collections::{HashMap, HashSet};

use late_core::models::deadchannel_street::{Stand, StreetBatch};
use uuid::Uuid;

/// How long a remote stand holds without a heartbeat. Three heartbeats
/// and a bit: a single lost notify never blinks a runner out.
pub const HEARD_TTL_MS: i64 = 10_000;

/// One runner on the street as a session draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreetRunner {
    pub x: u16,
    pub y: u16,
    /// Some session of this user is looking at the street right now.
    /// Otherwise the runner stands where it was left, dim.
    pub present: bool,
}

/// What sessions draw from: user id to runner, one per user.
pub type StreetView = HashMap<Uuid, StreetRunner>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Heard {
    stand: Stand,
    heard_at_ms: i64,
}

#[derive(Debug)]
pub struct Street {
    replica_id: Uuid,
    local: HashMap<Uuid, Stand>,
    remote: HashMap<Uuid, Heard>,
    /// Local sessions whose stand changed since the last publish.
    dirty: HashSet<Uuid>,
    /// Local sessions gone since the last publish.
    left: HashSet<Uuid>,
}

impl Street {
    pub fn new(replica_id: Uuid) -> Self {
        Self {
            replica_id,
            local: HashMap::new(),
            remote: HashMap::new(),
            dirty: HashSet::new(),
            left: HashSet::new(),
        }
    }

    /// A local session's runner stands at `(x, y)`, looking or not. The
    /// move stamp only moves when the runner does (or arrives), so looking
    /// away does not win the latest-mover tie for a second device. Returns
    /// whether anything changed.
    pub fn stand(
        &mut self,
        session_id: Uuid,
        user_id: Uuid,
        x: u16,
        y: u16,
        present: bool,
        now_ms: i64,
    ) -> bool {
        let moved_at_ms = match self.local.get(&session_id) {
            Some(old) if (old.x, old.y, old.present) == (x, y, present) => return false,
            Some(old) if (old.x, old.y) == (x, y) => old.moved_at_ms,
            Some(_) | None => now_ms,
        };
        self.local.insert(
            session_id,
            Stand {
                session_id,
                user_id,
                x,
                y,
                present,
                moved_at_ms,
            },
        );
        self.left.remove(&session_id);
        self.dirty.insert(session_id);
        true
    }

    /// A local session left the street (logged out, or stopped being a
    /// runner). Returns whether it was on it.
    pub fn leave(&mut self, session_id: Uuid) -> bool {
        if self.local.remove(&session_id).is_none() {
            return false;
        }
        self.dirty.remove(&session_id);
        self.left.insert(session_id);
        true
    }

    /// Fold in what another replica said. This replica's own batches come
    /// back over the same wire and are ignored: its local stands are
    /// already the truth. Returns whether anything changed.
    pub fn hear(&mut self, batch: StreetBatch, now_ms: i64) -> bool {
        if batch.replica_id == self.replica_id {
            return false;
        }
        let mut changed = false;
        for stand in batch.stands {
            let previous = self.remote.insert(
                stand.session_id,
                Heard {
                    stand,
                    heard_at_ms: now_ms,
                },
            );
            changed |= previous.map(|heard| heard.stand) != Some(stand);
        }
        for session_id in batch.left {
            changed |= self.remote.remove(&session_id).is_some();
        }
        changed
    }

    /// Drop every remote stand not heard from in [`HEARD_TTL_MS`]. Returns
    /// how many went.
    pub fn expire(&mut self, now_ms: i64) -> usize {
        let before = self.remote.len();
        self.remote
            .retain(|_, heard| now_ms - heard.heard_at_ms < HEARD_TTL_MS);
        before - self.remote.len()
    }

    /// What changed since the last publish, and forget it.
    pub fn take_changes(&mut self) -> StreetBatch {
        let stands = self
            .dirty
            .drain()
            .filter_map(|session_id| self.local.get(&session_id).copied())
            .collect();
        StreetBatch {
            replica_id: self.replica_id,
            stands,
            left: self.left.drain().collect(),
        }
    }

    /// Every local stand (and any leaver not yet published): the
    /// heartbeat that keeps this replica's runners alive elsewhere and
    /// fills a replica that just arrived.
    pub fn take_heartbeat(&mut self) -> StreetBatch {
        self.dirty.clear();
        StreetBatch {
            replica_id: self.replica_id,
            stands: self.local.values().copied().collect(),
            left: self.left.drain().collect(),
        }
    }

    /// Local sessions on the street right now.
    pub fn local_count(&self) -> usize {
        self.local.len()
    }

    /// One runner per user across every session on every replica: the
    /// position of the latest mover, present if any session is looking.
    pub fn view(&self) -> StreetView {
        let mut latest: HashMap<Uuid, Stand> = HashMap::new();
        let mut present: HashSet<Uuid> = HashSet::new();
        let stands = self
            .local
            .values()
            .chain(self.remote.values().map(|heard| &heard.stand));
        for stand in stands {
            if stand.present {
                present.insert(stand.user_id);
            }
            // The session id breaks a same-millisecond tie, so every
            // replica picks the same runner.
            match latest.get(&stand.user_id) {
                Some(kept)
                    if (kept.moved_at_ms, kept.session_id)
                        >= (stand.moved_at_ms, stand.session_id) => {}
                Some(_) | None => {
                    latest.insert(stand.user_id, *stand);
                }
            }
        }
        latest
            .into_iter()
            .map(|(user_id, stand)| {
                (
                    user_id,
                    StreetRunner {
                        x: stand.x,
                        y: stand.y,
                        present: present.contains(&user_id),
                    },
                )
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
