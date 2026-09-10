use anyhow::Result;
use late_core::db::Db;
use late_core::models::{
    chips::{ChipMove, UserChips},
    pet::PetCompanion,
};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;

pub(crate) const FEED_CHIP_BONUS: i64 = 100;

/// Persistence and side effects for the pet. `PetState` owns every rule;
/// this writes what it is handed and pays the daily chips exactly once.
#[derive(Clone)]
pub struct PetService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl PetService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    pub async fn ensure_cat(&self, user_id: Uuid) -> Result<PetCompanion> {
        let client = self.db.get().await?;
        PetCompanion::ensure(&client, user_id).await
    }

    /// Persist a feeding. The daily gate and the chip credit commit in one
    /// transaction, so they succeed or fail together; the activity event
    /// follows only when the gate said this was the first feed of the day.
    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.feed(user_id).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to feed pet");
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<()> {
        let mut client = self.db.get().await?;
        let today = chrono::Utc::now().date_naive();
        let tx = client.transaction().await?;
        let first_today = PetCompanion::feed_day(&*tx, user_id, today).await?;
        if first_today {
            UserChips::apply(
                &*tx,
                user_id,
                ChipMove::PetFed,
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
            .send(ActivityEvent::pet_fed(user_id, username));
        Ok(())
    }

    pub fn set_name_task(&self, user_id: Uuid, name: Option<String>) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_name(user_id, name.as_deref()).await {
                tracing::error!(error = ?e, "failed to set cat name");
            }
        });
    }

    async fn set_name(&self, user_id: Uuid, name: Option<&str>) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_name(&client, user_id, name).await
    }

    pub fn set_species_task(&self, user_id: Uuid, species: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_species(user_id, &species).await {
                tracing::error!(error = ?e, "failed to set pet species");
            }
        });
    }

    async fn set_species(&self, user_id: Uuid, species: &str) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_species(&client, user_id, species).await
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
