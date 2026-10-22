use anyhow::{Context, Result};
use tokio_postgres::Client;
use uuid::Uuid;

use super::article::NEWS_FEED_LIMIT;

/// A News article a reader has read: one `article_reads` row, and nothing
/// else. Unread is a snapshot article without one. Rows are only ever
/// added, so a reader's reads merge in any order. A video row also keeps
/// where its reader left the video, the one field that moves.
pub struct ArticleRead;

/// Where a reader left a News video: the position mpv last reported, and
/// the duration it reported with it (`None` for a live stream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchPosition {
    pub position_s: u64,
    pub duration_s: Option<u64>,
}

/// One read of the snapshot's, as a load carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArticleReadRow {
    pub article_id: Uuid,
    /// `Some` while the reader is partway through the video.
    pub watch_position: Option<WatchPosition>,
}

impl ArticleRead {
    /// Mark one article read for one reader. Opening it again is a no-op.
    pub async fn mark_read(client: &Client, user_id: Uuid, article_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_reads (user_id, article_id)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id, article_id) DO NOTHING",
                &[&user_id, &article_id],
            )
            .await?;

        Ok(())
    }

    /// Mark the whole feed read for one reader: a row for every article in
    /// the snapshot, the newest [`NEWS_FEED_LIMIT`]. A visit to the News
    /// room, and the seed for a brand-new user, so the back catalog never
    /// shows as unread on a first login.
    pub async fn mark_feed_read(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO article_reads (user_id, article_id)
                 SELECT $1, id FROM articles ORDER BY created DESC LIMIT $2
                 ON CONFLICT (user_id, article_id) DO NOTHING",
                &[&user_id, &NEWS_FEED_LIMIT],
            )
            .await?;

        Ok(())
    }

    /// Remember where the reader left a video. The row is upserted: a watch
    /// marks the article read when it opens, and this is the retry of that
    /// write should it have failed.
    pub async fn save_watch_position(
        client: &Client,
        user_id: Uuid,
        article_id: Uuid,
        position: WatchPosition,
    ) -> Result<()> {
        let position_s = i64::try_from(position.position_s).context("position fits bigint")?;
        let duration_s = match position.duration_s {
            Some(duration_s) => Some(i64::try_from(duration_s).context("duration fits bigint")?),
            None => None,
        };
        client
            .execute(
                "INSERT INTO article_reads (user_id, article_id, watch_position_s, watch_duration_s)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id, article_id) DO UPDATE
                 SET watch_position_s = EXCLUDED.watch_position_s,
                     watch_duration_s = EXCLUDED.watch_duration_s",
                &[&user_id, &article_id, &position_s, &duration_s],
            )
            .await?;
        Ok(())
    }

    /// The video was watched to the end, or stopped inside its first
    /// seconds: the next watch starts from the top.
    pub async fn clear_watch_position(
        client: &Client,
        user_id: Uuid,
        article_id: Uuid,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE article_reads
                 SET watch_position_s = NULL, watch_duration_s = NULL
                 WHERE user_id = $1 AND article_id = $2",
                &[&user_id, &article_id],
            )
            .await?;
        Ok(())
    }

    /// The reader's reads among the snapshot's articles, the newest
    /// [`NEWS_FEED_LIMIT`], and the stories on their shelf (`article_saves`,
    /// at most [`super::article_save::NEWS_SAVED_LIMIT`]): the only ones a
    /// badge can count or a card can show, so a long reading history never
    /// rides a session's load. Each carries where the reader left the
    /// video, when it is one they are partway through; the shelf is in so
    /// a saved video still resumes once it has left the snapshot.
    pub async fn rows_for_user(client: &Client, user_id: Uuid) -> Result<Vec<ArticleReadRow>> {
        let rows = client
            .query(
                "SELECT article_id, watch_position_s, watch_duration_s FROM article_reads
                 WHERE user_id = $1
                   AND (
                       article_id IN (SELECT id FROM articles ORDER BY created DESC LIMIT $2)
                       OR article_id IN (SELECT article_id FROM article_saves WHERE user_id = $1)
                   )",
                &[&user_id, &NEWS_FEED_LIMIT],
            )
            .await?;
        rows.iter()
            .map(|row| {
                let position_s: Option<i64> = row.get("watch_position_s");
                let duration_s: Option<i64> = row.get("watch_duration_s");
                let watch_position = match position_s {
                    Some(position_s) => Some(WatchPosition {
                        position_s: u64::try_from(position_s)
                            .context("position is checked non-negative")?,
                        duration_s: match duration_s {
                            Some(duration_s) => Some(
                                u64::try_from(duration_s)
                                    .context("duration is checked non-negative")?,
                            ),
                            None => None,
                        },
                    }),
                    None => None,
                };
                Ok(ArticleReadRow {
                    article_id: row.get("article_id"),
                    watch_position,
                })
            })
            .collect()
    }
}
