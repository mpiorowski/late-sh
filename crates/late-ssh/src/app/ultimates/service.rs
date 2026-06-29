use late_core::{
    db::Db,
    models::ultimate_cooldown::{UltimateCastClaim, UltimateCastCooldown, UltimateCooldown},
};

use super::manifest::{ULTIMATE_CAST_COOLDOWN, UltimateKind};

#[derive(Clone)]
pub struct UltimateService {
    db: Db,
}

impl UltimateService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list_cooldowns(
        &self,
        user_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<UltimateCooldown>> {
        let client = self.db.get().await?;
        UltimateCastCooldown::list_remaining(&client, user_id, ULTIMATE_CAST_COOLDOWN).await
    }

    pub async fn try_claim_cast(
        &self,
        user_id: uuid::Uuid,
        kind: UltimateKind,
    ) -> anyhow::Result<UltimateCastClaim> {
        let mut client = self.db.get().await?;
        UltimateCastCooldown::try_record_cast(
            &mut client,
            user_id,
            kind.id(),
            ULTIMATE_CAST_COOLDOWN,
        )
        .await
    }
}
