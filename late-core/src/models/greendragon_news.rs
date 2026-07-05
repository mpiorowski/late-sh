// Green Dragon daily news: the village's "yesterday in Duskmere" feed
// (LoGD's news.php / addnews). Items are short pre-rendered lines keyed by
// the game's UTC day-number; user_id is the subject when there is one
// (NULL = a system line). Retention is 180 days, pruned on write.

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

/// Days a news item stays readable before the daily prune reaps it.
pub const NEWS_RETENTION_DAYS: i64 = 180;

crate::model! {
    table = "greendragon_news";
    params = GreenDragonNewsParams;
    struct GreenDragonNews {
        @data
        pub day: i64,
        pub user_id: Option<Uuid>,
        pub body: String,
    }
}

impl GreenDragonNews {
    /// Append a news line for `day` (LoGD `addnews`).
    pub async fn add(client: &Client, day: i64, user_id: Option<Uuid>, body: &str) -> Result<()> {
        client
            .execute(
                "INSERT INTO greendragon_news (day, user_id, body) VALUES ($1, $2, $3)",
                &[&day, &user_id, &body],
            )
            .await?;
        Ok(())
    }

    /// The news for one day, newest first (upstream orders `newsid DESC`
    /// within the day), with a sanity cap standing in for its 50-per-page.
    pub async fn list_for_day(client: &Client, day: i64, limit: i64) -> Result<Vec<String>> {
        let rows = client
            .query(
                "SELECT body FROM greendragon_news
                 WHERE day = $1
                 ORDER BY created DESC, id DESC
                 LIMIT $2",
                &[&day, &limit],
            )
            .await?;
        Ok(rows.into_iter().map(|r| r.get("body")).collect())
    }

    /// Reap items older than the retention window, given the current day.
    pub async fn prune(client: &Client, today: i64) -> Result<u64> {
        let cutoff = today - NEWS_RETENTION_DAYS;
        Ok(client
            .execute(
                "DELETE FROM greendragon_news WHERE day < $1",
                &[&cutoff],
            )
            .await?)
    }
}
