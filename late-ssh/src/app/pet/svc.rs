use anyhow::Result;
use late_core::db::Db;
use late_core::models::pet::{PetCompanion, PetMood, PetSpecies};
use uuid::Uuid;

/// Persistence for the pet. `PetState` owns every rule; this writes what it
/// is handed. There is nothing to pay and nothing to announce: the pet has
/// no care and no chips, only a mood the profile can read.
#[derive(Clone)]
pub struct PetService {
    db: Db,
}

impl PetService {
    pub fn new(db: Db) -> Self {
        Self { db }
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
