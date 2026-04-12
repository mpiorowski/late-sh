use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ArticleFeedRead {
    pub user_id: Uuid,
    pub last_read_at: Option<DateTime<Utc>>,
}

impl ArticleFeedRead {
    pub async fn mark_read_now(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_feed_reads (user_id, last_read_at, updated)
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

    pub async fn unread_count_for_user(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(a.id)::bigint AS unread_count
                 FROM articles a
                 LEFT JOIN article_feed_reads afr ON afr.user_id = $1
                 WHERE
                   afr.user_id IS NULL
                   OR a.created > COALESCE(afr.last_read_at, '-infinity'::timestamptz)",
                &[&user_id],
            )
            .await?;
        Ok(row.get("unread_count"))
    }
}
