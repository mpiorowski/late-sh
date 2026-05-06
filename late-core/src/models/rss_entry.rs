use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "rss_entries";
    params = RssEntryParams;
    struct RssEntry {
        @data
        pub feed_id: Uuid,
        pub user_id: Uuid,
        pub guid: String,
        pub url: String,
        pub title: String,
        pub summary: String,
        pub published_at: Option<DateTime<Utc>>,
        pub shared_at: Option<DateTime<Utc>>,
        pub dismissed_at: Option<DateTime<Utc>>,
    }
}

#[derive(Clone)]
pub struct RssEntryView {
    pub entry: RssEntry,
    pub feed_title: String,
    pub feed_url: String,
}

impl RssEntry {
    pub async fn upsert_for_feed(client: &Client, params: RssEntryParams) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "INSERT INTO rss_entries
                    (feed_id, user_id, guid, url, title, summary, published_at, shared_at, dismissed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, NULL)
                 ON CONFLICT DO NOTHING
                 RETURNING *",
                &[
                    &params.feed_id,
                    &params.user_id,
                    &params.guid,
                    &params.url,
                    &params.title,
                    &params.summary,
                    &params.published_at,
                ],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list_pending_for_user(
        client: &Client,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RssEntryView>> {
        let rows = client
            .query(
                "SELECT e.*, f.title AS feed_title, f.url AS feed_url
                 FROM rss_entries e
                 JOIN rss_feeds f ON f.id = e.feed_id
                 WHERE e.user_id = $1
                   AND e.shared_at IS NULL
                   AND e.dismissed_at IS NULL
                 ORDER BY COALESCE(e.published_at, e.created) DESC, e.created DESC
                 LIMIT $2",
                &[&user_id, &limit],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| RssEntryView {
                feed_title: row.get("feed_title"),
                feed_url: row.get("feed_url"),
                entry: Self::from(row),
            })
            .collect())
    }

    pub async fn unread_count_for_user(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint
                 FROM rss_entries
                 WHERE user_id = $1
                   AND shared_at IS NULL
                   AND dismissed_at IS NULL",
                &[&user_id],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn mark_shared(client: &Client, user_id: Uuid, id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE rss_entries
                 SET shared_at = current_timestamp, updated = current_timestamp
                 WHERE user_id = $1 AND id = $2",
                &[&user_id, &id],
            )
            .await?;
        Ok(count)
    }

    pub async fn dismiss(client: &Client, user_id: Uuid, id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE rss_entries
                 SET dismissed_at = current_timestamp, updated = current_timestamp
                 WHERE user_id = $1 AND id = $2",
                &[&user_id, &id],
            )
            .await?;
        Ok(count)
    }
}
