use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "rss_feeds";
    user_field = user_id;
    params = RssFeedParams;
    struct RssFeed {
        @data
        pub user_id: Uuid,
        pub url: String,
        pub title: String,
        pub active: bool,
        pub last_checked_at: Option<DateTime<Utc>>,
        pub last_success_at: Option<DateTime<Utc>>,
        pub last_error: Option<String>,
    }
}

impl RssFeed {
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM rss_feeds
                 WHERE user_id = $1
                 ORDER BY created DESC",
                &[&user_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_active(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM rss_feeds
                 WHERE active = true
                 ORDER BY COALESCE(last_checked_at, '-infinity'::timestamptz), created
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn create_for_user(client: &Client, user_id: Uuid, url: &str) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO rss_feeds (user_id, url, title)
                 VALUES ($1, $2, '')
                 ON CONFLICT (user_id, url)
                 DO UPDATE SET active = true, updated = current_timestamp
                 RETURNING *",
                &[&user_id, &url],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn delete_for_user(client: &Client, user_id: Uuid, id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM rss_feeds WHERE user_id = $1 AND id = $2",
                &[&user_id, &id],
            )
            .await?;
        Ok(count)
    }

    pub async fn record_success(client: &Client, id: Uuid, title: &str) -> Result<()> {
        client
            .execute(
                "UPDATE rss_feeds
                 SET title = $1,
                     last_checked_at = current_timestamp,
                     last_success_at = current_timestamp,
                     last_error = NULL,
                     updated = current_timestamp
                 WHERE id = $2",
                &[&title, &id],
            )
            .await?;
        Ok(())
    }

    pub async fn record_failure(client: &Client, id: Uuid, message: &str) -> Result<()> {
        client
            .execute(
                "UPDATE rss_feeds
                 SET last_checked_at = current_timestamp,
                     last_error = $1,
                     updated = current_timestamp
                 WHERE id = $2",
                &[&message, &id],
            )
            .await?;
        Ok(())
    }
}
