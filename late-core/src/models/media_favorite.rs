use anyhow::{Context, Result};
use tokio_postgres::Client;
use uuid::Uuid;

use super::media_history_item::MediaHistoryItem;

/// How many Booth tracks one listener keeps. Favorites sit on top of the
/// 200-row History list, so they stay a short list of what to come back to;
/// past this an add is refused until one is let go.
pub const BOOTH_FAVORITES_LIMIT: i64 = 20;

// A Booth track a listener keeps: one `media_favorites` row, with its own
// copy of the validated metadata so it outlives the History row it came
// from. Every read and write is scoped to the owner; a favorite is let go
// with the macro's `delete_by_user_id`, whose count tells a remove from one
// already gone.
crate::user_scoped_model! {
    table = "media_favorites";
    user_field = user_id;
    params = MediaFavoriteParams;
    struct MediaFavorite {
        @data
        pub user_id: Uuid,
        pub media_kind: String,
        pub external_id: String,
        pub title: Option<String>,
        pub channel: Option<String>,
        pub duration_ms: Option<i32>,
        pub is_stream: bool,
    }
}

/// What one add did, decided by [`MediaFavorite::add_from_history`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FavoriteOutcome {
    /// The track is a favorite now.
    Added,
    /// It already was; nothing written.
    AlreadyFavorite,
    /// The listener holds [`BOOTH_FAVORITES_LIMIT`] favorites; nothing written.
    Full,
}

impl MediaFavorite {
    /// Keep a History track as the favorite `id`. The check and the insert
    /// run in one transaction under a per-listener advisory lock, so two
    /// sessions adding together cannot both read nineteen and keep a
    /// twenty-first. The caller names the id: the session shows the row
    /// before the write lands, and this is the row it showed.
    pub async fn add_from_history(
        client: &mut Client,
        user_id: Uuid,
        id: Uuid,
        track: &MediaHistoryItem,
    ) -> Result<FavoriteOutcome> {
        let tx = client.transaction().await?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(
               hashtextextended(concat_ws(':', 'booth_favorite', ($1::uuid)::text), 0)
             )",
            &[&user_id],
        )
        .await?;
        let row = tx
            .query_one(
                "SELECT
                     EXISTS (
                         SELECT 1 FROM media_favorites
                         WHERE user_id = $1 AND media_kind = $2 AND external_id = $3
                     ) AS kept,
                     (SELECT COUNT(*) FROM media_favorites WHERE user_id = $1)::BIGINT AS held",
                &[&user_id, &track.media_kind, &track.external_id],
            )
            .await?;
        let kept: bool = row.get("kept");
        let held: i64 = row.get("held");
        let outcome = if kept {
            FavoriteOutcome::AlreadyFavorite
        } else if held >= BOOTH_FAVORITES_LIMIT {
            FavoriteOutcome::Full
        } else {
            tx.execute(
                "INSERT INTO media_favorites
                    (id, user_id, media_kind, external_id, title, channel, duration_ms, is_stream)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &id,
                    &user_id,
                    &track.media_kind,
                    &track.external_id,
                    &track.title,
                    &track.channel,
                    &track.duration_ms,
                    &track.is_stream,
                ],
            )
            .await
            .context("inserting the favorite")?;
            FavoriteOutcome::Added
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// The listener's favorites, newest first.
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM media_favorites
                 WHERE user_id = $1
                 ORDER BY created DESC, id DESC
                 LIMIT $2",
                &[&user_id, &BOOTH_FAVORITES_LIMIT],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}
