//! A Dark Room service: thin persistence for a single-player door. There is no
//! shared world, no tick loop, and no published snapshot — each session owns
//! the authoritative game in its own `state::State`, and time is settled
//! forward on demand (see `sim`) rather than driven from here.
//!
//! Cheap to `Clone`: everything lives behind an `Arc`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};

use late_core::{db::Db, models::darkroom_save::DarkroomSave};
use serde_json::Value;
use tokio::sync::{Mutex as TokioMutex, watch};
use uuid::Uuid;

use super::model::Game;
use super::persist;

/// The async result of loading a session's game.
#[derive(Clone)]
pub enum GameLoad {
    /// The DB round-trip is still in flight.
    Loading,
    /// Loaded (or freshly created) and ready to play.
    Ready(Box<Game>),
}

struct Inner {
    db: Db,
    seq: AtomicU64,
    gates: StdMutex<HashMap<Uuid, Arc<TokioMutex<u64>>>>,
}

impl Inner {
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// The write gate for `user_id`, created on first use.
    fn gate(&self, user_id: Uuid) -> Arc<TokioMutex<u64>> {
        self.gates
            .lock()
            .unwrap()
            .entry(user_id)
            .or_default()
            .clone()
    }
}

#[derive(Clone)]
pub struct DarkroomService {
    inner: Arc<Inner>,
}

impl DarkroomService {
    pub fn new(db: Db) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                seq: AtomicU64::new(0),
                gates: StdMutex::new(HashMap::new()),
            }),
        }
    }

    /// Begin loading `user_id`'s game. Returns a watch receiver that flips from
    /// [`GameLoad::Loading`] to [`GameLoad::Ready`] once the DB round-trip
    /// completes. A missing save yields a fresh dark room.
    ///
    /// Note that time is **not** settled here: the session does that once it
    /// has the game, because settling needs the session's connect time to know
    /// how much of the gap to credit.
    pub fn load_game(&self, user_id: Uuid) -> watch::Receiver<GameLoad> {
        let (tx, rx) = watch::channel(GameLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let game = match inner.db.get().await {
                Ok(client) => match DarkroomSave::load(&client, user_id).await {
                    Ok(Some(blob)) => persist::from_json(&blob),
                    Ok(None) => Game::new(),
                    Err(e) => {
                        tracing::warn!(error = ?e, "darkroom save load failed");
                        Game::new()
                    }
                },
                Err(e) => {
                    tracing::warn!(error = ?e, "darkroom db get failed on load");
                    Game::new()
                }
            };
            let _ = tx.send(GameLoad::Ready(Box::new(game)));
        });
        rx
    }

    /// Persist a game, fire-and-forget. Ordered per user through a write gate,
    /// so a burst of saves cannot land out of order.
    pub fn save_game(&self, user_id: Uuid, game: &Game) {
        let seq = self.inner.next_seq();
        let gate = self.inner.gate(user_id);
        let db = self.inner.db.clone();
        let blob = persist::to_json(game);
        tokio::spawn(commit_save(db, gate, seq, user_id, blob));
    }

    /// Delete a user's save, fire-and-forget (the "start over" action), ordered
    /// against any pending save through the same gate.
    pub fn delete_game(&self, user_id: Uuid) {
        let seq = self.inner.next_seq();
        let gate = self.inner.gate(user_id);
        let db = self.inner.db.clone();
        tokio::spawn(commit_delete(db, gate, seq, user_id));
    }
}

/// Commit a save blob under the user's write gate, dropping the write if a
/// newer one (higher `seq`) already landed.
async fn commit_save(db: Db, gate: Arc<TokioMutex<u64>>, seq: u64, user_id: Uuid, blob: Value) {
    let mut watermark = gate.lock().await;
    if seq <= *watermark {
        return; // a newer snapshot already committed
    }
    match db.get().await {
        Ok(client) => match DarkroomSave::save(&client, user_id, blob).await {
            Ok(_) => *watermark = seq,
            Err(e) => tracing::warn!(error = ?e, "darkroom save failed"),
        },
        Err(e) => tracing::warn!(error = ?e, "darkroom db get failed on save"),
    }
}

/// Delete a save under the same write gate, ordered against pending saves.
async fn commit_delete(db: Db, gate: Arc<TokioMutex<u64>>, seq: u64, user_id: Uuid) {
    let mut watermark = gate.lock().await;
    if seq <= *watermark {
        return;
    }
    match db.get().await {
        Ok(client) => match DarkroomSave::delete_by_user_id(&client, user_id).await {
            Ok(_) => *watermark = seq,
            Err(e) => tracing::warn!(error = ?e, "darkroom delete failed"),
        },
        Err(e) => tracing::warn!(error = ?e, "darkroom db get failed on delete"),
    }
}
