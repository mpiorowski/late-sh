use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::models::chips::{ChipMove, UserChips};

/// What publishing one article pays its author. Every share path funnels
/// through [`Article::create_shared`], so the News composer and an RSS entry
/// shared with `s` pay exactly this, and neither can pay a different amount.
pub const NEWS_SHARE_REWARD_CHIPS: i64 = 500;

/// How many shares are paid per person per UTC day. `s` on an RSS entry is
/// one keypress, so without a cap an inbox is a chip printer; three a day is
/// "share the good ones", and 1,500 chips sits under a completionist arcade
/// day. Shares past the cap still publish, they just mint nothing.
pub const NEWS_SHARE_MAX_PAID_PER_DAY: i64 = 3;

/// What one share actually minted, decided by [`Article::create_shared`].
/// The banner and the metric read this, never the constant, so an unpaid
/// share can never be reported as paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewsShareReward {
    /// [`NEWS_SHARE_REWARD_CHIPS`] were credited.
    Paid,
    /// This person was already paid for this URL once; nothing minted.
    RepeatUrl,
    /// This person has already been paid [`NEWS_SHARE_MAX_PAID_PER_DAY`]
    /// times today (UTC); nothing minted.
    DailyCapReached,
}

impl NewsShareReward {
    pub fn chips(self) -> i64 {
        match self {
            Self::Paid => NEWS_SHARE_REWARD_CHIPS,
            Self::RepeatUrl | Self::DailyCapReached => 0,
        }
    }
}

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
    /// once per URL per user for all time and at most
    /// [`NEWS_SHARE_MAX_PAID_PER_DAY`] times per UTC day. Returns the article
    /// and what the share actually minted.
    ///
    /// The `articles` row cannot be the record of payment: a delete frees the
    /// URL to be shared again, so paying on insert alone would let one player
    /// share, delete, and re-share the same link forever. The `chip_ledger`
    /// row is the record, keyed on `(user_id, url)`, which is why
    /// `source_ref` here is the URL rather than the article id. A second
    /// player sharing the same link later is still paid: the cap is per
    /// person, not per link. The daily cap counts the same rows by their
    /// UTC date, the way pot tickets are capped.
    ///
    /// Insert, lookup, and credit are one transaction under a per-user
    /// advisory lock, like every other claim-plus-credit path: a credit that
    /// fails leaves no orphan article squatting on the URL, and two shares by
    /// one person landing together cannot both read the same count and pay a
    /// fourth. The insert is spelled out here rather than going through the
    /// generated `create_by_user_id`, which only takes a bare `Client`; that
    /// one still exists for fixtures and backfills. This is the only path a
    /// user-facing share may take, so the reward cannot be forgotten at one
    /// call site and applied at another.
    pub async fn create_shared(
        client: &mut Client,
        user_id: Uuid,
        params: ArticleParams,
    ) -> Result<(Self, NewsShareReward)> {
        let tx = client.transaction().await?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(
               hashtextextended(concat_ws(':', 'news_share', ($1::uuid)::text), 0)
             )",
            &[&user_id],
        )
        .await?;
        let row = tx
            .query_one(
                "INSERT INTO articles (user_id, url, title, summary, ascii_art)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING *",
                &[
                    &user_id,
                    &params.url,
                    &params.title,
                    &params.summary,
                    &params.ascii_art,
                ],
            )
            .await?;
        let article = Self::from(row);
        let row = tx
            .query_one(
                "SELECT
                     COALESCE(bool_or(source_ref = $3), false) AS paid_url,
                     COUNT(*) FILTER (
                         WHERE (created_at AT TIME ZONE 'UTC')::date
                             = (current_timestamp AT TIME ZONE 'UTC')::date
                     )::BIGINT AS paid_today
                 FROM chip_ledger
                 WHERE user_id = $1 AND reason = $2",
                &[&user_id, &ChipMove::NewsShared.reason(), &article.url],
            )
            .await?;
        let paid_url: bool = row.get("paid_url");
        let paid_today: i64 = row.get("paid_today");
        let reward = if paid_url {
            NewsShareReward::RepeatUrl
        } else if paid_today >= NEWS_SHARE_MAX_PAID_PER_DAY {
            NewsShareReward::DailyCapReached
        } else {
            UserChips::apply(
                &tx,
                user_id,
                ChipMove::NewsShared,
                NEWS_SHARE_REWARD_CHIPS,
                Some(&article.url),
            )
            .await?;
            NewsShareReward::Paid
        };
        tx.commit().await?;
        Ok((article, reward))
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
        reward: NewsShareReward,
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
