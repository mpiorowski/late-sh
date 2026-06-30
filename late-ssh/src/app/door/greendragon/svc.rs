//! Legend of the Green Dragon service: thin persistence + reward plumbing for
//! the single-player door. Unlike Lateania there is no shared world, no tick
//! loop, and no watch-published world snapshot — each session owns the
//! authoritative character in its own `state::State`. This service only loads
//! the character once (off the DB) and saves blobs back, fire-and-forget.
//!
//! Cheap to `Clone`: everything lives behind an `Arc`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use late_core::{db::Db, models::greendragon_character::GreenDragonCharacter};
use rand::Rng;
use serde_json::Value;
use tokio::sync::{Mutex as TokioMutex, watch};
use uuid::Uuid;

use crate::app::{activity::publisher::ActivityPublisher, games::chips::svc::ChipService};

use super::model::{self, Character};
use super::persist;

/// The async result of loading a session's character.
#[derive(Clone)]
pub enum CharacterLoad {
    /// The DB round-trip is still in flight.
    Loading,
    /// Loaded (or freshly created) and ready to play.
    Ready(Box<Character>),
}

#[derive(Clone)]
pub struct GreenDragonService {
    inner: Arc<Inner>,
}

struct Inner {
    db: Db,
    /// Monotonic write sequence. Every save/delete is stamped at submit time so
    /// a stale fire-and-forget write can be discarded instead of clobbering
    /// newer state.
    seq: AtomicU64,
    /// Per-user write gate: serializes that user's persistence and holds the
    /// highest sequence committed so far. An older snapshot (lower seq) that
    /// wins the race is skipped, so saves never go backwards.
    gates: StdMutex<HashMap<Uuid, Arc<TokioMutex<u64>>>>,
    // Held for the forthcoming dragon-kill reward path (chip payout + activity
    // feed entry), mirroring Lateania's milestone awards. Not yet wired.
    #[allow(dead_code)]
    activity: ActivityPublisher,
    #[allow(dead_code)]
    chips: ChipService,
}

impl Inner {
    /// Allocate the next write sequence (stamped synchronously at submit time).
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// The write gate for `user_id`, created on first use.
    fn gate(&self, user_id: Uuid) -> Arc<TokioMutex<u64>> {
        self.gates.lock().unwrap().entry(user_id).or_default().clone()
    }
}

/// Commit a character blob under the user's write gate, dropping the write if a
/// newer one (higher `seq`) already landed. Holding the gate across the DB write
/// serializes that user's persistence.
async fn commit_save(db: Db, gate: Arc<TokioMutex<u64>>, seq: u64, user_id: Uuid, blob: Value) {
    let mut watermark = gate.lock().await;
    if seq <= *watermark {
        return; // a newer snapshot already committed
    }
    match db.get().await {
        Ok(client) => match GreenDragonCharacter::save(&client, user_id, blob).await {
            Ok(_) => *watermark = seq,
            Err(e) => tracing::warn!("greendragon character save failed: {e}"),
        },
        Err(e) => tracing::warn!("greendragon db get failed on save: {e}"),
    }
}

/// Delete a character under the same write gate, ordered against pending saves.
async fn commit_delete(db: Db, gate: Arc<TokioMutex<u64>>, seq: u64, user_id: Uuid) {
    let mut watermark = gate.lock().await;
    if seq <= *watermark {
        return;
    }
    match db.get().await {
        Ok(client) => match GreenDragonCharacter::delete_by_user_id(&client, user_id).await {
            Ok(_) => *watermark = seq,
            Err(e) => tracing::warn!("greendragon character delete failed: {e}"),
        },
        Err(e) => tracing::warn!("greendragon db get failed on delete: {e}"),
    }
}

/// UTC day-number, used to drive once-per-day forest-turn/heal regeneration.
fn today() -> i64 {
    Utc::now().timestamp().div_euclid(86_400)
}

impl GreenDragonService {
    pub fn new(activity: ActivityPublisher, chips: ChipService, db: Db) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                seq: AtomicU64::new(0),
                gates: StdMutex::new(HashMap::new()),
                activity,
                chips,
            }),
        }
    }

    /// Begin loading `user_id`'s character. Returns a watch receiver that flips
    /// from [`CharacterLoad::Loading`] to [`CharacterLoad::Ready`] once the DB
    /// round-trip completes. A missing save yields a fresh level-1 character
    /// named `name`. The new-day reset is applied before the character is
    /// handed to the session.
    pub fn load_character(&self, user_id: Uuid, name: String) -> watch::Receiver<CharacterLoad> {
        let (tx, rx) = watch::channel(CharacterLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let db = inner.db.clone();
            let day = today();
            let mut character = match db.get().await {
                Ok(client) => match GreenDragonCharacter::load(&client, user_id).await {
                    Ok(Some(blob)) => persist::from_json(&blob),
                    Ok(None) => Character::new(name.clone(), day),
                    Err(e) => {
                        tracing::warn!("greendragon character load failed: {e}");
                        Character::new(name.clone(), day)
                    }
                },
                Err(e) => {
                    tracing::warn!("greendragon db get failed on load: {e}");
                    Character::new(name.clone(), day)
                }
            };
            // A corrupt/incompatible blob deserializes to a nameless default;
            // stamp the logged-in name so the player never loads as "".
            if character.name.trim().is_empty() {
                character.name = name;
            }
            // Refill forest turns / heal / revive if a new day has rolled over
            // since the last save. Banked dragon kills add extra daily turns; the
            // bank pays a freshly-rolled interest rate; the day's "spirits"
            // (e_rand(-1,1) twice, -2..+2) jitter the forest fights, LoGD-style.
            let forest_bonus = character.dk_forest_bonus();
            let (interest, spirits) = {
                let mut rng = rand::thread_rng();
                let interest =
                    rng.gen_range(model::MIN_INTEREST_PERCENT..=model::MAX_INTEREST_PERCENT);
                let spirits = rng.gen_range(-1..=1) + rng.gen_range(-1..=1);
                (interest, spirits)
            };
            let rolled = character.roll_new_day(day, forest_bonus, interest, spirits);
            // Persist the rollover immediately: otherwise an instant disconnect
            // drops the spent turns/interest, letting a player reconnect to
            // re-roll a favorable interest rate or dodge the resurrection cost.
            if rolled {
                let seq = inner.next_seq();
                let gate = inner.gate(user_id);
                let blob = persist::to_json(&character);
                tokio::spawn(commit_save(inner.db.clone(), gate, seq, user_id, blob));
            }
            let _ = tx.send(CharacterLoad::Ready(Box::new(character)));
        });
        rx
    }

    /// Persist a character blob, fire-and-forget but **ordered**: stale writes
    /// are dropped against newer ones for the same user (see [`commit_save`]).
    pub fn save_character(&self, user_id: Uuid, character: &Character) {
        let seq = self.inner.next_seq();
        let gate = self.inner.gate(user_id);
        let db = self.inner.db.clone();
        let blob = persist::to_json(character);
        tokio::spawn(commit_save(db, gate, seq, user_id, blob));
    }

    /// Delete a user's saved character, fire-and-forget (the "start over"
    /// action), ordered against any pending save through the same gate.
    pub fn delete_character(&self, user_id: Uuid) {
        let seq = self.inner.next_seq();
        let gate = self.inner.gate(user_id);
        let db = self.inner.db.clone();
        tokio::spawn(commit_delete(db, gate, seq, user_id));
    }
}
