use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// One News article a reader opened ahead of their cursor
/// (`article_feed_reads`). Unread is newer than the cursor and not here.
#[derive(Debug, Clone)]
pub struct ArticleRead {
    pub user_id: Uuid,
    pub article_id: Uuid,
}

impl ArticleRead {
    /// Mark one article read for one reader. Opening it again is a no-op.
    pub async fn mark_read(client: &Client, user_id: Uuid, article_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_reads (user_id, article_id)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id, article_id) DO NOTHING",
                &[&user_id, &article_id],
            )
            .await?;

        Ok(())
    }

    pub async fn article_ids_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = client
            .query(
                "SELECT article_id FROM article_reads WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(rows.iter().map(|row| row.get("article_id")).collect())
    }

    /// Drop the reads the reader's cursor covers once it moved to
    /// `last_read_at`. A row written at or before the cursor names an
    /// article created before it, so the cursor alone already reads it.
    pub async fn delete_covered(
        client: &Client,
        user_id: Uuid,
        last_read_at: DateTime<Utc>,
    ) -> Result<()> {
        client
            .execute(
                "DELETE FROM article_reads WHERE user_id = $1 AND created <= $2",
                &[&user_id, &last_read_at],
            )
            .await?;

        Ok(())
    }
}
