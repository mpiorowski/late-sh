//! The herald's daily post (`herald_posts`, `herald_deliveries`,
//! `herald_settings`, migration 252): every read and write of the three
//! tables.
//!
//! A row is a claim. The sweeper in late-ssh `app/herald/svc.rs` inserts
//! a `posting` row before it spends any network's calls, so two replicas
//! checking the same minute cannot both post; the winner stores its pick,
//! records a delivery per network as each lands, and settles the row to
//! `posted` once every network has one, or to `skipped`. A day that fails
//! flips its row to `failed` and keeps the attempt count: the next check
//! claims it again until the caller's cap, reuses the stored pick, and
//! sends only to the networks without a delivery. A `posting` row nobody
//! settled (a replica died mid-call) is reclaimed once it is older than
//! the caller's stale bound. The row's `id` is minted at the first claim
//! and survives every reclaim: it is the idempotency key the networks see.

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

/// What kind of picture a day's post carried. Stored as text, parsed on
/// read, and never defaulted: an unknown kind in the table is a bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeraldKind {
    /// Yesterday's daily Artboard snapshot. The only kind so far; the
    /// next ones (`AUDIENCE.md`, the herald's future work) widen the
    /// column's check in a forward migration and land here.
    Artboard,
}

impl HeraldKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artboard => "artboard",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "artboard" => Ok(Self::Artboard),
            other => bail!("unknown herald kind {other:?}"),
        }
    }
}

/// A network the herald delivers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeraldNetwork {
    Mastodon,
    Bluesky,
}

impl HeraldNetwork {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mastodon => "mastodon",
            Self::Bluesky => "bluesky",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "mastodon" => Ok(Self::Mastodon),
            "bluesky" => Ok(Self::Bluesky),
            other => bail!("unknown herald network {other:?}"),
        }
    }
}

/// Where a day's picture landed on one network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeraldDelivery {
    pub network: HeraldNetwork,
    pub url: String,
}

/// A day's row as it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeraldDayStatus {
    Posting,
    Posted,
    Skipped,
    Failed,
}

impl HeraldDayStatus {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "posting" => Ok(Self::Posting),
            "posted" => Ok(Self::Posted),
            "skipped" => Ok(Self::Skipped),
            "failed" => Ok(Self::Failed),
            other => bail!("unknown herald day status {other:?}"),
        }
    }
}

/// A day's row with its deliveries, for the operator's `/herald status`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeraldDay {
    pub status: HeraldDayStatus,
    pub attempts: i32,
    pub deliveries: Vec<HeraldDelivery>,
}

impl HeraldDay {
    /// One clause for a banner.
    pub fn describe(&self) -> String {
        match self.status {
            HeraldDayStatus::Posting => "posting right now".to_string(),
            HeraldDayStatus::Posted => format!(
                "posted to {}",
                self.deliveries
                    .iter()
                    .map(|delivery| delivery.network.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            HeraldDayStatus::Skipped => "skipped, nothing worth posting".to_string(),
            HeraldDayStatus::Failed => format!(
                "failed after {} attempt(s), {} delivered",
                self.attempts,
                self.deliveries.len()
            ),
        }
    }
}

/// What a settled day says went out, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeraldPosted {
    pub kind: HeraldKind,
    pub subject: String,
    pub deliveries: Vec<HeraldDelivery>,
}

/// The daily claim and its record.
pub struct HeraldPost;

impl HeraldPost {
    /// Take the day: a new row, a stale `posting` one, or a `failed` one
    /// under the cap. The row's id (the networks' idempotency key), or
    /// `None` when another replica has the day or the day is settled.
    pub async fn claim(
        client: &Client,
        day: NaiveDate,
        stale_before: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<Option<Uuid>> {
        let row = client
            .query_opt(
                "INSERT INTO herald_posts (day, id, status, attempts, claimed_at)
                 VALUES ($1, $2, 'posting', 1, current_timestamp)
                 ON CONFLICT (day) DO UPDATE
                    SET status = 'posting',
                        claimed_at = current_timestamp,
                        attempts = herald_posts.attempts + 1
                    WHERE (herald_posts.status = 'posting'
                           AND herald_posts.claimed_at < $3)
                       OR (herald_posts.status = 'failed'
                           AND herald_posts.attempts < $4)
                 RETURNING id",
                &[&day, &Uuid::now_v7(), &stale_before, &max_attempts],
            )
            .await?;
        Ok(row.map(|row| row.get("id")))
    }

    /// Whether the day is over for good: posted, skipped, or failed at
    /// the cap. A replica that reads `true` stops asking for that day.
    pub async fn is_finished(client: &Client, day: NaiveDate, max_attempts: i32) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT status, attempts FROM herald_posts WHERE day = $1",
                &[&day],
            )
            .await?;
        Ok(match row {
            Some(row) => {
                let status: String = row.get("status");
                let attempts: i32 = row.get("attempts");
                status == "posted"
                    || status == "skipped"
                    || (status == "failed" && attempts >= max_attempts)
            }
            None => false,
        })
    }

    /// The claimed day's pick, written before the first network call so a
    /// retry delivers the same picture.
    pub async fn store_picture(
        client: &Client,
        day: NaiveDate,
        kind: HeraldKind,
        subject: &str,
        picture: &Value,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE herald_posts SET kind = $2, subject = $3, picture = $4 WHERE day = $1",
                &[&day, &kind.as_str(), &subject, picture],
            )
            .await?;
        Ok(())
    }

    /// The pick an earlier claim of the day stored, if any.
    pub async fn stored_picture(client: &Client, day: NaiveDate) -> Result<Option<Value>> {
        let row = client
            .query_opt(
                "SELECT picture FROM herald_posts WHERE day = $1 AND picture IS NOT NULL",
                &[&day],
            )
            .await?;
        Ok(row.map(|row| row.get("picture")))
    }

    /// One network has the day's picture. A second record of the same
    /// network is ignored.
    pub async fn record_delivery(
        client: &Client,
        day: NaiveDate,
        network: HeraldNetwork,
        url: &str,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO herald_deliveries (day, network, url)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (day, network) DO NOTHING",
                &[&day, &network.as_str(), &url],
            )
            .await?;
        Ok(())
    }

    pub async fn deliveries(client: &Client, day: NaiveDate) -> Result<Vec<HeraldDelivery>> {
        let rows = client
            .query(
                "SELECT network, url FROM herald_deliveries WHERE day = $1 ORDER BY network",
                &[&day],
            )
            .await?;
        rows.into_iter()
            .map(|row| {
                let network: String = row.get("network");
                Ok(HeraldDelivery {
                    network: HeraldNetwork::parse(&network)?,
                    url: row.get("url"),
                })
            })
            .collect()
    }

    /// Every configured network delivered.
    pub async fn finish_posted(client: &Client, day: NaiveDate) -> Result<()> {
        client
            .execute(
                "UPDATE herald_posts
                 SET status = 'posted', finished_at = current_timestamp
                 WHERE day = $1",
                &[&day],
            )
            .await?;
        Ok(())
    }

    /// The claimed day had nothing worth posting; settled so no replica
    /// looks again.
    pub async fn finish_skipped(client: &Client, day: NaiveDate) -> Result<()> {
        client
            .execute(
                "UPDATE herald_posts
                 SET status = 'skipped', finished_at = current_timestamp
                 WHERE day = $1",
                &[&day],
            )
            .await?;
        Ok(())
    }

    pub async fn mark_failed(client: &Client, day: NaiveDate) -> Result<()> {
        client
            .execute(
                "UPDATE herald_posts SET status = 'failed' WHERE day = $1",
                &[&day],
            )
            .await?;
        Ok(())
    }

    /// The day's row as it stands, with its deliveries. `None` before the
    /// first claim.
    pub async fn day(client: &Client, day: NaiveDate) -> Result<Option<HeraldDay>> {
        let row = client
            .query_opt(
                "SELECT status, attempts FROM herald_posts WHERE day = $1",
                &[&day],
            )
            .await?;
        match row {
            Some(row) => {
                let status: String = row.get("status");
                Ok(Some(HeraldDay {
                    status: HeraldDayStatus::parse(&status)?,
                    attempts: row.get("attempts"),
                    deliveries: Self::deliveries(client, day).await?,
                }))
            }
            None => Ok(None),
        }
    }

    /// The day as settled, with where it landed, for the operator's
    /// reading and the tests.
    pub async fn posted_on(client: &Client, day: NaiveDate) -> Result<Option<HeraldPosted>> {
        let row = client
            .query_opt(
                "SELECT kind, subject FROM herald_posts
                 WHERE day = $1 AND status = 'posted'",
                &[&day],
            )
            .await?;
        match row {
            Some(row) => {
                let kind: String = row.get("kind");
                Ok(Some(HeraldPosted {
                    kind: HeraldKind::parse(&kind)?,
                    subject: row.get("subject"),
                    deliveries: Self::deliveries(client, day).await?,
                }))
            }
            None => Ok(None),
        }
    }
}

/// The operator's switch (`herald_settings`, key `posting`): one row,
/// seeded off by migration 252, so a flip on one replica is a flip on
/// every replica and survives a restart. A missing row is an error, never
/// a default: nobody ships an off-state by accident.
///
/// Flipping it is a statement, not a key: `UPDATE herald_settings SET
/// enabled = true WHERE key = 'posting'` through `scripts/connect_db.sh`.
pub struct HeraldSwitch;

impl HeraldSwitch {
    const KEY: &'static str = "posting";

    /// The switch as it stands. An error for a row the seed never wrote.
    pub async fn is_on(client: &Client) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT enabled FROM herald_settings WHERE key = $1",
                &[&Self::KEY],
            )
            .await?;
        match row {
            Some(row) => Ok(row.get("enabled")),
            None => bail!("herald setting {} has no row", Self::KEY),
        }
    }

    /// Flip the switch. For tests and the operator's tooling; the app
    /// itself only reads.
    pub async fn set(client: &Client, enabled: bool) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE herald_settings SET enabled = $2, updated = current_timestamp WHERE key = $1",
                &[&Self::KEY, &enabled],
            )
            .await?;
        if updated != 1 {
            bail!("herald setting {} has no row", Self::KEY);
        }
        Ok(())
    }
}
