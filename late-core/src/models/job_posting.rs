//! The job feed's rows (migrations 193 and 194): every read and write of
//! `job_postings` and `job_press_runs`. The press in `late-ssh/app/jobs`
//! drives the feed rows and the post form writes the `late` ones; the
//! shelf and the paper only read.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// Where a posting came from. Closed: the fetcher has one parser per feed
/// variant, the card names the site, and `Late` is written by a person on
/// the Jobs shelf rather than pulled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobSource {
    Hn,
    Wwr,
    Jobicy,
    Late,
}

impl JobSource {
    pub const ALL: [Self; 4] = [Self::Hn, Self::Wwr, Self::Jobicy, Self::Late];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hn => "hn",
            Self::Wwr => "wwr",
            Self::Jobicy => "jobicy",
            Self::Late => "late",
        }
    }

    /// The site a card credits: `via news.ycombinator.com`.
    pub const fn site(self) -> &'static str {
        match self {
            Self::Hn => "news.ycombinator.com",
            Self::Wwr => "weworkremotely.com",
            Self::Jobicy => "jobicy.com",
            Self::Late => "late.sh",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "hn" => Self::Hn,
            "wwr" => Self::Wwr,
            "jobicy" => Self::Jobicy,
            "late" => Self::Late,
            other => panic!("unknown job source in the database: {other}"),
        }
    }
}

/// Where a row is in its life. `Pending` has source text and no card;
/// `Queued` is an HN card waiting for its day in the drip; `Active` is on
/// the shelf; the last three are settled and never listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Queued,
    Active,
    Expired,
    /// Read, remote, and the link answered 404 or 410.
    Dead,
    /// Not remote, failed the source filter, or unreadable at the cap: a
    /// tombstone so the fetcher never reprocesses the pair.
    Dropped,
}

impl JobStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Dead => "dead",
            Self::Dropped => "dropped",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "queued" => Self::Queued,
            "active" => Self::Active,
            "expired" => Self::Expired,
            "dead" => Self::Dead,
            "dropped" => Self::Dropped,
            other => panic!("unknown job status in the database: {other}"),
        }
    }
}

/// How remote the posting is. `Hybrid` stays because HN posts say
/// "REMOTE or HYBRID" and the reader decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteKind {
    Worldwide,
    Regions,
    Hybrid,
}

impl RemoteKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Worldwide => "worldwide",
            Self::Regions => "regions",
            Self::Hybrid => "hybrid",
        }
    }

    /// What the card prints before the regions.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Worldwide => "remote worldwide",
            Self::Regions => "remote",
            Self::Hybrid => "hybrid",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "worldwide" => Self::Worldwide,
            "regions" => Self::Regions,
            "hybrid" => Self::Hybrid,
            other => panic!("unknown remote kind in the database: {other}"),
        }
    }
}

crate::text_column_enum!(JobSource);
crate::text_column_enum!(JobStatus);
crate::text_column_enum!(RemoteKind);

#[derive(Clone, Debug, PartialEq)]
pub struct JobPosting {
    pub id: Uuid,
    pub source: JobSource,
    pub external_id: String,
    pub status: JobStatus,
    pub read_attempts: i32,
    pub url: String,
    pub company: String,
    pub title: String,
    pub remote_kind: Option<RemoteKind>,
    pub regions: Vec<String>,
    pub tags: Vec<String>,
    pub pay: String,
    pub excerpt: String,
    /// The source text the read works from; cleared once the row settles.
    pub raw: String,
    pub posted_at: DateTime<Utc>,
    pub released_on: Option<NaiveDate>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    /// Who wrote a `Late` row; a feed row has nobody.
    pub posted_by: Option<Uuid>,
}

impl From<tokio_postgres::Row> for JobPosting {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            source: row.get("source"),
            external_id: row.get("external_id"),
            status: row.get("status"),
            read_attempts: row.get("read_attempts"),
            url: row.get("url"),
            company: row.get("company"),
            title: row.get("title"),
            remote_kind: row.get("remote_kind"),
            regions: row.get("regions"),
            tags: row.get("tags"),
            pay: row.get("pay"),
            excerpt: row.get("excerpt"),
            raw: row.get("raw"),
            posted_at: row.get("posted_at"),
            released_on: row.get("released_on"),
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
            posted_by: row.get("posted_by"),
        }
    }
}

/// A posting written on the shelf itself, already checked by the form:
/// every field is what the card prints, and it goes active at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPosting {
    pub posted_by: Uuid,
    pub url: String,
    pub company: String,
    pub title: String,
    pub remote_kind: RemoteKind,
    pub regions: Vec<String>,
    pub tags: Vec<String>,
    pub pay: String,
    pub excerpt: String,
}

/// What a take-down did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retract {
    /// The row left the shelf; `posted_by` wrote it, for the audit of a
    /// moderator's take-down.
    Gone { posted_by: Uuid },
    /// Not an active `late` row of that person's (or of anyone's, for a
    /// moderator): nothing changed.
    NotYours,
}

/// One posting as the fetcher hands it over. `remote_kind` and `regions`
/// are set when the feed says so (WWR's region tag, Jobicy's geo) and
/// left for the read otherwise. `dropped` inserts a tombstone instead: the
/// posting failed the source-side filter (Jobicy's substring tag match),
/// and only the pair is kept.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchedPosting {
    pub source: JobSource,
    pub external_id: String,
    pub url: String,
    pub raw: String,
    pub posted_at: DateTime<Utc>,
    pub remote_kind: Option<RemoteKind>,
    pub regions: Vec<String>,
    pub dropped: bool,
}

/// What the read produced for a remote posting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRead {
    pub url: String,
    pub company: String,
    pub title: String,
    pub remote_kind: RemoteKind,
    pub regions: Vec<String>,
    pub tags: Vec<String>,
    pub pay: String,
    pub excerpt: String,
}

/// How a pending row settles after its read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settle {
    /// HN: waits for its slice of the drip.
    Queued(JobRead),
    /// WWR and Jobicy: on the shelf now, stamped with the run's day.
    Active(JobRead, NaiveDate),
    /// Remote, but the link is gone.
    Dead(JobRead),
    /// Not remote.
    Dropped,
}

/// Whether an upsert made a new row or only stamped an old one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upsert {
    Inserted,
    Seen,
}

impl JobPosting {
    /// Insert a fetched posting, or stamp `last_seen` on the row the pair
    /// already has: a repeat fetch changes nothing else, so a settled row
    /// stays settled.
    pub async fn upsert_fetched(client: &Client, posting: &FetchedPosting) -> Result<Upsert> {
        let status = if posting.dropped {
            JobStatus::Dropped
        } else {
            JobStatus::Pending
        };
        let raw = if posting.dropped {
            ""
        } else {
            posting.raw.as_str()
        };
        let row = client
            .query_one(
                "INSERT INTO job_postings
                    (id, source, external_id, status, url, raw, posted_at, remote_kind, regions)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (source, external_id) DO UPDATE
                    SET last_seen = current_timestamp
                 RETURNING (xmax = 0) AS inserted",
                &[
                    &Uuid::now_v7(),
                    &posting.source,
                    &posting.external_id,
                    &status,
                    &posting.url,
                    &raw,
                    &posting.posted_at,
                    &posting.remote_kind,
                    &posting.regions,
                ],
            )
            .await?;
        Ok(if row.get::<_, bool>("inserted") {
            Upsert::Inserted
        } else {
            Upsert::Seen
        })
    }

    /// Rows waiting for a read, under the attempt cap, oldest post first.
    pub async fn list_pending(client: &Client, max_attempts: i32, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM job_postings
                 WHERE status = 'pending' AND read_attempts < $1
                 ORDER BY posted_at, id
                 LIMIT $2",
                &[&max_attempts, &limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Settle a read and return the row as it now stands, so the press
    /// can shelve an active one without a re-read. The source text goes
    /// with it; the excerpt is all the shelf ever shows.
    pub async fn settle(client: &Client, id: Uuid, settle: Settle) -> Result<Self> {
        let (status, read, released_on): (JobStatus, Option<JobRead>, Option<NaiveDate>) =
            match settle {
                Settle::Queued(read) => (JobStatus::Queued, Some(read), None),
                Settle::Active(read, day) => (JobStatus::Active, Some(read), Some(day)),
                Settle::Dead(read) => (JobStatus::Dead, Some(read), None),
                Settle::Dropped => (JobStatus::Dropped, None, None),
            };
        let row = match read {
            Some(read) => {
                client
                    .query_one(
                        "UPDATE job_postings
                         SET status = $2, url = $3, company = $4, title = $5, remote_kind = $6,
                             regions = $7, tags = $8, pay = $9, excerpt = $10, raw = '',
                             released_on = $11
                         WHERE id = $1
                         RETURNING *",
                        &[
                            &id,
                            &status,
                            &read.url,
                            &read.company,
                            &read.title,
                            &read.remote_kind,
                            &read.regions,
                            &read.tags,
                            &read.pay,
                            &read.excerpt,
                            &released_on,
                        ],
                    )
                    .await?
            }
            None => {
                client
                    .query_one(
                        "UPDATE job_postings SET status = $2, raw = ''
                         WHERE id = $1
                         RETURNING *",
                        &[&id, &status],
                    )
                    .await?
            }
        };
        Ok(Self::from(row))
    }

    /// A failed read: count it, keep the row pending for the next run.
    pub async fn note_read_failure(client: &Client, id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE job_postings SET read_attempts = read_attempts + 1 WHERE id = $1",
                &[&id],
            )
            .await?;
        Ok(())
    }

    /// Pending rows that reached the attempt cap become tombstones, so a
    /// posting the model cannot read does not cost a call every night.
    pub async fn drop_exhausted(client: &Client, max_attempts: i32) -> Result<u64> {
        let dropped = client
            .execute(
                "UPDATE job_postings SET status = 'dropped', raw = ''
                 WHERE status = 'pending' AND read_attempts >= $1",
                &[&max_attempts],
            )
            .await?;
        Ok(dropped)
    }

    /// How many of a source's rows wait for release, and the earliest
    /// post among them (the drip's window starts there).
    pub async fn queued(client: &Client, source: JobSource) -> Result<(i64, Option<NaiveDate>)> {
        let row = client
            .query_one(
                "SELECT count(*) AS queued, min(posted_at)::date AS earliest
                 FROM job_postings WHERE source = $1 AND status = 'queued'",
                &[&source],
            )
            .await?;
        Ok((row.get("queued"), row.get("earliest")))
    }

    /// Release the next `n` queued rows of a source, oldest post first,
    /// and stamp them with `day`. `SKIP LOCKED` keeps two replicas
    /// releasing at once from taking the same rows.
    pub async fn release_slice(
        client: &Client,
        source: JobSource,
        n: i64,
        day: NaiveDate,
    ) -> Result<Vec<Uuid>> {
        let rows = client
            .query(
                "UPDATE job_postings SET status = 'active', released_on = $3
                 WHERE id IN (SELECT id FROM job_postings
                              WHERE source = $1 AND status = 'queued'
                              ORDER BY posted_at, id
                              LIMIT $2
                              FOR UPDATE SKIP LOCKED)
                 RETURNING id",
                &[&source, &n, &day],
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    /// Active rows released before `before` leave the shelf.
    pub async fn expire_released_before(client: &Client, before: NaiveDate) -> Result<u64> {
        let expired = client
            .execute(
                "UPDATE job_postings SET status = 'expired'
                 WHERE status = 'active' AND released_on < $1",
                &[&before],
            )
            .await?;
        Ok(expired)
    }

    /// Active rows of a feed source that the feed stopped listing: absent
    /// since `last_seen_before`.
    pub async fn expire_unseen_since(
        client: &Client,
        source: JobSource,
        last_seen_before: DateTime<Utc>,
    ) -> Result<u64> {
        let expired = client
            .execute(
                "UPDATE job_postings SET status = 'expired'
                 WHERE status = 'active' AND source = $1 AND last_seen < $2",
                &[&source, &last_seen_before],
            )
            .await?;
        Ok(expired)
    }

    /// A posting written on the shelf: active now, released on `day`, its
    /// own id as the pair's external id. Returns the new row.
    pub async fn post(client: &Client, posting: &NewPosting, day: NaiveDate) -> Result<Self> {
        let id = Uuid::now_v7();
        let row = client
            .query_one(
                "INSERT INTO job_postings
                    (id, source, external_id, status, url, company, title, remote_kind,
                     regions, tags, pay, excerpt, posted_at, released_on, posted_by)
                 VALUES ($1, 'late', $2, 'active', $3, $4, $5, $6, $7, $8, $9, $10,
                         current_timestamp, $11, $12)
                 RETURNING *",
                &[
                    &id,
                    &id.to_string(),
                    &posting.url,
                    &posting.company,
                    &posting.title,
                    &posting.remote_kind,
                    &posting.regions,
                    &posting.tags,
                    &posting.pay,
                    &posting.excerpt,
                    &day,
                    &posting.posted_by,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Take a shelf-written posting down: the writer's own, or any for a
    /// moderator. Feed rows are never touched here. Owner scope is in the
    /// query.
    pub async fn retract(client: &Client, id: Uuid, by: Uuid, moderator: bool) -> Result<Retract> {
        let row = client
            .query_opt(
                "UPDATE job_postings SET status = 'expired'
                 WHERE id = $1 AND source = 'late' AND status = 'active'
                   AND (posted_by = $2 OR $3)
                 RETURNING posted_by",
                &[&id, &by, &moderator],
            )
            .await?;
        Ok(match row {
            Some(row) => Retract::Gone {
                posted_by: row.get("posted_by"),
            },
            None => Retract::NotYours,
        })
    }

    /// How many postings of `user_id`'s are on the shelf, for the cap.
    pub async fn count_active_by(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT count(*) FROM job_postings
                 WHERE status = 'active' AND posted_by = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.get(0))
    }

    /// The shelf: every active row, newest release first.
    pub async fn list_active(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM job_postings WHERE status = 'active'
                 ORDER BY released_on DESC, posted_at DESC, id DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// The paper's NEW WORK: what went active on `day`, still active.
    pub async fn list_released_on(client: &Client, day: NaiveDate) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM job_postings WHERE status = 'active' AND released_on = $1
                 ORDER BY posted_at DESC, id DESC",
                &[&day],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_by_external_id(
        client: &Client,
        source: JobSource,
        external_id: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM job_postings WHERE source = $1 AND external_id = $2",
                &[&source, &external_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }
}

/// What one press run did, kept on its row for the admin's banner and
/// the next day's reading of the logs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PressCounts {
    pub fetched: i32,
    pub read: i32,
    pub released: i32,
    pub expired: i32,
}

/// The daily claim.
pub struct JobPressRun;

impl JobPressRun {
    /// Take the day's run: a new row, a stale `running` one, or a `failed`
    /// one under the cap. `false` means another replica has it or had it.
    pub async fn claim(
        client: &Client,
        run_on: NaiveDate,
        stale_before: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<bool> {
        let claimed = client
            .execute(
                "INSERT INTO job_press_runs (run_on, status, attempts, claimed_at)
                 VALUES ($1, 'running', 1, current_timestamp)
                 ON CONFLICT (run_on) DO UPDATE
                    SET status = 'running',
                        claimed_at = current_timestamp,
                        attempts = job_press_runs.attempts + 1
                    WHERE (job_press_runs.status = 'running'
                           AND job_press_runs.claimed_at < $2)
                       OR (job_press_runs.status = 'failed'
                           AND job_press_runs.attempts < $3)",
                &[&run_on, &stale_before, &max_attempts],
            )
            .await?;
        Ok(claimed == 1)
    }

    /// Whether the day's run is over for good: done, or failed at the cap.
    /// A replica that reads `true` stops asking for that day.
    pub async fn is_finished(
        client: &Client,
        run_on: NaiveDate,
        max_attempts: i32,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT status, attempts FROM job_press_runs WHERE run_on = $1",
                &[&run_on],
            )
            .await?;
        Ok(match row {
            Some(row) => {
                let status: String = row.get("status");
                let attempts: i32 = row.get("attempts");
                status == "done" || (status == "failed" && attempts >= max_attempts)
            }
            None => false,
        })
    }

    pub async fn finish(client: &Client, run_on: NaiveDate, counts: PressCounts) -> Result<()> {
        client
            .execute(
                "UPDATE job_press_runs
                 SET status = 'done', finished_at = current_timestamp,
                     fetched = $2, read = $3, released = $4, expired = $5
                 WHERE run_on = $1",
                &[
                    &run_on,
                    &counts.fetched,
                    &counts.read,
                    &counts.released,
                    &counts.expired,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn mark_failed(client: &Client, run_on: NaiveDate) -> Result<()> {
        client
            .execute(
                "UPDATE job_press_runs SET status = 'failed' WHERE run_on = $1",
                &[&run_on],
            )
            .await?;
        Ok(())
    }
}
