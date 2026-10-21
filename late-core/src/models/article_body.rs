use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, Row};
use uuid::Uuid;

/// The words a reader gets through in a minute, for a card's `7 min read`.
pub const READING_WORDS_PER_MINUTE: u32 = 220;

/// Where a story's body stands, as the cards and the reader see it. No
/// `article_bodies` row is `Pending`; a row is `Ready` or `Unavailable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyStatus {
    /// The fetch has not landed yet.
    Pending,
    /// The article text is stored and the reader shows it; the card reads
    /// `7 min read`.
    Ready { reading_minutes: u16 },
    /// There is no text to show (a video, an X post, a page the fetch could
    /// not read, a body retired by retention): the reader shows the summary.
    Unavailable,
}

impl BodyStatus {
    /// The `status` and `word_count` columns of a `LEFT JOIN`ed body row:
    /// none is pending.
    pub fn from_columns(status: Option<&str>, word_count: Option<i32>) -> Result<Self> {
        let status = match status {
            Some(status) => Some(StoredStatus::parse(status)?),
            None => None,
        };
        match (status, word_count) {
            (None, _) => Ok(Self::Pending),
            (Some(StoredStatus::Ready), Some(words)) => Ok(Self::Ready {
                reading_minutes: reading_minutes(
                    u32::try_from(words).context("negative word count")?,
                ),
            }),
            (Some(StoredStatus::Ready), None) => bail!("ready article body without a word count"),
            (Some(StoredStatus::Unavailable), _) => Ok(Self::Unavailable),
        }
    }
}

/// A stored row's `status`, the two values the table's CHECK allows. Both
/// read paths (the feed's join and [`ArticleBody::find`]) parse it here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoredStatus {
    Ready,
    Unavailable,
}

impl StoredStatus {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "ready" => Ok(Self::Ready),
            "unavailable" => Ok(Self::Unavailable),
            other => bail!("unknown article body status {other:?}"),
        }
    }
}

/// One line of a story as the reader draws it. Stored as JSON in
/// `article_bodies.lines`, so a variant renamed here breaks every stored
/// body: add variants, never rename them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReaderLine {
    Heading { level: u8, text: String },
    Paragraph { spans: Vec<ReaderSpan> },
    Quote { spans: Vec<ReaderSpan> },
    Code { lines: Vec<String> },
    ListItem { spans: Vec<ReaderSpan> },
    Rule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderSpan {
    Text(String),
    Link { text: String, href: String },
}

/// What a stored body holds. A ready body always has its words and lines;
/// an unavailable one has neither.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyContent {
    Ready {
        word_count: u32,
        lines: Vec<ReaderLine>,
    },
    Unavailable,
}

/// One `article_bodies` row: the reader's text for one story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArticleBody {
    pub article_id: Uuid,
    pub content: BodyContent,
    pub fetched_at: DateTime<Utc>,
}

/// `ceil(words / READING_WORDS_PER_MINUTE)`, never zero for a body with
/// words in it.
pub fn reading_minutes(word_count: u32) -> u16 {
    let minutes = word_count.div_ceil(READING_WORDS_PER_MINUTE);
    u16::try_from(minutes).unwrap_or(u16::MAX)
}

impl ArticleBody {
    pub fn status(&self) -> BodyStatus {
        match &self.content {
            BodyContent::Ready { word_count, .. } => BodyStatus::Ready {
                reading_minutes: reading_minutes(*word_count),
            },
            BodyContent::Unavailable => BodyStatus::Unavailable,
        }
    }

    /// The one story's body, for the reader on open. `None` is pending.
    pub async fn find(client: &Client, article_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM article_bodies WHERE article_id = $1",
                &[&article_id],
            )
            .await?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// Store a fetched body. A second store for the same story replaces the
    /// first, so a retried fetch is harmless.
    pub async fn store_ready(
        client: &Client,
        article_id: Uuid,
        word_count: u32,
        lines: &[ReaderLine],
    ) -> Result<()> {
        let lines = serde_json::to_value(lines).context("serializing reader lines")?;
        let word_count = i32::try_from(word_count).context("word count out of range")?;
        client
            .execute(
                "INSERT INTO article_bodies (article_id, status, word_count, lines)
                 VALUES ($1, 'ready', $2, $3)
                 ON CONFLICT (article_id) DO UPDATE
                 SET status = EXCLUDED.status, word_count = EXCLUDED.word_count,
                     lines = EXCLUDED.lines, fetched_at = current_timestamp",
                &[&article_id, &word_count, &lines],
            )
            .await?;
        Ok(())
    }

    /// Record that a story has no text to read, so its card stops waiting.
    pub async fn store_unavailable(client: &Client, article_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_bodies (article_id, status, word_count, lines)
                 VALUES ($1, 'unavailable', 0, '[]'::jsonb)
                 ON CONFLICT (article_id) DO UPDATE
                 SET status = EXCLUDED.status, word_count = 0, lines = '[]'::jsonb,
                     fetched_at = current_timestamp",
                &[&article_id],
            )
            .await?;
        Ok(())
    }

    /// Retention: drop the text of every ready body fetched before
    /// `fetched_before`. The row stays as `unavailable`, so the card reads
    /// `link` and the reader shows the summary instead of a fetch that
    /// will never come. A story on anyone's shelf (`article_saves`) keeps
    /// its text: it was saved to be read. Returns how many bodies were
    /// retired.
    pub async fn retire_fetched_before(
        client: &Client,
        fetched_before: DateTime<Utc>,
    ) -> Result<u64> {
        let retired = client
            .execute(
                "UPDATE article_bodies
                 SET status = 'unavailable', word_count = 0, lines = '[]'::jsonb
                 WHERE status = 'ready' AND fetched_at < $1
                   AND NOT EXISTS (
                       SELECT 1 FROM article_saves s WHERE s.article_id = article_bodies.article_id
                   )",
                &[&fetched_before],
            )
            .await?;
        Ok(retired)
    }

    /// Sweep: a story shared before `created_before` with no body row had
    /// its fetch cut short (a restart mid-fetch, an aborted task, a store
    /// that failed). Settle it as `unavailable`, so the card reads `link`
    /// and the reader shows the summary instead of a fetch that will never
    /// land. The first run also settles every story shared before bodies
    /// existed. Returns how many were settled.
    pub async fn settle_pending_before(
        client: &Client,
        created_before: DateTime<Utc>,
    ) -> Result<u64> {
        let settled = client
            .execute(
                "INSERT INTO article_bodies (article_id, status, word_count, lines)
                 SELECT a.id, 'unavailable', 0, '[]'::jsonb
                 FROM articles a
                 WHERE a.created < $1
                   AND NOT EXISTS (SELECT 1 FROM article_bodies b WHERE b.article_id = a.id)
                 ON CONFLICT (article_id) DO NOTHING",
                &[&created_before],
            )
            .await?;
        Ok(settled)
    }

    fn from_row(row: &Row) -> Result<Self> {
        let status: String = row.get("status");
        let content = match StoredStatus::parse(&status)? {
            StoredStatus::Ready => {
                let word_count: i32 = row.get("word_count");
                let lines: serde_json::Value = row.get("lines");
                BodyContent::Ready {
                    word_count: u32::try_from(word_count).context("negative word count")?,
                    lines: serde_json::from_value(lines).context("reading reader lines")?,
                }
            }
            StoredStatus::Unavailable => BodyContent::Unavailable,
        };
        Ok(Self {
            article_id: row.get("article_id"),
            content,
            fetched_at: row.get("fetched_at"),
        })
    }
}
