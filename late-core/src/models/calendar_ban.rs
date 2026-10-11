//! A ban from posting to the events board. Shaped like the artboard ban: one
//! row per target, overwritten on a repeat ban, checked inside the post
//! transaction (`CalendarStore::save`). Personal events are never touched.
use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "calendar_bans";
    params = CalendarBanParams;
    struct CalendarBan {
        @data
        pub target_user_id: Uuid,
        pub actor_user_id: Uuid,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

pub struct CalendarBanListItem {
    pub ban: CalendarBan,
    pub target_username: Option<String>,
    pub actor_username: Option<String>,
}

impl CalendarBan {
    pub async fn find_active_for_user(
        client: &Client,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM calendar_bans
                 WHERE target_user_id = $1
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn active_with_usernames_page(
        client: &Client,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CalendarBanListItem>> {
        let rows = client
            .query(
                "SELECT cb.*, target.username AS target_username, actor.username AS actor_username
                 FROM calendar_bans cb
                 LEFT JOIN users target ON target.id = cb.target_user_id
                 LEFT JOIN users actor ON actor.id = cb.actor_user_id
                 WHERE cb.expires_at IS NULL OR cb.expires_at > current_timestamp
                 ORDER BY cb.created DESC
                 LIMIT $1 OFFSET $2",
                &[&limit, &offset],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let target_username: Option<String> = row.get("target_username");
                let actor_username: Option<String> = row.get("actor_username");
                CalendarBanListItem {
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
                "INSERT INTO calendar_bans
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
                "DELETE FROM calendar_bans WHERE target_user_id = $1",
                &[&target_user_id],
            )
            .await?)
    }
}
