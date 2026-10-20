//! The drunk map: every drinker's raw buzz as last written to
//! `user_drinks`, mirrored in memory for the process so the tavern's
//! wobble, the passed-out figure and every chat author label read it
//! without a query. A cache of DB rows, not a source of truth: the ghost
//! seed task (`run_drunk_glow_task`) replaces it from the table every 60s
//! on every replica, and a pour on this replica bumps it at once
//! (`record_drink`); a pour that lands while a seed is reading the table
//! outranks the seed's older row. A pour on another replica shows here on
//! the next seed.
//!
//! The effective level decays against wall clock at read time (see
//! `late_core::models::drinks`), so a drinker sobers up while logged out.
//! Not pruned by presence: a drinker who logs out keeps tinting their chat
//! history until the buzz decays or the seed replaces the map.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use late_core::MutexRecover;
use late_core::models::drinks::{decayed_points, drunk_level};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
struct DrunkEntry {
    points: i64,
    last_drink_at: DateTime<Utc>,
}

impl DrunkEntry {
    fn level(&self, now: DateTime<Utc>) -> u8 {
        drunk_level(decayed_points(
            self.points,
            (now - self.last_drink_at).num_seconds(),
        ))
    }
}

#[derive(Clone, Debug, Default)]
pub struct DrunkMap {
    inner: Arc<Mutex<HashMap<Uuid, DrunkEntry>>>,
}

impl DrunkMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a fresh pour so the buyer's glow updates instantly, ahead of
    /// the next DB seed pass.
    pub fn record_drink(&self, user_id: Uuid, points: i64, last_drink_at: DateTime<Utc>) {
        self.inner.lock_recover().insert(
            user_id,
            DrunkEntry {
                points,
                last_drink_at,
            },
        );
    }

    /// Replace the map from a DB seed pass read at `seeded_at`. Passing only
    /// still-active rows also prunes users who sobered up or were deleted.
    /// A pour recorded here after `seeded_at` (it landed while the seed was
    /// reading the table) is newer than the snapshot and is kept, so a seed
    /// never knocks a patron back below a line they just crossed.
    pub fn set_drunk_states(
        &self,
        seeded_at: DateTime<Utc>,
        entries: Vec<(Uuid, i64, DateTime<Utc>)>,
    ) {
        let mut inner = self.inner.lock_recover();
        let mut seeded: HashMap<Uuid, DrunkEntry> = entries
            .into_iter()
            .map(|(user_id, points, last_drink_at)| {
                (
                    user_id,
                    DrunkEntry {
                        points,
                        last_drink_at,
                    },
                )
            })
            .collect();
        for (user_id, entry) in inner.iter() {
            if entry.last_drink_at > seeded_at {
                seeded.insert(*user_id, *entry);
            }
        }
        *inner = seeded;
    }

    /// One drinker's level right now; 0 when the map has never seen them.
    pub fn level(&self, user_id: Uuid, now: DateTime<Utc>) -> u8 {
        self.inner
            .lock_recover()
            .get(&user_id)
            .map_or(0, |entry| entry.level(now))
    }

    /// Current drunk levels, omitting the sober.
    pub fn levels(&self, now: DateTime<Utc>) -> HashMap<Uuid, u8> {
        self.inner
            .lock_recover()
            .iter()
            .filter_map(|(id, entry)| {
                let level = entry.level(now);
                (level > 0).then_some((*id, level))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "drunk_test.rs"]
mod drunk_test;
