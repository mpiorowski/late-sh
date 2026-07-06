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
use late_core::{
    db::Db,
    models::{
        greendragon_character::GreenDragonCharacter, greendragon_commentary::GreenDragonCommentary,
        greendragon_news::GreenDragonNews, greendragon_setting::GreenDragonSetting,
    },
};
use rand::Rng;
use serde_json::Value;
use tokio::sync::{Mutex as TokioMutex, watch};
use uuid::Uuid;

use crate::app::{activity::publisher::ActivityPublisher, games::chips::svc::ChipService};

use super::commentary::CommentLine;
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

/// The async result of loading one day's news page.
#[derive(Clone)]
pub enum NewsLoad {
    Loading,
    /// The day's lines, newest first. Empty means a quiet day (or a failed
    /// read — the village paper doesn't distinguish).
    Ready(Arc<Vec<String>>),
}

/// The async result of loading (or posting into) a commentary room's page.
#[derive(Clone)]
pub enum CommentaryLoad {
    Loading,
    Ready {
        /// The room's newest lines, newest first. Empty means a quiet room
        /// (or a failed read — the table doesn't distinguish).
        lines: Arc<Vec<CommentLine>>,
        /// A post was dropped as an exact repeat of the section's newest
        /// line by the same speaker (upstream's double-post check).
        double_post: bool,
    },
}

/// The async result of settling a Five Sixes play against the shared pot.
#[derive(Clone)]
pub enum FiveSixLoad {
    Loading,
    /// `(pot the roll was played against, gold left in the pot afterwards)`.
    /// The win is the difference — or the whole pot on five sixes.
    Ready {
        pot: u64,
        left_over: u64,
    },
    /// The DB failed; the caller refunds the stake and shrugs it off.
    Failed,
}

/// Cap for one day's news page. Upstream pages 50 at a time with page links;
/// a single generous cap stands in for the pager.
const NEWS_PAGE_LIMIT: i64 = 200;

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
        self.gates
            .lock()
            .unwrap()
            .entry(user_id)
            .or_default()
            .clone()
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

/// Fetch a section's newest `limit` rows, stamping each with whether it was
/// posted on the current UTC day (which feeds the daily post allowance).
async fn read_commentary(
    client: &tokio_postgres::Client,
    section: &str,
    limit: usize,
) -> Vec<CommentLine> {
    let today = today();
    match GreenDragonCommentary::latest(client, section, limit as i64).await {
        Ok(rows) => rows
            .into_iter()
            .map(|r| CommentLine {
                user_id: r.user_id,
                name: r.name,
                body: r.body,
                today: r.day == today,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("greendragon commentary read failed: {e}");
            Vec::new()
        }
    }
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
            // since the last save. Spent ff dragon points add extra daily turns;
            // the bank pays a freshly-rolled interest rate; the day's "spirits"
            // (e_rand(-1,1) twice, -2..+2) jitter the forest fights, LoGD-style.
            // The RNG stays inside a sync block (thread_rng isn't Send).
            let rolled = {
                let mut rng = rand::thread_rng();
                let interest =
                    rng.gen_range(model::MIN_INTEREST_PERCENT..=model::MAX_INTEREST_PERCENT);
                let spirits = rng.gen_range(-1..=1) + rng.gen_range(-1..=1);
                character.roll_new_day(day, interest, spirits, &mut rng)
            };
            // Persist the rollover immediately: otherwise an instant disconnect
            // drops the spent turns/interest, letting a player reconnect to
            // re-roll a favorable interest rate or dodge the resurrection cost.
            if let Some(fx) = rolled {
                let seq = inner.next_seq();
                let gate = inner.gate(user_id);
                let blob = persist::to_json(&character);
                tokio::spawn(commit_save(inner.db.clone(), gate, seq, user_id, blob));
                // A dawn divorce makes the paper (`lovers.php`'s addnews).
                if fx.divorced {
                    let body = format!(
                        "{} has left {} to pursue other interests.",
                        crate::app::door::greendragon::data::partner(character.style),
                        character.titled_name(),
                    );
                    if let Ok(client) = inner.db.get().await
                        && let Err(e) =
                            GreenDragonNews::add(&client, day, Some(user_id), &body).await
                    {
                        tracing::warn!("greendragon divorce news write failed: {e}");
                    }
                }
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

    /// Append a line to the village's daily news, fire-and-forget (LoGD
    /// `addnews`). `user_id` is the item's subject; `None` marks a system line.
    pub fn publish_news(&self, user_id: Option<Uuid>, body: String) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonNews::add(&client, today(), user_id, &body).await {
                        tracing::warn!("greendragon news write failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("greendragon db get failed on news write: {e}"),
            }
        });
    }

    /// Load the news page for `days_back` days ago (0 = today). Expired items
    /// are reaped first — upstream prunes at view time too (`news.php`).
    pub fn load_news(&self, days_back: i64) -> watch::Receiver<NewsLoad> {
        let (tx, rx) = watch::channel(NewsLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let day = today() - days_back;
            let lines = match inner.db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonNews::prune(&client, today()).await {
                        tracing::warn!("greendragon news prune failed: {e}");
                    }
                    match GreenDragonNews::list_for_day(&client, day, NEWS_PAGE_LIMIT).await {
                        Ok(lines) => lines,
                        Err(e) => {
                            tracing::warn!("greendragon news read failed: {e}");
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("greendragon db get failed on news read: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send(NewsLoad::Ready(Arc::new(lines)));
        });
        rx
    }

    /// Load a commentary room's display window: the newest `limit` lines,
    /// newest first (upstream `viewcommentary`).
    pub fn load_commentary(
        &self,
        section: &'static str,
        limit: usize,
    ) -> watch::Receiver<CommentaryLoad> {
        let (tx, rx) = watch::channel(CommentaryLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let lines = match inner.db.get().await {
                Ok(client) => read_commentary(&client, section, limit).await,
                Err(e) => {
                    tracing::warn!("greendragon db get failed on commentary read: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send(CommentaryLoad::Ready {
                lines: Arc::new(lines),
                double_post: false,
            });
        });
        rx
    }

    /// Post a prepared line into a room and return its refreshed window. The
    /// double-post check runs here against the section's actual newest row
    /// (upstream `injectcommentary`), not the possibly stale page the player
    /// was reading. Old comments are pruned opportunistically on write.
    pub fn post_commentary(
        &self,
        section: &'static str,
        limit: usize,
        user_id: Uuid,
        name: String,
        body: String,
    ) -> watch::Receiver<CommentaryLoad> {
        let (tx, rx) = watch::channel(CommentaryLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let (lines, double_post) = match inner.db.get().await {
                Ok(client) => {
                    let newest = GreenDragonCommentary::latest(&client, section, 1)
                        .await
                        .unwrap_or_default();
                    let double_post = newest
                        .first()
                        .is_some_and(|r| r.user_id == Some(user_id) && r.body == body);
                    if !double_post {
                        if let Err(e) = GreenDragonCommentary::add(
                            &client,
                            section,
                            Some(user_id),
                            &name,
                            &body,
                        )
                        .await
                        {
                            tracing::warn!("greendragon commentary write failed: {e}");
                        }
                        if let Err(e) = GreenDragonCommentary::prune(&client).await {
                            tracing::warn!("greendragon commentary prune failed: {e}");
                        }
                    }
                    (read_commentary(&client, section, limit).await, double_post)
                }
                Err(e) => {
                    tracing::warn!("greendragon db get failed on commentary write: {e}");
                    (Vec::new(), false)
                }
            };
            let _ = tx.send(CommentaryLoad::Ready {
                lines: Arc::new(lines),
                double_post,
            });
        });
        rx
    }

    /// Read the current Five Sixes jackpot (for the tavern's signboard).
    pub fn load_fivesix_pot(&self) -> watch::Receiver<Option<u64>> {
        let (tx, rx) = watch::channel(None);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            if let Ok(client) = inner.db.get().await
                && let Ok(Some(pot)) = GreenDragonSetting::get(&client, "fivesix_jackpot").await
            {
                let _ = tx.send(Some(pot.max(0) as u64));
            }
        });
        rx
    }

    /// Settle a Five Sixes play (`cost` staked, `sixes` rolled) against the
    /// one shared jackpot, atomically. The caller has already taken the stake
    /// off the character; the receiver reports what the pot paid.
    pub fn settle_fivesix(
        &self,
        cost: u64,
        max_pot: u64,
        sixes: u32,
    ) -> watch::Receiver<FiveSixLoad> {
        let (tx, rx) = watch::channel(FiveSixLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let settled = match inner.db.get().await {
                Ok(client) => {
                    GreenDragonSetting::settle_fivesix(&client, cost as i64, max_pot as i64, sixes)
                        .await
                }
                Err(e) => Err(e),
            };
            let msg = match settled {
                Ok((pot, left_over)) => FiveSixLoad::Ready {
                    pot: pot.max(0) as u64,
                    left_over: left_over.max(0) as u64,
                },
                Err(e) => {
                    tracing::warn!("greendragon fivesix settle failed: {e}");
                    FiveSixLoad::Failed
                }
            };
            let _ = tx.send(msg);
        });
        rx
    }
}
