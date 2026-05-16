use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "media_queue_items";
    params = MediaQueueItemParams;
    struct MediaQueueItem {
        @data
        pub submitter_id: Uuid,
        pub media_kind: String,
        pub external_id: String,
        pub title: Option<String>,
        pub channel: Option<String>,
        pub duration_ms: Option<i32>,
        pub is_stream: bool,
        pub status: String,
        pub started_at: Option<DateTime<Utc>>,
        pub ended_at: Option<DateTime<Utc>>,
        pub error: Option<String>,
    }
}

impl MediaQueueItem {
    pub const STATUS_QUEUED: &'static str = "queued";
    pub const STATUS_PLAYING: &'static str = "playing";
    pub const STATUS_PLAYED: &'static str = "played";
    pub const STATUS_SKIPPED: &'static str = "skipped";
    pub const STATUS_FAILED: &'static str = "failed";
    pub const KIND_YOUTUBE: &'static str = "youtube";

    pub async fn insert_youtube(
        client: &Client,
        submitter_id: Uuid,
        external_id: &str,
        title: Option<&str>,
        channel: Option<&str>,
        duration_ms: Option<i32>,
        is_stream: bool,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO media_queue_items
                    (submitter_id, media_kind, external_id, title, channel,
                     duration_ms, is_stream, status)
                 VALUES ($1, 'youtube', $2, $3, $4, $5, $6, 'queued')
                 RETURNING *",
                &[
                    &submitter_id,
                    &external_id,
                    &title,
                    &channel,
                    &duration_ms,
                    &is_stream,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn find_by_id(client: &Client, id: Uuid) -> Result<Option<Self>> {
        Self::get(client, id).await
    }

    pub async fn list_snapshot(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM media_queue_items
                 WHERE status IN ('queued', 'playing')
                 ORDER BY
                    CASE status WHEN 'playing' THEN 0 ELSE 1 END,
                    created
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn queued_before_count(client: &Client, created: DateTime<Utc>) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint FROM media_queue_items
                 WHERE status = 'queued' AND created < $1",
                &[&created],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn recent_submission_count(
        client: &Client,
        submitter_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint FROM media_queue_items
                 WHERE submitter_id = $1 AND created >= $2",
                &[&submitter_id, &since],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn first_queued(client: &Client) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM media_queue_items
                 WHERE status = 'queued'
                 ORDER BY created
                 LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn current_playing(client: &Client) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM media_queue_items
                 WHERE status = 'playing'
                 ORDER BY started_at NULLS LAST, created
                 LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn mark_playing(
        client: &Client,
        id: Uuid,
        started_at: DateTime<Utc>,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE media_queue_items
                 SET status = 'playing',
                     started_at = $2,
                     ended_at = NULL,
                     error = NULL,
                     updated = current_timestamp
                 WHERE id = $1 AND status = 'queued'
                 RETURNING *",
                &[&id, &started_at],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn sweep_orphan_playing(client: &Client, older_than: DateTime<Utc>) -> Result<u64> {
        let rows = client
            .execute(
                "UPDATE media_queue_items
                 SET status = 'failed',
                     error = 'orphan playing row swept at startup',
                     ended_at = current_timestamp,
                     updated = current_timestamp
                 WHERE status = 'playing'
                   AND (started_at IS NULL OR started_at < $1)",
                &[&older_than],
            )
            .await?;
        Ok(rows)
    }

    pub async fn update_status(client: &Client, id: Uuid, status: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE media_queue_items
                 SET status = $2, updated = current_timestamp
                 WHERE id = $1
                 RETURNING *",
                &[&id, &status],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn set_duration_if_missing(
        client: &Client,
        id: Uuid,
        duration_ms: i32,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE media_queue_items
                 SET duration_ms = $2,
                     updated = current_timestamp
                 WHERE id = $1
                   AND duration_ms IS NULL
                 RETURNING *",
                &[&id, &duration_ms],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn mark_played(client: &Client, id: Uuid, ended_at: DateTime<Utc>) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE media_queue_items
                 SET status = 'played',
                     ended_at = $2,
                     updated = current_timestamp
                 WHERE id = $1 AND status = 'playing'",
                &[&id, &ended_at],
            )
            .await?;
        Ok(count)
    }

    pub async fn mark_failed(
        client: &Client,
        id: Uuid,
        ended_at: DateTime<Utc>,
        error: &str,
    ) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE media_queue_items
                 SET status = 'failed',
                     ended_at = $2,
                     error = $3,
                     updated = current_timestamp
                 WHERE id = $1 AND status = 'playing'",
                &[&id, &ended_at, &error],
            )
            .await?;
        Ok(count)
    }
}
