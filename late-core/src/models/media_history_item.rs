use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::media_queue_item::MediaQueueItem;

crate::model! {
    table = "media_history_items";
    params = MediaHistoryItemParams;
    struct MediaHistoryItem {
        @data
        pub media_kind: String,
        pub external_id: String,
        pub title: Option<String>,
        pub channel: Option<String>,
        pub duration_ms: Option<i32>,
        pub is_stream: bool,
        pub first_played_at: DateTime<Utc>,
        pub last_played_at: DateTime<Utc>,
        pub play_count: i32,
        pub last_submitter_id: Option<Uuid>,
    }
}

impl MediaHistoryItem {
    pub async fn find_by_id(client: &Client, id: Uuid) -> Result<Option<Self>> {
        Self::get(client, id).await
    }

    pub async fn delete_by_id(client: &Client, id: Uuid) -> Result<u64> {
        let count = client
            .execute("DELETE FROM media_history_items WHERE id = $1", &[&id])
            .await?;
        Ok(count)
    }

    pub async fn record_play_from_queue_item(
        client: &Client,
        item: &MediaQueueItem,
        limit: i64,
    ) -> Result<()> {
        let existing = client
            .query_opt(
                "SELECT * FROM media_history_items
                 WHERE media_kind = $1 AND external_id = $2",
                &[&item.media_kind, &item.external_id],
            )
            .await?;

        if let Some(row) = existing {
            let history_item = Self::from(row);
            client
                .execute(
                    "UPDATE media_history_items
                     SET title = COALESCE($2, title),
                         channel = COALESCE($3, channel),
                         duration_ms = COALESCE($4, duration_ms),
                         is_stream = $5,
                         last_played_at = current_timestamp,
                         play_count = play_count + 1,
                         last_submitter_id = $6,
                         updated = current_timestamp
                     WHERE id = $1",
                    &[
                        &history_item.id,
                        &item.title,
                        &item.channel,
                        &item.duration_ms,
                        &item.is_stream,
                        &item.submitter_id,
                    ],
                )
                .await?;
        } else {
            let row = client
                .query_one(
                    "INSERT INTO media_history_items
                        (media_kind, external_id, title, channel, duration_ms,
                         is_stream, last_submitter_id)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)
                     RETURNING *",
                    &[
                        &item.media_kind,
                        &item.external_id,
                        &item.title,
                        &item.channel,
                        &item.duration_ms,
                        &item.is_stream,
                        &item.submitter_id,
                    ],
                )
                .await?;
            let history_item = Self::from(row);
            client
                .execute(
                    "INSERT INTO media_history_votes (item_id, user_id, value, created, updated)
                     SELECT $1, user_id, value, created, updated
                     FROM media_queue_votes
                     WHERE item_id = $2
                     ON CONFLICT (user_id, item_id) DO NOTHING",
                    &[&history_item.id, &item.id],
                )
                .await?;
        }

        Self::prune_to_limit(client, limit).await?;
        Ok(())
    }

    pub async fn list_ranked(client: &Client, limit: i64) -> Result<Vec<(Self, i32)>> {
        let rows = client
            .query(
                "SELECT mhi.*, COALESCE(SUM(mhv.value), 0)::int AS vote_score
                 FROM media_history_items mhi
                 LEFT JOIN media_history_votes mhv ON mhv.item_id = mhi.id
                 GROUP BY mhi.id
                 ORDER BY vote_score DESC, mhi.last_played_at DESC, mhi.created DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let score: i32 = row.get("vote_score");
                (Self::from(row), score)
            })
            .collect())
    }

    pub async fn prune_to_limit(client: &Client, limit: i64) -> Result<u64> {
        let deleted = client
            .execute(
                "WITH ranked AS (
                    SELECT mhi.id,
                           row_number() OVER (
                               ORDER BY COALESCE(SUM(mhv.value), 0)::int DESC,
                                        mhi.last_played_at DESC,
                                        mhi.created DESC
                           ) AS rank
                    FROM media_history_items mhi
                    LEFT JOIN media_history_votes mhv ON mhv.item_id = mhi.id
                    GROUP BY mhi.id
                 )
                 DELETE FROM media_history_items
                 WHERE id IN (SELECT id FROM ranked WHERE rank > $1)",
                &[&limit],
            )
            .await?;
        Ok(deleted)
    }
}
