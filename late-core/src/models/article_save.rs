use anyhow::{Context, Result};
use tokio_postgres::Client;
use uuid::Uuid;

use super::article::FeedArticle;

/// How many stories one reader keeps saved. The shelf sits above the feed's
/// [`super::article::NEWS_FEED_LIMIT`] stories, so it stays a short list of
/// what to come back to, never an archive; past this a save is refused
/// until one is let go.
pub const NEWS_SAVED_LIMIT: i64 = 10;

/// A News story a reader keeps for later: one `article_saves` row. The
/// shelf is the reader's alone, holds at most [`NEWS_SAVED_LIMIT`] stories,
/// and lists newest save first.
pub struct ArticleSave;

/// What one save did, decided by [`ArticleSave::save`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The story is on the shelf now.
    Saved,
    /// It already was; nothing written.
    AlreadySaved,
    /// The shelf holds [`NEWS_SAVED_LIMIT`] stories; nothing written.
    Full,
}

impl ArticleSave {
    /// Put a story on the reader's shelf. The check and the insert run in
    /// one transaction under a per-reader advisory lock, so two sessions
    /// saving together cannot both read nine and shelve an eleventh.
    pub async fn save(client: &mut Client, user_id: Uuid, article_id: Uuid) -> Result<SaveOutcome> {
        let tx = client.transaction().await?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(
               hashtextextended(concat_ws(':', 'news_save', ($1::uuid)::text), 0)
             )",
            &[&user_id],
        )
        .await?;
        let row = tx
            .query_one(
                "SELECT
                     EXISTS (
                         SELECT 1 FROM article_saves
                         WHERE user_id = $1 AND article_id = $2
                     ) AS saved,
                     (SELECT COUNT(*) FROM article_saves WHERE user_id = $1)::BIGINT AS held",
                &[&user_id, &article_id],
            )
            .await?;
        let saved: bool = row.get("saved");
        let held: i64 = row.get("held");
        let outcome = if saved {
            SaveOutcome::AlreadySaved
        } else if held >= NEWS_SAVED_LIMIT {
            SaveOutcome::Full
        } else {
            tx.execute(
                "INSERT INTO article_saves (user_id, article_id) VALUES ($1, $2)",
                &[&user_id, &article_id],
            )
            .await
            .context("inserting the save")?;
            SaveOutcome::Saved
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// Take a story off the reader's shelf. A story that is not on it is a
    /// no-op.
    pub async fn unsave(client: &Client, user_id: Uuid, article_id: Uuid) -> Result<()> {
        client
            .execute(
                "DELETE FROM article_saves WHERE user_id = $1 AND article_id = $2",
                &[&user_id, &article_id],
            )
            .await?;
        Ok(())
    }

    /// The reader's shelf, newest save first, each story with where its
    /// body stands, the same row shape the feed carries.
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<FeedArticle>> {
        let rows = client
            .query(
                "SELECT a.*, b.status AS body_status, b.word_count AS body_word_count
                 FROM article_saves s
                 JOIN articles a ON a.id = s.article_id
                 LEFT JOIN article_bodies b ON b.article_id = a.id
                 WHERE s.user_id = $1
                 ORDER BY s.created DESC, a.id DESC
                 LIMIT $2",
                &[&user_id, &NEWS_SAVED_LIMIT],
            )
            .await?;
        rows.into_iter().map(FeedArticle::from_row).collect()
    }
}
