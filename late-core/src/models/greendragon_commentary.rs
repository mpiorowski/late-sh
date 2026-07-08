// Green Dragon commentary: the one shared chat primitive behind every talk
// room (LoGD lib/commentary.php). Rooms are `section` values on one table;
// `user_id` NULL marks a system line; `name` snapshots the speaker's
// character name at post time. Bodies are stored in upstream's post-time
// shape (non-"says" venues arrive pre-baked as `:verb, "..."` emotes) and
// compose into rendered lines at view time. Retention is 180 days
// (upstream's `expirecontent` default), pruned opportunistically on write.

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

/// Days a comment stays readable before the prune reaps it (upstream
/// `expirecontent` default).
pub const COMMENTARY_RETENTION_DAYS: i32 = 180;

crate::model! {
    table = "greendragon_commentary";
    params = GreenDragonCommentaryParams;
    struct GreenDragonCommentary {
        @data
        pub section: String,
        pub user_id: Option<Uuid>,
        pub name: String,
        pub body: String,
    }
}

/// One comment as the game reads it back: who said it, the stored body, and
/// the UTC day-number it was posted (for the daily post-allowance count).
pub struct CommentaryRow {
    pub user_id: Option<Uuid>,
    pub name: String,
    pub body: String,
    pub day: i64,
}

impl GreenDragonCommentary {
    /// Append one comment to a section (LoGD `injectrawcomment`).
    pub async fn add(
        client: &Client,
        section: &str,
        user_id: Option<Uuid>,
        name: &str,
        body: &str,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO greendragon_commentary (section, user_id, name, body)
                 VALUES ($1, $2, $3, $4)",
                &[&section, &user_id, &name, &body],
            )
            .await?;
        Ok(())
    }

    /// One display window of a section, newest first: page 0 is the newest
    /// `limit` rows, page N starts `N * limit` rows back (upstream's
    /// `ORDER BY commentid DESC LIMIT (com*limit), limit` comscroll pages).
    pub async fn latest(
        client: &Client,
        section: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CommentaryRow>> {
        let rows = client
            .query(
                "SELECT user_id, name, body,
                        floor(extract(epoch FROM created) / 86400)::bigint AS day
                 FROM greendragon_commentary
                 WHERE section = $1
                 ORDER BY created DESC, id DESC
                 LIMIT $2 OFFSET $3",
                &[&section, &limit, &offset],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| CommentaryRow {
                user_id: r.get("user_id"),
                name: r.get("name"),
                body: r.get("body"),
                day: r.get("day"),
            })
            .collect())
    }

    /// How many of a section's comments were posted on or after the given
    /// UTC day-number — the "first unseen" jump's numerator (upstream counts
    /// `postdate > recentcomments`; ours compares at the blob's day
    /// granularity and uses the marker's own `>=` so the jump lands on
    /// exactly the set that renders new).
    pub async fn count_since_day(client: &Client, section: &str, day: i64) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT count(*) FROM greendragon_commentary
                 WHERE section = $1
                   AND floor(extract(epoch FROM created) / 86400)::bigint >= $2",
                &[&section, &day],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Reap comments older than the retention window.
    pub async fn prune(client: &Client) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM greendragon_commentary
                 WHERE created < current_timestamp - make_interval(days => $1)",
                &[&COMMENTARY_RETENTION_DAYS],
            )
            .await?)
    }
}
