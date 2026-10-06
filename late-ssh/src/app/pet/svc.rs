use anyhow::Result;
use chrono::{NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::chips::{ChipMove, UserChips};
use late_core::models::marketplace;
use late_core::models::pet::{PetCompanion, PetMood, PetSpecies};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;

/// What the first pet of the UTC day pays.
pub(crate) const PET_CHIP_BONUS: i64 = 100;

/// What a petting came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetOutcome {
    /// The first pet of the day: the chips are paid.
    Petted,
    AlreadyPettedToday,
    /// The account never bought the pet.
    NoPet,
}

/// Persistence for the pet. `PetState` owns the mood rules; this writes
/// what it is handed, plus the one thing the pet pays: the first pet of the
/// day credits `PET_CHIP_BONUS` and tells the session on the activity feed.
#[derive(Clone)]
pub struct PetService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl PetService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    /// Persist a petting. The ownership check, the daily gate, and the chip
    /// credit commit in one transaction; the event that tells the session
    /// follows only when the gate said this was the first pet of the day.
    pub fn pet_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.pet_on(user_id, Utc::now().date_naive()).await {
                Ok(PetOutcome::Petted) | Ok(PetOutcome::AlreadyPettedToday) => {}
                Ok(PetOutcome::NoPet) => {
                    tracing::warn!(user_id = %user_id, "pet petted without a pet companion");
                }
                Err(e) => {
                    tracing::error!(error = ?e, user_id = %user_id, "failed to pet the pet");
                }
            }
        });
    }

    async fn pet_on(&self, user_id: Uuid, today: NaiveDate) -> Result<PetOutcome> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        if !marketplace::user_owns_pet_companion(&*tx, user_id).await? {
            tx.commit().await?;
            return Ok(PetOutcome::NoPet);
        }
        if !PetCompanion::pet_day(&*tx, user_id, today).await? {
            tx.commit().await?;
            return Ok(PetOutcome::AlreadyPettedToday);
        }
        UserChips::apply(
            &*tx,
            user_id,
            ChipMove::PetPetted,
            PET_CHIP_BONUS,
            &today.to_string(),
        )
        .await?;
        tx.commit().await?;

        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self
            .activity_feed
            .send(ActivityEvent::pet_petted(user_id, username));
        Ok(PetOutcome::Petted)
    }

    pub async fn ensure_pet(&self, user_id: Uuid) -> Result<PetCompanion> {
        let client = self.db.get().await?;
        PetCompanion::ensure(&client, user_id).await
    }

    pub fn set_name_task(&self, user_id: Uuid, name: Option<String>) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_name(user_id, name.as_deref()).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to set pet name");
            }
        });
    }

    async fn set_name(&self, user_id: Uuid, name: Option<&str>) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_name(&client, user_id, name).await
    }

    pub fn set_species_task(&self, user_id: Uuid, species: PetSpecies) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_species(user_id, species).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to set pet species");
            }
        });
    }

    async fn set_species(&self, user_id: Uuid, species: PetSpecies) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_species(&client, user_id, species).await
    }

    /// Record the mood the session inferred, so a profile can show it. Fired
    /// on every mood change and once more, `Asleep`, when the session ends.
    pub fn set_mood_task(&self, user_id: Uuid, mood: PetMood) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.set_mood(user_id, mood).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to set pet mood");
            }
        });
    }

    async fn set_mood(&self, user_id: Uuid, mood: PetMood) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::set_mood(&client, user_id, mood).await
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
