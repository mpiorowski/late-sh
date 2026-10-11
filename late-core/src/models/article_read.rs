use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use super::article::NEWS_FEED_LIMIT;

/// A News article a reader has read: one `article_reads` row, and nothing
/// else. Unread is a snapshot article without one. Rows are only ever
/// added, so a reader's reads merge in any order.
pub struct ArticleRead;

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

    /// Mark the whole feed read for one reader: a row for every article in
    /// the snapshot, the newest [`NEWS_FEED_LIMIT`]. A visit to the News
    /// room, and the seed for a brand-new user, so the back catalog never
    /// shows as unread on a first login.
    pub async fn mark_feed_read(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_reads (user_id, article_id)
                 SELECT $1, id FROM articles ORDER BY created DESC LIMIT $2
                 ON CONFLICT (user_id, article_id) DO NOTHING",
                &[&user_id, &NEWS_FEED_LIMIT],
            )
            .await?;

        Ok(())
    }

    /// The reader's reads among the snapshot's articles, the newest
    /// [`NEWS_FEED_LIMIT`]: the only ones a badge can count, so a long
    /// reading history never rides a session's load.
    pub async fn article_ids_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = client
            .query(
                "SELECT article_id FROM article_reads
                 WHERE user_id = $1
                   AND article_id IN (SELECT id FROM articles ORDER BY created DESC LIMIT $2)",
                &[&user_id, &NEWS_FEED_LIMIT],
            )
            .await?;
        Ok(rows.iter().map(|row| row.get("article_id")).collect())
    }
}
