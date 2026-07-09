use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::marketplace::{ConsumableUseStatus, consume_pet_food};
use late_core::models::pet::PetCompanion;
use uuid::Uuid;

#[derive(Clone)]
pub struct PetService {
    db: Db,
}

impl PetService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn ensure_cat(&self, user_id: Uuid) -> Result<PetCompanion> {
        let client = self.db.get().await?;
        PetCompanion::ensure(&client, user_id).await
    }

    /// One transaction spends a pet food and stamps `last_fed`, so a
    /// double-click cannot spend two meals for one stamp.
    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.feed(user_id).await {
                Ok(ConsumableUseStatus::Used) => {}
                Ok(status) => {
                    tracing::warn!(?status, user_id = %user_id, "pet was not fed");
                }
                Err(e) => {
                    tracing::error!(error = ?e, "failed to feed pet");
                }
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<ConsumableUseStatus> {
        let mut client = self.db.get().await?;
        let result = consume_pet_food(&mut client, user_id).await?;
        Ok(result.status)
    }

    pub fn water_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.water(user_id).await {
                tracing::error!(error = ?e, "failed to water cat");
            }
        });
    }

    async fn water(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::touch_watered(&client, user_id).await
    }

    pub fn record_care_completed_task(&self, user_id: Uuid, care_date: NaiveDate) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.record_care_completed(user_id, care_date).await {
                tracing::error!(error = ?e, "failed to record pet care streak");
            }
        });
    }

    async fn record_care_completed(&self, user_id: Uuid, care_date: NaiveDate) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::record_care_completed(&client, user_id, care_date).await
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
