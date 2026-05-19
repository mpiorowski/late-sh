use anyhow::Result;
use late_core::db::Db;
use late_core::models::goldfish::GoldfishBowl;
use uuid::Uuid;

#[derive(Clone)]
pub struct GoldfishService {
    db: Db,
}

impl GoldfishService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn ensure_bowl(&self, user_id: Uuid) -> Result<GoldfishBowl> {
        let client = self.db.get().await?;
        GoldfishBowl::ensure(&client, user_id).await
    }

    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.feed(user_id).await {
                tracing::error!(error = ?e, "failed to feed goldfish");
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        GoldfishBowl::touch_fed(&client, user_id).await
    }

    pub fn decorate_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.decorate(user_id).await {
                tracing::error!(error = ?e, "failed to decorate goldfish bowl");
            }
        });
    }

    async fn decorate(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        GoldfishBowl::touch_decorated(&client, user_id).await
    }

    pub fn light_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.light(user_id).await {
                tracing::error!(error = ?e, "failed to adjust goldfish lights");
            }
        });
    }

    async fn light(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        GoldfishBowl::touch_lit(&client, user_id).await
    }

    pub fn change_water_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.change_water(user_id).await {
                tracing::error!(error = ?e, "failed to change goldfish water");
            }
        });
    }

    async fn change_water(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        GoldfishBowl::touch_water_changed(&client, user_id).await
    }

    pub fn add_friend_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.add_friend(user_id).await {
                tracing::error!(error = ?e, "failed to add goldfish friend");
            }
        });
    }

    async fn add_friend(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        GoldfishBowl::add_friend(&client, user_id).await
    }
}
