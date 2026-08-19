//! A Dark Room service: thin persistence and the ending's reward plumbing for
//! a single-player door. There is no shared world, no tick loop, and no
//! published snapshot — each session owns the authoritative game in its own
//! `state::State`, and time is settled forward on demand (see `sim`) rather
//! than driven from here.
//!
//! Cheap to `Clone`: everything lives behind an `Arc`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};

use late_core::{
    db::Db,
    models::{
        chips::ChipMove,
        darkroom_save::DarkroomSave,
        profile_award::{
            DARKROOM_ESCAPE_AWARD_CATEGORY, award_badge, grant_unique_milestone_award,
        },
        reward::DARKROOM_ESCAPE_REWARD_KEY,
    },
};
use serde_json::Value;
use tokio::sync::{Mutex as TokioMutex, watch};
use uuid::Uuid;

use crate::app::{
    activity::event::ActivityGame, activity::publisher::ActivityPublisher,
    games::chips::svc::ChipService,
};

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
    activity: ActivityPublisher,
    chips: ChipService,
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
    pub fn new(activity: ActivityPublisher, chips: ChipService, db: Db) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                // Gates start at watermark 0, and `commit_save` drops any
                // seq <= watermark, so the first seq handed out must be 1:
                // at 0 the process's first save is silently discarded.
                seq: AtomicU64::new(1),
                gates: StdMutex::new(HashMap::new()),
                activity,
                chips,
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

    /// The ending's reward, fire-and-forget (the Green Dragon dragon-kill
    /// shape): a feed line for every escape, and — first escape only, deduped
    /// by the lifetime reward template and the `NOT EXISTS` award insert — a
    /// once-per-account chip payout plus the rankless ADE profile badge.
    ///
    /// The save is wiped on the way out, so every later run reaches the same
    /// ending; the account only ever gets paid for the first one.
    pub fn reward_escape(&self, user_id: Uuid) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            inner
                .activity
                .game_won_task(user_id, ActivityGame::Darkroom, None, None);

            let grant = match inner
                .chips
                .credit_lifetime_reward_template(
                    user_id,
                    DARKROOM_ESCAPE_REWARD_KEY,
                    ChipMove::DarkroomEscape,
                )
                .await
            {
                Ok(grant) => grant,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        "failed to credit darkroom escape chips"
                    );
                    return;
                }
            };
            // Already claimed on an earlier run: the chips stay suppressed,
            // but the badge insert below still runs (it is `NOT EXISTS`
            // idempotent), so a badge insert that failed on the crediting run
            // heals on a later escape instead of being lost for good.
            if !grant.credited {
                tracing::info!(
                    user_id = %user_id,
                    "suppressed darkroom escape chips because lifetime payout was already claimed"
                );
            }

            let badge = award_badge(DARKROOM_ESCAPE_AWARD_CATEGORY, 1);
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(error) = grant_unique_milestone_award(
                        &client,
                        user_id,
                        DARKROOM_ESCAPE_AWARD_CATEGORY,
                        grant.amount,
                    )
                    .await
                    {
                        tracing::error!(
                            ?error,
                            user_id = %user_id,
                            badge = %badge,
                            "failed to grant darkroom profile award badge"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        badge = %badge,
                        "no db client for darkroom profile award badge"
                    );
                }
            }
        });
    }

    /// Delete a user's save, fire-and-forget (the "start over" action, and the
    /// wipe the ending performs), ordered against any pending save through the
    /// same gate.
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
