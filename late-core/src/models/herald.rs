//! The herald's daily post (`herald_posts`, migration 252): every read and
//! write of the table, and the two reads that feed the rotation (what
//! went out the day before, which poster is waiting).
//!
//! A row is a claim. The sweeper in late-ssh `app/herald/svc.rs` inserts
//! a `posting` row before it spends the instance's calls, so two replicas
//! checking the same minute cannot both post; the winner settles the row
//! to `posted` or `skipped`, a loser finds nothing to insert. A post that
//! fails flips its row to `failed` and keeps the attempt count: the next
//! check claims it again until the caller's cap, after which the day
//! rests. A `posting` row nobody settled (a replica died mid-call) is
//! reclaimed once it is older than the caller's stale bound. The row's
//! `id` is minted at the first claim and survives every reclaim: it is the
//! idempotency key the status call carries.

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// What kind of picture a day's post carried. Stored as text, parsed on
/// read, and never defaulted: an unknown kind in the table is a bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeraldKind {
    /// Yesterday's daily Artboard snapshot.
    Artboard,
    /// A runner's poster from PRINTS.
    Poster,
}

impl HeraldKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artboard => "artboard",
            Self::Poster => "poster",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "artboard" => Ok(Self::Artboard),
            "poster" => Ok(Self::Poster),
            other => bail!("unknown herald kind {other:?}"),
        }
    }
}

/// A standing runner's poster the herald has not posted: the member to
/// credit and the public page the picture hangs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosterCandidate {
    pub user_id: Uuid,
    pub username: String,
    /// The work profile's slug: `/profiles/{slug}` and its `poster.png`.
    pub slug: String,
}

/// What a settled day says went out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeraldPosted {
    pub kind: HeraldKind,
    pub subject: String,
    pub status_url: String,
}

/// The daily claim and its record.
pub struct HeraldPost;

impl HeraldPost {
    /// Take the day: a new row, a stale `posting` one, or a `failed` one
    /// under the cap. The row's id (the status call's idempotency key),
    /// or `None` when another replica has the day or the day is settled.
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

    /// The claimed day went out: what, about whom, and where it landed.
    pub async fn finish_posted(
        client: &Client,
        day: NaiveDate,
        posted: &HeraldPosted,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE herald_posts
                 SET status = 'posted', kind = $2, subject = $3, status_url = $4,
                     finished_at = current_timestamp
                 WHERE day = $1",
                &[
                    &day,
                    &posted.kind.as_str(),
                    &posted.subject,
                    &posted.status_url,
                ],
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

    /// What the newest post before `day` carried, for the rotation. `None`
    /// before the first post ever.
    pub async fn last_posted_kind(client: &Client, day: NaiveDate) -> Result<Option<HeraldKind>> {
        let row = client
            .query_opt(
                "SELECT kind FROM herald_posts
                 WHERE status = 'posted' AND day < $1
                 ORDER BY day DESC
                 LIMIT 1",
                &[&day],
            )
            .await?;
        match row {
            Some(row) => {
                let kind: String = row.get("kind");
                Ok(Some(HeraldKind::parse(&kind)?))
            }
            None => Ok(None),
        }
    }

    /// The day's row as settled, for the admin's reading and the tests.
    pub async fn posted_on(client: &Client, day: NaiveDate) -> Result<Option<HeraldPosted>> {
        let row = client
            .query_opt(
                "SELECT kind, subject, status_url FROM herald_posts
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
                    status_url: row.get("status_url"),
                }))
            }
            None => Ok(None),
        }
    }

    /// The next poster to post: a standing runner (`left_at` null) with a
    /// poster and a public work profile, never posted by the herald. The
    /// member seen most recently first, so the credit lands on someone
    /// around to see it.
    pub async fn next_poster_candidate(client: &Client) -> Result<Option<PosterCandidate>> {
        let row = client
            .query_opt(
                "SELECT r.user_id, u.username, w.slug
                 FROM deadchannel_runners r
                 JOIN work_profiles w ON w.user_id = r.user_id
                 JOIN users u ON u.id = r.user_id
                 WHERE r.poster IS NOT NULL
                   AND r.left_at IS NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM herald_posts h
                       WHERE h.kind = 'poster' AND h.subject = r.user_id::text)
                 ORDER BY u.last_seen DESC, r.user_id
                 LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(|row| PosterCandidate {
            user_id: row.get("user_id"),
            username: row.get("username"),
            slug: row.get("slug"),
        }))
    }
}
