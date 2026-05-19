use anyhow::Result;
use late_core::db::Db;
use late_core::models::cat::CatCompanion;
use uuid::Uuid;

#[derive(Clone)]
pub struct CatService {
    db: Db,
}

impl CatService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn ensure_cat(&self, user_id: Uuid) -> Result<CatCompanion> {
        let client = self.db.get().await?;
        CatCompanion::ensure(&client, user_id).await
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
        CatCompanion::touch_fed(&client, user_id).await
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
        CatCompanion::touch_watered(&client, user_id).await
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
        CatCompanion::touch_played(&client, user_id).await
    }
}
