use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "showcases";
    user_field = user_id;
    params = ShowcaseParams;
    struct Showcase {
        @data
        pub user_id: Uuid,
        pub title: String,
        pub url: String,
        pub description: String,
        pub tags: Vec<String>,
    }
}

/// A project with its author's name, for the herald's post.
#[derive(Clone, Debug)]
pub struct AuthoredShowcase {
    pub showcase: Showcase,
    pub username: String,
}

impl Showcase {
    pub async fn list_recent(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM showcases ORDER BY created DESC, id DESC LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Up to `limit` projects in random order whose author logged in on or
    /// after `active_since` and is not banned, leaving out `excluded`: what
    /// the herald draws its weekly project from.
    pub async fn random_active(
        client: &Client,
        active_since: DateTime<Utc>,
        excluded: &[Uuid],
        limit: i64,
    ) -> Result<Vec<AuthoredShowcase>> {
        let rows = client
            .query(
                "SELECT s.*, u.username AS author_username
                 FROM showcases s
                 JOIN users u ON u.id = s.user_id
                 WHERE u.last_seen >= $1
                   AND NOT (s.id = ANY($2))
                   AND NOT EXISTS (
                       SELECT 1 FROM server_bans b
                       WHERE b.target_user_id = u.id
                         AND (b.expires_at IS NULL OR b.expires_at > current_timestamp)
                   )
                 ORDER BY random()
                 LIMIT $3",
                &[&active_since, &excluded, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let username = row.get("author_username");
                AuthoredShowcase {
                    showcase: Self::from(row),
                    username,
                }
            })
            .collect())
    }
}
