//! Persistence and side effects for the tank's care. `AquariumCare` owns
//! the rule (one meal a day); this writes the stamp and pays the daily chips
//! exactly once.

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::db::Db;
use late_core::models::{
    aquarium_care::AquariumCare,
    chips::{ChipMove, UserChips},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;

pub(crate) const FEED_CHIP_BONUS: i64 = 100;

#[derive(Clone)]
pub struct AquariumService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl AquariumService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    /// When the tank was last fed, for the session's care state at login.
    pub async fn last_fed(&self, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        let client = self.db.get().await?;
        AquariumCare::last_fed(&client, user_id).await
    }

    /// Persist a feeding. The daily gate and the chip credit commit in one
    /// transaction, so they succeed or fail together; the activity event
    /// follows only when the gate said this was the first feed of the day.
    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.feed(user_id).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to feed aquarium");
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<()> {
        let mut client = self.db.get().await?;
        let today = Utc::now().date_naive();
        let tx = client.transaction().await?;
        let first_today = AquariumCare::feed_day(&*tx, user_id, today).await?;
        if first_today {
            UserChips::apply(
                &*tx,
                user_id,
                ChipMove::AquariumFed,
                FEED_CHIP_BONUS,
                &today.to_string(),
            )
            .await?;
        }
        tx.commit().await?;
        if !first_today {
            return Ok(());
        }
        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self
            .activity_feed
            .send(ActivityEvent::aquarium_fed(user_id, username));
        Ok(())
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
