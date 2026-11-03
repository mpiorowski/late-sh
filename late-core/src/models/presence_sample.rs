use anyhow::Result;
use chrono::{DateTime, Duration, DurationRound, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// How wide a sample bucket is, in minutes: each replica writes one row a
/// bucket.
pub const SAMPLE_MINUTES: i64 = 5;
/// How far back the pulse reads.
pub const HISTORY_HOURS: i64 = 24;
/// Rows older than this go on the next record.
const KEEP_HOURS: i64 = 48;

/// The `presence_samples` table: a replica's headcount per bucket, summed
/// across replicas on read.
pub struct PresenceSample;

/// One bucket of the day as the pulse reads it: every replica's people
/// online, added up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadcountPoint {
    pub bucket: DateTime<Utc>,
    pub online: i64,
}

impl PresenceSample {
    /// The bucket `at` falls in: the clock floored to `SAMPLE_MINUTES`.
    pub fn bucket_of(at: DateTime<Utc>) -> DateTime<Utc> {
        at.duration_trunc(Duration::minutes(SAMPLE_MINUTES))
            .unwrap_or(at)
    }

    /// Write a replica's headcount for `bucket`, replacing its earlier row
    /// for the same bucket, and drop rows past `KEEP_HOURS`.
    pub async fn record(
        client: &Client,
        replica_id: Uuid,
        bucket: DateTime<Utc>,
        online: i64,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO presence_samples (bucket, replica_id, online)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (bucket, replica_id) DO UPDATE SET online = EXCLUDED.online",
                &[&bucket, &replica_id, &online],
            )
            .await?;
        let keep_from = bucket - Duration::hours(KEEP_HOURS);
        client
            .execute(
                "DELETE FROM presence_samples WHERE bucket < $1",
                &[&keep_from],
            )
            .await?;
        Ok(())
    }

    /// The last `HISTORY_HOURS` of buckets before `now`, oldest first, each
    /// the sum of every replica's row.
    pub async fn last_day(client: &Client, now: DateTime<Utc>) -> Result<Vec<HeadcountPoint>> {
        let since = now - Duration::hours(HISTORY_HOURS);
        let rows = client
            .query(
                "SELECT bucket, SUM(online)::BIGINT AS online
                 FROM presence_samples
                 WHERE bucket > $1 AND bucket <= $2
                 GROUP BY bucket
                 ORDER BY bucket",
                &[&since, &now],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|row| HeadcountPoint {
                bucket: row.get("bucket"),
                online: row.get("online"),
            })
            .collect())
    }
}
