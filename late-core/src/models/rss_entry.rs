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
    /// Returns `Some(entry)` only when a brand-new row was inserted; existing
    /// rows are silently refreshed (title/summary/published_at) so parser
    /// improvements heal previously-stored junk on the next poll. A poll mostly
    /// re-reads the same items, so a row that would not change is left alone:
    /// rewriting them all was a fifth of the database's WAL.
    ///
    /// An unknown item published before the feed's `keep_per_feed` newest rows
    /// is not inserted either: `prune_past_keep` took it once, and a fresh row
    /// would only come back unread, lose its dismissal, and be pruned again.
    /// An item with no publish date counts as published now, so it always
    /// lands.
    pub async fn upsert_for_feed(
        client: &Client,
        params: RssEntryParams,
        keep_per_feed: i64,
    ) -> Result<Option<Self>> {
        let known = client
            .query_one(
                "WITH existing AS (
                     SELECT id, title, summary, published_at
                     FROM rss_entries
                     WHERE feed_id = $1 AND guid = $2
                 ), refreshed AS (
                     UPDATE rss_entries e
                     SET title = $3,
                         summary = $4,
                         published_at = $5,
                         updated = current_timestamp
                     FROM existing x
                     WHERE e.id = x.id
                       AND (x.title, x.summary, x.published_at)
                           IS DISTINCT FROM ($3::text, $4::text, $5::timestamptz)
                 )
                 SELECT EXISTS (SELECT 1 FROM existing) AS known",
                &[
                    &params.feed_id,
                    &params.guid,
                    &params.title,
                    &params.summary,
                    &params.published_at,
                ],
            )
            .await?;
        if known.get("known") {
            return Ok(None);
        }
        let row = client
            .query_opt(
                "INSERT INTO rss_entries
                    (feed_id, user_id, guid, url, title, summary, published_at, shared_at, dismissed_at)
                 SELECT $1, $2, $3, $4, $5, $6, $7, NULL, NULL
                 WHERE NOT EXISTS (
                     SELECT 1 FROM (
                         SELECT COALESCE(published_at, created) AS seen_at
                         FROM rss_entries
                         WHERE feed_id = $1
                         ORDER BY COALESCE(published_at, created) DESC, created DESC, id DESC
                         OFFSET $8::bigint - 1
                         LIMIT 1
                     ) oldest_kept
                     WHERE $7::timestamptz < oldest_kept.seen_at
                 )
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
                    &keep_per_feed,
                ],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Delete up to `batch` entries past the newest `keep_per_feed` of their
    /// feed, newest by publish date (creation when the feed gave none, the
    /// order the inbox shows), returning how many went. Across every feed on
    /// purpose: the poller skips feeds whose owner is gone, and those hold
    /// most of the rows. A retention sweep, so it has no owner to scope to.
    pub async fn prune_past_keep(client: &Client, keep_per_feed: i64, batch: i64) -> Result<u64> {
        let deleted = client
            .execute(
                "DELETE FROM rss_entries
                 WHERE id IN (
                     SELECT id FROM (
                         SELECT id,
                                ROW_NUMBER() OVER (
                                    PARTITION BY feed_id
                                    ORDER BY COALESCE(published_at, created) DESC,
                                             created DESC,
                                             id DESC
                                ) AS feed_rank
                         FROM rss_entries
                     ) ranked
                     WHERE feed_rank > $1
                     LIMIT $2
                 )",
                &[&keep_per_feed, &batch],
            )
            .await?;
        Ok(deleted)
    }

    /// List the newest non-dismissed entries across all of a user's feeds,
    /// capped at `per_feed_limit` per feed so one high-volume feed (a news
    /// site posting dozens of items a day) cannot evict low-volume feeds
    /// (weekly digests, release blogs) from the flat `limit`-sized window.
    pub async fn list_visible_for_user(
        client: &Client,
        user_id: Uuid,
        limit: i64,
        per_feed_limit: i64,
    ) -> Result<Vec<RssEntryView>> {
        let rows = client
            .query(
                "SELECT * FROM (
                     SELECT e.*, f.title AS feed_title, f.url AS feed_url,
                            ROW_NUMBER() OVER (
                                PARTITION BY e.feed_id
                                ORDER BY COALESCE(e.published_at, e.created) DESC,
                                         e.created DESC
                            ) AS feed_rank
                     FROM rss_entries e
                     JOIN rss_feeds f ON f.id = e.feed_id
                     WHERE e.user_id = $1
                       AND e.dismissed_at IS NULL
                 ) ranked
                 WHERE feed_rank <= $3
                 ORDER BY COALESCE(published_at, created) DESC, created DESC
                 LIMIT $2",
                &[&user_id, &limit, &per_feed_limit],
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
