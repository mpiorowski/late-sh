use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ShowcaseFeedRead {
    pub user_id: Uuid,
    pub last_read_at: Option<DateTime<Utc>>,
}

impl ShowcaseFeedRead {
    pub async fn mark_read_now(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO showcase_feed_reads (user_id, last_read_at, updated)
                 VALUES ($1, current_timestamp, current_timestamp)
                 ON CONFLICT (user_id)
                 DO UPDATE SET
                   last_read_at = EXCLUDED.last_read_at,
                   updated = current_timestamp",
                &[&user_id],
            )
            .await?;

        Ok(())
    }

    pub async fn last_read_at(client: &Client, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "SELECT last_read_at FROM showcase_feed_reads WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|row| row.get("last_read_at")).unwrap_or(None))
    }

    pub async fn unread_count_for_user(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(s.id)::bigint AS unread_count
                 FROM showcases s
                 LEFT JOIN showcase_feed_reads sfr ON sfr.user_id = $1
                 WHERE
                   sfr.user_id IS NULL
                   OR s.created > COALESCE(sfr.last_read_at, '-infinity'::timestamptz)",
                &[&user_id],
            )
            .await?;
        Ok(row.get("unread_count"))
    }
}
