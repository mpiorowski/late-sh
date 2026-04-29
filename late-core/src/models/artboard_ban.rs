use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "artboard_bans";
    params = ArtboardBanParams;
    struct ArtboardBan {
        @data
        pub target_user_id: Uuid,
        pub actor_user_id: Uuid,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

impl ArtboardBan {
    pub async fn find_for_user(client: &Client, target_user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM artboard_bans
                 WHERE target_user_id = $1",
                &[&target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_active_for_user(
        client: &Client,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM artboard_bans
                 WHERE target_user_id = $1
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn is_active_for_user(client: &Client, target_user_id: Uuid) -> Result<bool> {
        Ok(Self::find_active_for_user(client, target_user_id)
            .await?
            .is_some())
    }

    pub async fn active_with_actor_username(
        client: &Client,
    ) -> Result<Vec<(Self, Option<String>)>> {
        let rows = client
            .query(
                "SELECT ab.*, u.username AS actor_username
                 FROM artboard_bans ab
                 LEFT JOIN users u ON u.id = ab.actor_user_id
                 WHERE ab.expires_at IS NULL OR ab.expires_at > current_timestamp
                 ORDER BY ab.created DESC",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let actor_username: Option<String> = row.get("actor_username");
                (Self::from(row), actor_username)
            })
            .collect())
    }

    pub async fn activate(
        client: &Client,
        target_user_id: Uuid,
        actor_user_id: Uuid,
        reason: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let params = ArtboardBanParams {
            target_user_id,
            actor_user_id,
            reason: reason.into(),
            expires_at,
        };

        if let Some(existing) = Self::find_for_user(client, target_user_id).await? {
            Self::update(client, existing.id, params).await
        } else {
            Self::create(client, params).await
        }
    }

    pub async fn delete_for_user(client: &Client, target_user_id: Uuid) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM artboard_bans WHERE target_user_id = $1",
                &[&target_user_id],
            )
            .await?)
    }
}
