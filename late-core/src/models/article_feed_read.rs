use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ArticleFeedRead {
    pub user_id: Uuid,
    pub last_read_created: Option<DateTime<Utc>>,
    pub last_read_article_id: Option<Uuid>,
}

impl ArticleFeedRead {
    pub async fn mark_read_latest(client: &Client, user_id: Uuid) -> Result<()> {
        let latest = client
            .query_opt(
                "SELECT created, id
                 FROM articles
                 ORDER BY created DESC, id DESC
                 LIMIT 1",
                &[],
            )
            .await?;

        let (last_read_created, last_read_article_id): (Option<DateTime<Utc>>, Option<Uuid>) =
            if let Some(row) = latest {
                (Some(row.get("created")), Some(row.get("id")))
            } else {
                (None, None)
            };

        client
            .execute(
                "INSERT INTO article_feed_reads (user_id, last_read_created, last_read_article_id, updated)
                 VALUES ($1, $2, $3, current_timestamp)
                 ON CONFLICT (user_id)
                 DO UPDATE SET
                   last_read_created = EXCLUDED.last_read_created,
                   last_read_article_id = EXCLUDED.last_read_article_id,
                   updated = current_timestamp",
                &[&user_id, &last_read_created, &last_read_article_id],
            )
            .await?;

        Ok(())
    }

    /// Call this immediately before deleting an article. Shifts every
    /// affected user's checkpoint back to the next-older article so they
    /// stay "caught up", or nulls the pair if there is no older article.
    ///
    /// Needed because the FK's `ON DELETE SET NULL` would otherwise leave
    /// `article_feed_reads` in the half-null state that
    /// `article_feed_reads_checkpoint_chk` forbids.
    pub async fn repoint_checkpoint_before_article_delete(
        client: &Client,
        article_id: Uuid,
    ) -> Result<()> {
        // Shift affected checkpoints to the next-older article.
        client
            .execute(
                "UPDATE article_feed_reads AS afr
                 SET last_read_created = prev.created,
                     last_read_article_id = prev.id,
                     updated = current_timestamp
                 FROM articles AS deleted_a,
                      LATERAL (
                          SELECT a.created, a.id
                          FROM articles a
                          WHERE (a.created, a.id) < (deleted_a.created, deleted_a.id)
                          ORDER BY a.created DESC, a.id DESC
                          LIMIT 1
                      ) AS prev
                 WHERE deleted_a.id = $1
                   AND afr.last_read_article_id = $1",
                &[&article_id],
            )
            .await?;

        // Anything still pointing at the deleted article has no predecessor
        // — null both columns to satisfy the checkpoint check constraint.
        client
            .execute(
                "UPDATE article_feed_reads
                 SET last_read_created = NULL,
                     last_read_article_id = NULL,
                     updated = current_timestamp
                 WHERE last_read_article_id = $1",
                &[&article_id],
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
                   OR (a.created, a.id) > (
                        COALESCE(afr.last_read_created, '-infinity'::timestamptz),
                        COALESCE(afr.last_read_article_id, '00000000-0000-0000-0000-000000000000'::uuid)
                   )",
                &[&user_id],
            )
            .await?;
        Ok(row.get("unread_count"))
    }
}
