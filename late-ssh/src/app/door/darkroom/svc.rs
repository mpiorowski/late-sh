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
        darkroom_veteran::DarkroomVeteran,
        profile_award::{award_badge, grant_unique_milestone_award},
        reward::{DARKROOM_BEACON_REWARD_KEY, DARKROOM_ESCAPE_REWARD_KEY},
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
use super::state::Escape;

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
            let mut game = match inner.db.get().await {
                Ok(client) => {
                    let veteran = has_escaped(&client, user_id).await;
                    let mut game = match DarkroomSave::load(&client, user_id).await {
                        Ok(Some(blob)) => persist::from_json(&blob),
                        Ok(None) => Game::new(veteran),
                        Err(e) => {
                            tracing::warn!(error = ?e, "darkroom save load failed");
                            Game::new(veteran)
                        }
                    };
                    // The account's history is read on every load, not only on
                    // the first: whoever earned the unlock mid-save should see
                    // the wreck without throwing that save away. A legacy read
                    // that fails reads as no history, which costs a veteran
                    // one landmark rather than costing everyone the run.
                    game.veteran = veteran;
                    game
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "darkroom db get failed on load");
                    Game::new(false)
                }
            };
            // A map drawn before the account had ever flown out has no wreck
            // on it. Put one there now rather than making them start over.
            if game.veteran
                && let Some(world) = game.world.as_mut()
            {
                let mut rng = rand::thread_rng();
                if super::world::place_battleship(world, &mut rng) {
                    tracing::info!(
                        user_id = %user_id,
                        "placed the ravaged battleship on an existing darkroom map"
                    );
                }
            }
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

    /// Everything the account keeps from a finished run, fire-and-forget (the
    /// Green Dragon dragon-kill shape): a feed line every time, the veteran
    /// row that unlocks the ravaged battleship on later maps, the chip payout,
    /// and — first escape of this kind only, on the `NOT EXISTS` award insert
    /// — a rankless profile badge.
    ///
    /// The chips land for every run that gets out (SHOP.md Phase 6): the
    /// ending wipes the save, so a repeat is the whole arc walked again, and
    /// the run is the whole gate. `run_id` is that gate, keyed per ending, so
    /// a retry of this task pays once.
    ///
    /// The two endings are separate claims: an account that flies out plainly
    /// and later flies out holding the fleet beacon is paid for both.
    pub fn reward_escape(&self, user_id: Uuid, escape: Escape, run_id: Uuid) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            // Both endings post to the feed, and they say different things:
            // "flew out of A Dark Room" is the whole story for one of them and
            // only half of it for the other. Kept short, like Lateania's
            // crowns: the chips and the badge are on the profile, not spelled
            // out in the stream.
            inner.activity.game_won_task(
                user_id,
                ActivityGame::Darkroom,
                escape.feed_detail().map(str::to_string),
                None,
            );

            // The veteran row goes first and on its own: it is the one thing
            // here that changes what the game *is* next time, and it must not
            // be lost to a failure in the payout path below.
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(error) = DarkroomVeteran::record(&client, user_id).await {
                        tracing::error!(
                            ?error,
                            user_id = %user_id,
                            "failed to record darkroom escape; the battleship stays locked"
                        );
                    }
                }
                Err(error) => tracing::error!(
                    ?error,
                    user_id = %user_id,
                    "no db client to record darkroom escape"
                ),
            }

            let (reward_key, chip_move) = match escape {
                Escape::Plain => (DARKROOM_ESCAPE_REWARD_KEY, ChipMove::DarkroomEscape),
                Escape::WithBeacon => (DARKROOM_BEACON_REWARD_KEY, ChipMove::DarkroomBeaconEscape),
            };
            let category = escape.award_category();
            let event_key = run_id.to_string();
            let grant = match inner
                .chips
                .credit_per_event_reward_template(user_id, reward_key, &event_key, chip_move)
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
            // This run already paid (a retry of the same fire-and-forget
            // task): the chips stay suppressed, but the badge insert below
            // still runs (it is `NOT EXISTS` idempotent), so a badge insert
            // that failed on the crediting run heals on a later escape
            // instead of being lost for good.
            if !grant.credited {
                tracing::info!(
                    user_id = %user_id,
                    run_id = %run_id,
                    "suppressed darkroom escape chips because this run already paid"
                );
            }

            let badge = award_badge(category, 1);
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(error) =
                        grant_unique_milestone_award(&client, user_id, category, grant.amount).await
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

/// Whether the account has finished before, treating a failed read as "no".
/// A room has to be handed to the player either way, and the worst a wrong
/// answer here can do is leave one landmark off one map until the next visit.
async fn has_escaped(client: &tokio_postgres::Client, user_id: Uuid) -> bool {
    match DarkroomVeteran::has_escaped(client, user_id).await {
        Ok(escaped) => escaped,
        Err(error) => {
            tracing::warn!(
                ?error,
                user_id = %user_id,
                "darkroom veteran lookup failed; starting the run without the battleship"
            );
            false
        }
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
