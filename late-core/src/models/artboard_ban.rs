use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
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

pub struct ArtboardBanListItem {
    pub ban: ArtboardBan,
    pub target_username: Option<String>,
    pub actor_username: Option<String>,
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

    pub async fn active_with_usernames(
        client: &Client,
        limit: i64,
    ) -> Result<Vec<ArtboardBanListItem>> {
        let rows = client
            .query(
                "SELECT ab.*, target.username AS target_username, actor.username AS actor_username
                 FROM artboard_bans ab
                 LEFT JOIN users target ON target.id = ab.target_user_id
                 LEFT JOIN users actor ON actor.id = ab.actor_user_id
                 WHERE ab.expires_at IS NULL OR ab.expires_at > current_timestamp
                 ORDER BY ab.created DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let target_username: Option<String> = row.get("target_username");
                let actor_username: Option<String> = row.get("actor_username");
                ArtboardBanListItem {
                    ban: Self::from(row),
                    target_username,
                    actor_username,
                }
            })
            .collect())
    }

    pub async fn activate(
        client: &impl GenericClient,
        target_user_id: Uuid,
        actor_user_id: Uuid,
        reason: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let reason = reason.into();
        let row = client
            .query_one(
                "INSERT INTO artboard_bans
                 (target_user_id, actor_user_id, reason, expires_at)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (target_user_id)
                 DO UPDATE SET actor_user_id = EXCLUDED.actor_user_id,
                               reason = EXCLUDED.reason,
                               expires_at = EXCLUDED.expires_at,
                               updated = current_timestamp
                 RETURNING *",
                &[&target_user_id, &actor_user_id, &reason, &expires_at],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn delete_for_user(client: &impl GenericClient, target_user_id: Uuid) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM artboard_bans WHERE target_user_id = $1",
                &[&target_user_id],
            )
            .await?)
    }
}
