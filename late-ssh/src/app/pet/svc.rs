use anyhow::Result;
use late_core::db::Db;
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

    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.feed(user_id).await {
                tracing::error!(error = ?e, "failed to feed cat");
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::touch_fed(&client, user_id).await
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

    pub fn play_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.play(user_id).await {
                tracing::error!(error = ?e, "failed to play with cat");
            }
        });
    }

    async fn play(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        PetCompanion::touch_played(&client, user_id).await
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
