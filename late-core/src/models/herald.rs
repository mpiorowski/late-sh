//! The herald's daily post (`herald_posts`, `herald_deliveries`,
//! `herald_settings`, migration 252): every read and write of the three
//! tables.
//!
//! A row is a claim. The checker in late-ssh `app/herald/svc.rs` inserts
//! a `posting` row before it spends any network's calls, so two replicas
//! checking the same minute cannot both post; the winner stores its pick
//! (the picture and the look, migration 255; the kinds, migration 256),
//! records a delivery per network as each lands, and settles the row to
//! `posted` once every network has one, or to `skipped`. A day that fails
//! flips its row to `failed` and keeps the attempt count: the next check
//! claims it again until the caller's cap, reuses the stored pick, and
//! sends only to the networks without a delivery. A `posting` row nobody
//! settled (a replica died mid-call) is reclaimed once it is older than
//! the caller's stale bound. Every reclaim stays inside the caller's window
//! from the day's first claim (`first_claimed_at`), the hour Mastodon keeps
//! an idempotency key: past it a stale `posting` row rests as `failed` and
//! a `failed` one is left alone. The row's `id` is minted at the first
//! claim and survives every reclaim: it is the idempotency key the networks
//! see.

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

/// What kind of picture a day's post carried. Stored as text, parsed on
/// read, and never defaulted: an unknown kind in the table is a bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeraldKind {
    /// Yesterday's daily Artboard snapshot.
    Artboard,
    /// A piece a member hung in the gallery, once.
    Piece,
    /// The Late Edition's card, on a day with nothing drawn.
    Paper,
}

impl HeraldKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artboard => "artboard",
            Self::Piece => "piece",
            Self::Paper => "paper",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "artboard" => Ok(Self::Artboard),
            "piece" => Ok(Self::Piece),
            "paper" => Ok(Self::Paper),
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

/// What a settled day says went out, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeraldPosted {
    pub kind: HeraldKind,
    pub subject: String,
    pub deliveries: Vec<HeraldDelivery>,
}

/// A day's pick as stored: the picture and what the look saw in it, as
/// the herald serialized them. Both or neither (migration 255).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeraldPick {
    pub picture: Value,
    pub look: Value,
}

/// The daily claim and its record.
pub struct HeraldPost;

impl HeraldPost {
    /// Take the day: a new row, or a stale `posting` or `failed` one that
    /// is under the cap and whose first claim is after `window_start`. A
    /// stale `posting` row past the window rests as `failed` instead. The
    /// row's id (the networks' idempotency key), or `None` when another
    /// replica has the day or the day is settled.
    pub async fn claim(
        client: &Client,
        day: NaiveDate,
        stale_before: DateTime<Utc>,
        window_start: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<Option<Uuid>> {
        client
            .execute(
                "UPDATE herald_posts SET status = 'failed'
                 WHERE day = $1
                   AND status = 'posting'
                   AND claimed_at < $2
                   AND first_claimed_at < $3",
                &[&day, &stale_before, &window_start],
            )
            .await?;
        let row = client
            .query_opt(
                "INSERT INTO herald_posts
                    (day, id, status, attempts, claimed_at, first_claimed_at)
                 VALUES ($1, $2, 'posting', 1, current_timestamp, current_timestamp)
                 ON CONFLICT (day) DO UPDATE
                    SET status = 'posting',
                        claimed_at = current_timestamp,
                        attempts = herald_posts.attempts + 1
                    WHERE herald_posts.attempts < $4
                      AND herald_posts.first_claimed_at >= $3
                      AND ((herald_posts.status = 'posting'
                            AND herald_posts.claimed_at < $5)
                           OR herald_posts.status = 'failed')
                 RETURNING id",
                &[
                    &day,
                    &Uuid::now_v7(),
                    &window_start,
                    &max_attempts,
                    &stale_before,
                ],
            )
            .await?;
        Ok(row.map(|row| row.get("id")))
    }

    /// Whether the day is over for good: posted, skipped, or failed at
    /// the cap or past the window since its first claim. A replica that
    /// reads `true` stops asking for that day.
    pub async fn is_finished(
        client: &Client,
        day: NaiveDate,
        window_start: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT status, attempts, first_claimed_at FROM herald_posts WHERE day = $1",
                &[&day],
            )
            .await?;
        match row {
            Some(row) => {
                let status: String = row.get("status");
                let attempts: i32 = row.get("attempts");
                let first_claimed_at: DateTime<Utc> = row.get("first_claimed_at");
                Ok(match HeraldDayStatus::parse(&status)? {
                    HeraldDayStatus::Posted | HeraldDayStatus::Skipped => true,
                    HeraldDayStatus::Failed => {
                        attempts >= max_attempts || first_claimed_at < window_start
                    }
                    HeraldDayStatus::Posting => false,
                })
            }
            None => Ok(false),
        }
    }

    /// Settle a day nobody claimed as `skipped`, for `/herald on`: the day
    /// the switch slept through stays unposted instead of being caught up.
    /// `false` when the day already has a row, which is left alone.
    pub async fn settle_unclaimed(client: &Client, day: NaiveDate) -> Result<bool> {
        let inserted = client
            .execute(
                "INSERT INTO herald_posts
                    (day, id, status, attempts, claimed_at, first_claimed_at, finished_at)
                 VALUES ($1, $2, 'skipped', 0, current_timestamp, current_timestamp,
                         current_timestamp)
                 ON CONFLICT (day) DO NOTHING",
                &[&day, &Uuid::now_v7()],
            )
            .await?;
        Ok(inserted == 1)
    }

    /// The claimed day's pick, the picture and the look in one statement,
    /// written before the first network call so a retry delivers the same
    /// picture with the same alt text and mark.
    pub async fn store_pick(
        client: &Client,
        day: NaiveDate,
        kind: HeraldKind,
        subject: &str,
        pick: &HeraldPick,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE herald_posts SET kind = $2, subject = $3, picture = $4, look = $5 WHERE day = $1",
                &[&day, &kind.as_str(), &subject, &pick.picture, &pick.look],
            )
            .await?;
        Ok(())
    }

    /// The pick an earlier claim of the day stored, if any.
    pub async fn stored_pick(client: &Client, day: NaiveDate) -> Result<Option<HeraldPick>> {
        let row = client
            .query_opt(
                "SELECT picture, look FROM herald_posts WHERE day = $1 AND picture IS NOT NULL",
                &[&day],
            )
            .await?;
        Ok(row.map(|row| HeraldPick {
            picture: row.get("picture"),
            look: row.get("look"),
        }))
    }

    /// The subjects of `kind` picked on `since` or after, whatever came of
    /// the day: a piece picked once is never picked again, even when its
    /// day failed.
    pub async fn subjects_since(
        client: &Client,
        kind: HeraldKind,
        since: NaiveDate,
    ) -> Result<Vec<String>> {
        let rows = client
            .query(
                "SELECT subject FROM herald_posts WHERE kind = $1 AND day >= $2 ORDER BY day",
                &[&kind.as_str(), &since],
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.get("subject")).collect())
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
/// An admin flips it with `/herald on|off`; the same statement through
/// `scripts/connect_db.sh` works too: `UPDATE herald_settings SET enabled =
/// true WHERE key = 'posting'`.
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

    /// Flip the switch, for an admin's `/herald on|off`.
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
