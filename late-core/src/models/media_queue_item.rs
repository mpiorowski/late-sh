use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use crate::models::chips::{ChipMove, UserChips};

/// What queueing one track pays the person who brought it. Every submission
/// path funnels through [`MediaQueueItem::insert_youtube`], so a pasted URL,
/// a booth submit, and a re-queue from history all pay exactly this, and none
/// of them can pay a different amount.
///
/// What the track is does not enter into it. A song somebody played last
/// night is still worth putting on now, and a jukebox that only pays for
/// music nobody has heard is a jukebox that argues with the person filling
/// it. The day's cap is the only gate.
pub const SONG_QUEUE_REWARD_CHIPS: i64 = 200;

/// How many tracks are paid per person per UTC day, and the whole of the
/// reward's gating: nothing else looks at what was queued or by whom. The
/// queue already limits submissions to ten every five minutes, which is a
/// rate limit for the room's sake and would be a chip printer if it were also
/// the reward's only gate. Five a day is "bring the good ones"; tracks past
/// the cap still queue, they just mint nothing.
pub const SONG_QUEUE_MAX_PAID_PER_DAY: i64 = 5;

/// What one submission actually minted, decided by
/// [`MediaQueueItem::insert_youtube`]. The banner and the metric read this,
/// never the constant, so an unpaid submission can never be reported as paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongQueueReward {
    /// [`SONG_QUEUE_REWARD_CHIPS`] were credited.
    Paid,
    /// This person has already been paid [`SONG_QUEUE_MAX_PAID_PER_DAY`]
    /// times today (UTC); nothing minted.
    DailyCapReached,
}

impl SongQueueReward {
    pub fn chips(self) -> i64 {
        match self {
            Self::Paid => SONG_QUEUE_REWARD_CHIPS,
            Self::DailyCapReached => 0,
        }
    }
}

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
        pub unskippable: bool,
    }
}

impl MediaQueueItem {
    pub const STATUS_QUEUED: &'static str = "queued";
    pub const STATUS_PLAYING: &'static str = "playing";
    pub const STATUS_PLAYED: &'static str = "played";
    pub const STATUS_SKIPPED: &'static str = "skipped";
    pub const STATUS_FAILED: &'static str = "failed";
    pub const KIND_YOUTUBE: &'static str = "youtube";

    /// Queue a track and pay the person who brought it
    /// [`SONG_QUEUE_REWARD_CHIPS`], for the first
    /// [`SONG_QUEUE_MAX_PAID_PER_DAY`] tracks they queue in a UTC day.
    /// Returns the queue item and what the submission actually minted.
    ///
    /// Every submission pays: the same track twice, a re-queue from history,
    /// one somebody else brought an hour ago. The day's count is the only
    /// question asked, which is what keeps the answer to "why did that one not
    /// pay?" a single number the guide can state. `source_ref` is the video id
    /// as provenance, so a ledger row says which track it paid for; nothing
    /// reads it back.
    ///
    /// Insert, lookup, and credit are one transaction under a per-user
    /// advisory lock, like every other claim-plus-credit path: a credit that
    /// fails leaves no orphan row in the queue, and two submissions by one
    /// person landing together cannot both read the same count and pay a
    /// sixth. This is the only path a submission may take, so the reward
    /// cannot be forgotten at one call site and applied at another.
    pub async fn insert_youtube(
        client: &mut Client,
        submitter_id: Uuid,
        external_id: &str,
        title: Option<&str>,
        channel: Option<&str>,
        duration_ms: Option<i32>,
        is_stream: bool,
    ) -> Result<(Self, SongQueueReward)> {
        let tx = client.transaction().await?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(
               hashtextextended(concat_ws(':', 'song_queued', ($1::uuid)::text), 0)
             )",
            &[&submitter_id],
        )
        .await?;
        let row = tx
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
        let item = Self::from(row);
        let row = tx
            .query_one(
                "SELECT COUNT(*)::BIGINT AS paid_today
                 FROM chip_ledger
                 WHERE user_id = $1
                   AND reason = $2
                   AND (created_at AT TIME ZONE 'UTC')::date
                     = (current_timestamp AT TIME ZONE 'UTC')::date",
                &[&submitter_id, &ChipMove::SongQueued.reason()],
            )
            .await?;
        let paid_today: i64 = row.get("paid_today");
        let reward = if paid_today >= SONG_QUEUE_MAX_PAID_PER_DAY {
            SongQueueReward::DailyCapReached
        } else {
            UserChips::apply(
                &tx,
                submitter_id,
                ChipMove::SongQueued,
                SONG_QUEUE_REWARD_CHIPS,
                Some(external_id),
            )
            .await?;
            SongQueueReward::Paid
        };
        tx.commit().await?;
        Ok((item, reward))
    }

    pub async fn find_by_id(client: &Client, id: Uuid) -> Result<Option<Self>> {
        Self::get(client, id).await
    }

    /// Whether this YouTube video is already queued or playing. The playlist
    /// holds a track once (`idx_media_queue_active_track`); a finished track
    /// leaves the active set and can be submitted again.
    pub async fn youtube_is_active(client: &Client, external_id: &str) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT EXISTS (
                    SELECT 1 FROM media_queue_items
                    WHERE media_kind = 'youtube'
                      AND external_id = $1
                      AND status IN ('queued', 'playing')
                 )",
                &[&external_id],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn list_snapshot(client: &Client, limit: i64) -> Result<Vec<(Self, i32)>> {
        let rows = client
            .query(
                "SELECT mqi.*, COALESCE(SUM(mqv.value), 0)::int AS vote_score
                 FROM media_queue_items mqi
                 LEFT JOIN media_queue_votes mqv ON mqv.item_id = mqi.id
                 WHERE mqi.status IN ('queued', 'playing')
                 GROUP BY mqi.id
                 ORDER BY
                    CASE mqi.status WHEN 'playing' THEN 0 ELSE 1 END,
                    vote_score DESC,
                    mqi.created
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

    pub async fn first_queued(client: &Client) -> Result<Option<(Self, i32)>> {
        let row = client
            .query_opt(
                "SELECT mqi.*, COALESCE(SUM(mqv.value), 0)::int AS vote_score
                 FROM media_queue_items mqi
                 LEFT JOIN media_queue_votes mqv ON mqv.item_id = mqi.id
                 WHERE mqi.status = 'queued'
                 GROUP BY mqi.id
                 ORDER BY vote_score DESC, mqi.created
                 LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(|row| {
            let score: i32 = row.get("vote_score");
            (Self::from(row), score)
        }))
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

    /// Atomically flip `unskippable` on a queued item. Returns `Some(row)`
    /// with the new value on success; `None` if the item is not queued (or
    /// does not exist).
    pub async fn toggle_unskippable_queued(client: &Client, id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE media_queue_items
                 SET unskippable = NOT unskippable,
                     updated = current_timestamp
                 WHERE id = $1 AND status = 'queued'
                 RETURNING *",
                &[&id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn delete_queued(client: &Client, id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM media_queue_items
                 WHERE id = $1 AND status = 'queued'",
                &[&id],
            )
            .await?;
        Ok(count)
    }

    pub async fn mark_skipped(client: &Client, id: Uuid, ended_at: DateTime<Utc>) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE media_queue_items
                 SET status = 'skipped',
                     ended_at = $2,
                     updated = current_timestamp
                 WHERE id = $1 AND status = 'playing'",
                &[&id, &ended_at],
            )
            .await?;
        Ok(count)
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
