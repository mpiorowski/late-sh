//! Legend of the Green Dragon service: thin persistence + reward plumbing for
//! the single-player door. Unlike Lateania there is no shared world, no tick
//! loop, and no watch-published world snapshot — each session owns the
//! authoritative character in its own `state::State`. This service only loads
//! the character once (off the DB) and saves blobs back, fire-and-forget.
//!
//! Cheap to `Clone`: everything lives behind an `Arc`.

use std::sync::Arc;

use chrono::Utc;
use late_core::{db::Db, models::greendragon_character::GreenDragonCharacter};
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::{activity::publisher::ActivityPublisher, games::chips::svc::ChipService};

use super::model::Character;
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
    // Held for the forthcoming dragon-kill reward path (chip payout + activity
    // feed entry), mirroring Lateania's milestone awards. Not yet wired.
    #[allow(dead_code)]
    activity: ActivityPublisher,
    #[allow(dead_code)]
    chips: ChipService,
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
        let db = self.inner.db.clone();
        tokio::spawn(async move {
            let day = today();
            let mut character = match db.get().await {
                Ok(client) => match GreenDragonCharacter::load(&client, user_id).await {
                    Ok(Some(blob)) => persist::from_json(&blob),
                    Ok(None) => Character::new(name, day),
                    Err(e) => {
                        tracing::warn!("greendragon character load failed: {e}");
                        Character::new(name, day)
                    }
                },
                Err(e) => {
                    tracing::warn!("greendragon db get failed on load: {e}");
                    Character::new(name, day)
                }
            };
            // Refill forest turns / heal / revive if a new day has rolled over
            // since the last save.
            character.roll_new_day(day, 0);
            let _ = tx.send(CharacterLoad::Ready(Box::new(character)));
        });
        rx
    }

    /// Persist a character blob, fire-and-forget.
    pub fn save_character(&self, user_id: Uuid, character: &Character) {
        let db = self.inner.db.clone();
        let blob = persist::to_json(character);
        tokio::spawn(async move {
            match db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonCharacter::save(&client, user_id, blob).await {
                        tracing::warn!("greendragon character save failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("greendragon db get failed on save: {e}"),
            }
        });
    }

    /// Delete a user's saved character, fire-and-forget (the "start over"
    /// action).
    pub fn delete_character(&self, user_id: Uuid) {
        let db = self.inner.db.clone();
        tokio::spawn(async move {
            match db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonCharacter::delete_by_user_id(&client, user_id).await {
                        tracing::warn!("greendragon character delete failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("greendragon db get failed on delete: {e}"),
            }
        });
    }
}
