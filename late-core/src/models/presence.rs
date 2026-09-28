//! Who is where, as it travels between replicas (`late-ssh` `app/presence`).
//!
//! There is no table here on purpose. Presence is not truth: it lives in
//! the session it describes, and losing all of it on a restart is the
//! right behavior (the next login sits down fresh). Each replica batches
//! its own sessions' records into one notify per flush and repeats all of
//! them as a heartbeat, so a replica that boots or reconnects fills within
//! one heartbeat and a replica that dies drops off everyone else's rooms
//! when its heartbeats stop. Nothing to store, nothing to sweep.
//!
//! One record per session carries every room: the clubhouse (always: a
//! logged-in session is in the tavern), the Nightcap stool it holds, and
//! its runner on the night city street. The payload is JSON; a batch too
//! big for one notify goes out as several ([`PresenceBatch::into_notifies`]),
//! each under [`MAX_PAYLOAD_BYTES`], well below Postgres' 8000-byte cap.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio_postgres::Client;
use uuid::Uuid;

/// Cross-process channel for presence.
pub const PRESENCE_CHANNEL: &str = "presence";

/// Largest payload one notify carries.
pub const MAX_PAYLOAD_BYTES: usize = 7000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emote {
    Wave,
    Dance,
}

/// Where a session's patron is in the tavern: parked on a seat, a standing
/// spot, or at the door, or walking at a cell. The session picks its own
/// spot; two sessions that pick the same one are settled where the rooms
/// are drawn (`clubhouse/crowd.rs`), the earlier claim keeping it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum Spot {
    Seat { index: u16 },
    Standing { index: u16 },
    Door,
    Walking { x: u16, y: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClubhouseStand {
    pub spot: Spot,
    /// When this spot was taken (or the last step landed), unix ms: the
    /// earlier claim keeps a contested seat, and one user with two
    /// sessions shows where they last moved.
    pub since_ms: i64,
    /// The last wave or dance and when it started.
    pub emote: Option<(Emote, i64)>,
    /// When this session last petted the dog.
    pub petted_dog_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NightcapStand {
    pub stool: u8,
    /// When they sat, unix ms: the earlier claim keeps a contested stool.
    pub sat_at_ms: i64,
    /// Drinks poured for them this sitting.
    pub drinks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreetStand {
    pub x: u16,
    pub y: u16,
    /// Whether the session is looking at the street right now. A runner
    /// whose session is on another page stays where it was left, dim.
    pub present: bool,
    /// When this runner last moved (or arrived), unix ms.
    pub moved_at_ms: i64,
}

/// One session, everywhere it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceRecord {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub clubhouse: ClubhouseStand,
    pub nightcap: Option<NightcapStand>,
    pub street: Option<StreetStand>,
}

/// What one replica says in one breath: the records that changed (or all
/// of them, on a heartbeat) and the sessions that ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceBatch {
    pub replica_id: Uuid,
    pub records: Vec<PresenceRecord>,
    pub left: Vec<Uuid>,
}

impl PresenceBatch {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.left.is_empty()
    }

    pub fn to_payload(&self) -> String {
        serde_json::to_string(self).expect("a presence batch always serializes")
    }

    /// Split into batches whose payloads each fit [`MAX_PAYLOAD_BYTES`],
    /// records first, in order. An empty batch splits into nothing.
    pub fn into_notifies(self) -> Vec<PresenceBatch> {
        let empty = || PresenceBatch {
            replica_id: self.replica_id,
            records: Vec::new(),
            left: Vec::new(),
        };
        let base = empty().to_payload().len();
        let mut out = Vec::new();
        let mut current = empty();
        let mut size = base;
        for record in self.records.iter().cloned() {
            let entry = serde_json::to_string(&record)
                .expect("a presence record always serializes")
                .len()
                + 1;
            if size + entry > MAX_PAYLOAD_BYTES && !current.is_empty() {
                out.push(std::mem::replace(&mut current, empty()));
                size = base;
            }
            current.records.push(record);
            size += entry;
        }
        for session_id in self.left.iter().copied() {
            // A quoted hyphenated uuid and its comma.
            let entry = 39;
            if size + entry > MAX_PAYLOAD_BYTES && !current.is_empty() {
                out.push(std::mem::replace(&mut current, empty()));
                size = base;
            }
            current.left.push(session_id);
            size += entry;
        }
        if !current.is_empty() {
            out.push(current);
        }
        out
    }
}

/// Send one batch to every replica, this one included (it ignores its
/// own). The caller splits first: an unsplit batch may not fit a notify.
pub async fn notify_presence(client: &Client, batch: &PresenceBatch) -> Result<()> {
    let payload = batch.to_payload();
    client
        .execute("SELECT pg_notify($1, $2)", &[&PRESENCE_CHANNEL, &payload])
        .await
        .context("notifying presence")?;
    Ok(())
}

/// The batch carried by a [`PRESENCE_CHANNEL`] payload. `None` for anything
/// this module did not write.
pub fn parse_presence_payload(payload: &str) -> Option<PresenceBatch> {
    serde_json::from_str(payload).ok()
}

#[cfg(test)]
#[path = "presence_test.rs"]
mod presence_test;
