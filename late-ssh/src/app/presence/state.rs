//! Presence as one replica knows it: its own sessions' records (the source
//! of what it publishes) and every other replica's, heard off the wire.
//! Pure: no I/O, no clock reads (every `now_ms` is handed in), so the whole
//! exchange between replicas is a pair of these driven by hand.
//!
//! Room-agnostic on purpose: a record is one session everywhere it is, and
//! each room derives what it draws from the records (`clubhouse/crowd.rs`,
//! `clubhouse/nightcap/stools.rs`, `deadchannel/street/state.rs`).
//!
//! Local records are this replica's truth about its own sessions. Remote
//! records are only as good as their last heartbeat: one not heard from in
//! [`HEARD_TTL_MS`] is dropped, which is how a replica that died (and could
//! say nothing on the way out) leaves everyone else's rooms.

use std::collections::{HashMap, HashSet};

use late_core::models::presence::{PresenceBatch, PresenceRecord};
use uuid::Uuid;

/// How long a remote record holds without a heartbeat. Three heartbeats
/// and a bit: a single lost notify never blinks anyone out.
pub const HEARD_TTL_MS: i64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Heard {
    record: PresenceRecord,
    heard_at_ms: i64,
}

#[derive(Debug)]
pub struct Presence {
    replica_id: Uuid,
    local: HashMap<Uuid, PresenceRecord>,
    remote: HashMap<Uuid, Heard>,
    /// Local sessions whose record changed since the last publish.
    dirty: HashSet<Uuid>,
    /// Local sessions gone since the last publish.
    left: HashSet<Uuid>,
}

impl Presence {
    pub fn new(replica_id: Uuid) -> Self {
        Self {
            replica_id,
            local: HashMap::new(),
            remote: HashMap::new(),
            dirty: HashSet::new(),
            left: HashSet::new(),
        }
    }

    /// A local session's record, as it stands now. Returns whether it
    /// changed.
    pub fn put(&mut self, record: PresenceRecord) -> bool {
        let session_id = record.session_id;
        if self.local.get(&session_id) == Some(&record) {
            return false;
        }
        self.local.insert(session_id, record);
        self.left.remove(&session_id);
        self.dirty.insert(session_id);
        true
    }

    /// A local session ended. Returns whether it had a record.
    pub fn leave(&mut self, session_id: Uuid) -> bool {
        if self.local.remove(&session_id).is_none() {
            return false;
        }
        self.dirty.remove(&session_id);
        self.left.insert(session_id);
        true
    }

    /// Fold in what another replica said. This replica's own batches come
    /// back over the same wire and are ignored: its local records are
    /// already the truth. Returns whether anything changed.
    pub fn hear(&mut self, batch: PresenceBatch, now_ms: i64) -> bool {
        if batch.replica_id == self.replica_id {
            return false;
        }
        let mut changed = false;
        for record in batch.records {
            let session_id = record.session_id;
            changed |= self.remote.get(&session_id).map(|heard| &heard.record) != Some(&record);
            self.remote.insert(
                session_id,
                Heard {
                    record,
                    heard_at_ms: now_ms,
                },
            );
        }
        for session_id in batch.left {
            changed |= self.remote.remove(&session_id).is_some();
        }
        changed
    }

    /// Drop every remote record not heard from in [`HEARD_TTL_MS`]. Returns
    /// how many went.
    pub fn expire(&mut self, now_ms: i64) -> usize {
        let before = self.remote.len();
        self.remote
            .retain(|_, heard| now_ms - heard.heard_at_ms < HEARD_TTL_MS);
        before - self.remote.len()
    }

    /// What changed since the last publish, and forget it.
    pub fn take_changes(&mut self) -> PresenceBatch {
        let records = self
            .dirty
            .drain()
            .filter_map(|session_id| self.local.get(&session_id).cloned())
            .collect();
        PresenceBatch {
            replica_id: self.replica_id,
            records,
            left: self.left.drain().collect(),
        }
    }

    /// Every local record (and any leaver not yet published): the heartbeat
    /// that keeps this replica's sessions alive elsewhere and fills a
    /// replica that just arrived.
    pub fn take_heartbeat(&mut self) -> PresenceBatch {
        self.dirty.clear();
        PresenceBatch {
            replica_id: self.replica_id,
            records: self.local.values().cloned().collect(),
            left: self.left.drain().collect(),
        }
    }

    /// This replica's own sessions.
    pub fn local_count(&self) -> usize {
        self.local.len()
    }

    /// Every live record on every replica, in session order, so every
    /// replica hands its rooms the same list.
    pub fn records(&self) -> Vec<PresenceRecord> {
        let mut records: Vec<PresenceRecord> = self
            .local
            .values()
            .cloned()
            .chain(self.remote.values().map(|heard| heard.record.clone()))
            .collect();
        records.sort_by_key(|record| record.session_id);
        records
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
