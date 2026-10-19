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
    /// The article text is stored and the reader shows it.
    Ready,
    /// There is no text to show (a video, an X post, a page the fetch could
    /// not read, a body retired by retention): the reader shows the summary.
    Unavailable,
}

impl BodyStatus {
    /// The `status` column of a `LEFT JOIN`ed body row: none is pending, and
    /// a value no build ever wrote is an error rather than a guess.
    pub fn from_column(status: Option<&str>) -> Result<Self> {
        match status {
            None => Ok(Self::Pending),
            Some("ready") => Ok(Self::Ready),
            Some("unavailable") => Ok(Self::Unavailable),
            Some(other) => bail!("unknown article body status {other:?}"),
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
    /// The host the fetch ended on, after redirects.
    pub domain: String,
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
        match self.content {
            BodyContent::Ready { .. } => BodyStatus::Ready,
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
        domain: &str,
        word_count: u32,
        lines: &[ReaderLine],
    ) -> Result<()> {
        let lines = serde_json::to_value(lines).context("serializing reader lines")?;
        let word_count = i32::try_from(word_count).context("word count out of range")?;
        client
            .execute(
                "INSERT INTO article_bodies (article_id, status, domain, word_count, lines)
                 VALUES ($1, 'ready', $2, $3, $4)
                 ON CONFLICT (article_id) DO UPDATE
                 SET status = EXCLUDED.status, domain = EXCLUDED.domain,
                     word_count = EXCLUDED.word_count, lines = EXCLUDED.lines,
                     fetched_at = current_timestamp",
                &[&article_id, &domain, &word_count, &lines],
            )
            .await?;
        Ok(())
    }

    /// Record that a story has no text to read, so its card stops waiting.
    pub async fn store_unavailable(client: &Client, article_id: Uuid, domain: &str) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_bodies (article_id, status, domain, word_count, lines)
                 VALUES ($1, 'unavailable', $2, 0, '[]'::jsonb)
                 ON CONFLICT (article_id) DO UPDATE
                 SET status = EXCLUDED.status, domain = EXCLUDED.domain,
                     word_count = 0, lines = '[]'::jsonb,
                     fetched_at = current_timestamp",
                &[&article_id, &domain],
            )
            .await?;
        Ok(())
    }

    /// Retention: drop the text of every ready body fetched before
    /// `fetched_before`. The row stays as `unavailable`, so the card reads
    /// `link` and the reader shows the summary instead of a fetch that
    /// will never come. Returns how many bodies were retired.
    pub async fn retire_fetched_before(
        client: &Client,
        fetched_before: DateTime<Utc>,
    ) -> Result<u64> {
        let retired = client
            .execute(
                "UPDATE article_bodies
                 SET status = 'unavailable', word_count = 0, lines = '[]'::jsonb
                 WHERE status = 'ready' AND fetched_at < $1",
                &[&fetched_before],
            )
            .await?;
        Ok(retired)
    }

    fn from_row(row: &Row) -> Result<Self> {
        let status: String = row.get("status");
        let content = match BodyStatus::from_column(Some(&status))? {
            BodyStatus::Ready => {
                let word_count: i32 = row.get("word_count");
                let lines: serde_json::Value = row.get("lines");
                BodyContent::Ready {
                    word_count: u32::try_from(word_count).context("negative word count")?,
                    lines: serde_json::from_value(lines).context("reading reader lines")?,
                }
            }
            BodyStatus::Unavailable => BodyContent::Unavailable,
            BodyStatus::Pending => unreachable!("a stored row always has a status"),
        };
        Ok(Self {
            article_id: row.get("article_id"),
            domain: row.get("domain"),
            content,
            fetched_at: row.get("fetched_at"),
        })
    }
}
