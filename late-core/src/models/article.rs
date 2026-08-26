use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::models::chips::{ChipMove, UserChips};

/// What publishing one article pays its author. Every share path funnels
/// through [`Article::create_shared`], so the News composer and an RSS entry
/// shared with `s` pay exactly this, and neither can pay a different amount.
pub const NEWS_SHARE_REWARD_CHIPS: i64 = 500;

crate::user_scoped_model! {
    table = "articles";
    user_field = user_id;
    params = ArticleParams;
    struct Article {
        @data
        pub user_id: Uuid,
        pub url: String,
        pub title: String,
        pub summary: String,
        pub ascii_art: String,
    }
}

impl Article {
    /// List recent articles across all users
    pub async fn list_recent(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM articles ORDER BY created DESC LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Publish an article and pay its author [`NEWS_SHARE_REWARD_CHIPS`],
    /// once per URL per user for all time. Returns the article and what the
    /// share actually paid, which is zero on a repeat.
    ///
    /// The `articles` row cannot be the record of payment: a delete frees the
    /// URL to be shared again, so paying on insert alone would let one player
    /// share, delete, and re-share the same link forever. The `chip_ledger`
    /// row is the record, keyed on `(user_id, url)`, which is why
    /// `source_ref` here is the URL rather than the article id. A second
    /// player sharing the same link later is still paid: the cap is per
    /// person, not per link.
    ///
    /// The plain `create_by_user_id` still exists for fixtures and backfills;
    /// this is the only path a user-facing share may take, so the reward
    /// cannot be forgotten at one call site and applied at another.
    pub async fn create_shared(
        client: &Client,
        user_id: Uuid,
        params: ArticleParams,
    ) -> Result<(Self, i64)> {
        let url = params.url.clone();
        let article = Self::create_by_user_id(client, user_id, params).await?;
        let paid_before = client
            .query_opt(
                "SELECT 1 FROM chip_ledger
                 WHERE user_id = $1 AND reason = $2 AND source_ref = $3
                 LIMIT 1",
                &[&user_id, &ChipMove::NewsShared.reason(), &url],
            )
            .await?
            .is_some();
        if paid_before {
            return Ok((article, 0));
        }

        UserChips::apply(
            client,
            user_id,
            ChipMove::NewsShared,
            NEWS_SHARE_REWARD_CHIPS,
            Some(&url),
        )
        .await?;
        Ok((article, NEWS_SHARE_REWARD_CHIPS))
    }

    pub async fn find_by_url(client: &Client, url: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM articles WHERE url = $1", &[&url])
            .await?;
        Ok(row.map(Self::from))
    }
}

pub const NEWS_MARKER: &str = "---NEWS---";

#[derive(Clone, Default)]
pub struct ArticleSnapshot {
    pub user_id: Option<Uuid>,
    pub articles: Vec<ArticleFeedItem>,
}

#[derive(Clone)]
pub struct ArticleFeedItem {
    pub article: Article,
    pub author_username: String,
}

#[derive(Clone, Debug)]
pub enum ArticleEvent {
    Created {
        user_id: Uuid,
        url: String,
    },
    Failed {
        user_id: Uuid,
        error: String,
        url: Option<String>,
    },
    Deleted {
        user_id: Uuid,
    },
    UnreadCountUpdated {
        user_id: Uuid,
        unread_count: i64,
        last_read_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    NewArticlesAvailable {
        user_id: Uuid,
        unread_count: i64,
    },
}
