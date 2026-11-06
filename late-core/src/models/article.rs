use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::models::article_body::BodyStatus;
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
        @generated
        // The share's card in #lounge (migration 250): set once the post
        // lands (`Article::link_lounge_message`), `None` before cards, when
        // the room refused the post, and once the message is deleted.
        pub lounge_message_id: Option<Uuid>;
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

    /// The shared news snapshot's rows: the newest `limit` articles, each
    /// with where its body stands. Only the body's status and word count
    /// ride along, never its lines: the snapshot is re-read on every notify
    /// on every replica, and the reader loads the one body it opens
    /// ([`ArticleBody::find`]).
    pub async fn list_feed(client: &Client, limit: i64) -> Result<Vec<FeedArticle>> {
        let rows = client
            .query(
                "SELECT a.*, b.status AS body_status, b.word_count AS body_word_count
                 FROM articles a
                 LEFT JOIN article_bodies b ON b.article_id = a.id
                 ORDER BY a.created DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        rows.into_iter().map(FeedArticle::from_row).collect()
    }

    /// The newest `limit` shares that have a card in #lounge, each with
    /// where its body stands: what chat draws those messages as. A #lounge
    /// history holds far fewer shares than this, so every card a session
    /// has loaded is among them.
    pub async fn list_lounge_cards(client: &Client, limit: i64) -> Result<Vec<FeedArticle>> {
        let rows = client
            .query(
                "SELECT a.*, b.status AS body_status, b.word_count AS body_word_count
                 FROM articles a
                 LEFT JOIN article_bodies b ON b.article_id = a.id
                 WHERE a.lounge_message_id IS NOT NULL
                 ORDER BY a.created DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        rows.into_iter().map(FeedArticle::from_row).collect()
    }

    /// Point the sharer's share at its card in #lounge, the message the
    /// share posted. The count of rows linked: 0 when the share is gone,
    /// deleted while its card went up.
    pub async fn link_lounge_message(
        client: &Client,
        user_id: Uuid,
        article_id: Uuid,
        message_id: Uuid,
    ) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE articles SET lounge_message_id = $3 WHERE id = $2 AND user_id = $1",
                &[&user_id, &article_id, &message_id],
            )
            .await?;
        Ok(count)
    }

    /// Whether `message_id` is a share's card in #lounge.
    pub async fn is_lounge_card(client: &Client, message_id: Uuid) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM articles WHERE lounge_message_id = $1)",
                &[&message_id],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Delete a share and hand back the row as it went, `None` when it was
    /// already gone. The card pointer is read by the delete itself, so a
    /// card linked while the delete ran is never missed.
    pub async fn delete_returning(client: &Client, id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt("DELETE FROM articles WHERE id = $1 RETURNING *", &[&id])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Articles shared inside `[floor, ceiling)` with the sharer's username,
    /// oldest first: the input of the paper's "what we were reading" page.
    pub async fn list_shared_between(
        client: &Client,
        floor: DateTime<Utc>,
        ceiling: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<(Self, String)>> {
        let rows = client
            .query(
                "SELECT a.*, u.username AS sharer
                 FROM articles a
                 JOIN users u ON u.id = a.user_id
                 WHERE a.created >= $1 AND a.created < $2
                 ORDER BY a.created ASC, a.id ASC
                 LIMIT $3",
                &[&floor, &ceiling, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let sharer: String = row.get("sharer");
                (Self::from(row), sharer)
            })
            .collect())
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
                &article.url,
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

/// How many articles the shared news snapshot holds. Each session counts its
/// unread badge from that snapshot, so the badge saturates here.
pub const NEWS_FEED_LIMIT: i64 = 40;

/// How many #lounge cards the shared news snapshot holds
/// ([`Article::list_lounge_cards`]).
pub const NEWS_LOUNGE_CARD_LIMIT: i64 = 100;

/// Cross-process refresh channel. Any write to `articles` fires it (migration
/// 198 statement trigger, empty payload); every replica's listener re-reads
/// the newest [`NEWS_FEED_LIMIT`] articles into its shared snapshot.
pub const ARTICLES_CHANGED_CHANNEL: &str = "articles_changed";

#[derive(Clone, Default)]
pub struct ArticleSnapshot {
    pub user_id: Option<Uuid>,
    pub articles: Vec<ArticleFeedItem>,
    /// The newest shares with a card in #lounge, by the card's message id.
    /// Shared, so a session taking the snapshot copies a pointer.
    pub lounge_cards: Arc<HashMap<Uuid, ArticleFeedItem>>,
}

/// One row of [`Article::list_feed`] or a reader's shelf
/// ([`super::article_save::ArticleSave::list_for_user`]), before the
/// sharer's name is attached.
pub struct FeedArticle {
    pub article: Article,
    pub body: BodyStatus,
}

impl FeedArticle {
    /// A row of `articles.*` joined with the body's `status` and
    /// `word_count` as `body_status` and `body_word_count`.
    pub fn from_row(row: tokio_postgres::Row) -> Result<Self> {
        let status: Option<String> = row.get("body_status");
        let word_count: Option<i32> = row.get("body_word_count");
        Ok(Self {
            body: BodyStatus::from_columns(status.as_deref(), word_count)?,
            article: Article::from(row),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ArticleFeedItem {
    pub article: Article,
    pub author_username: String,
    /// Where the story's body stands, with its reading time once ready; the
    /// card's meta and the reader's first frame read it.
    pub body: BodyStatus,
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
    /// This user's news reads among the snapshot's articles
    /// (`article_reads`), loaded at session start and after every write
    /// (a News room visit, one article opened, a video position saved).
    /// Reads only grow, so a session merges each load into what it holds,
    /// in whatever order the loads land, and counts its own unread badge
    /// from them against the shared snapshot. The video positions riding
    /// the rows replace what the session holds: the newest load is the
    /// truth about where a video stands.
    ReadsLoaded {
        user_id: Uuid,
        reads: Vec<super::article_read::ArticleReadRow>,
    },
    /// This user's shelf (`article_saves`), newest save first and at most
    /// [`super::article_save::NEWS_SAVED_LIMIT`] stories, loaded at session
    /// start, on a News room visit, and after every save or unsave. The
    /// shelf moves (a save, an unsave, a delete cascading), so each load
    /// replaces what the session holds.
    SavesLoaded {
        user_id: Uuid,
        saved: Vec<ArticleFeedItem>,
    },
    /// A save the shelf refused: it already held
    /// [`super::article_save::NEWS_SAVED_LIMIT`] stories by the time the
    /// write landed (another session of the user's filled it). It follows
    /// the `SavesLoaded` that took the story back off the session's shelf.
    SaveRefused {
        user_id: Uuid,
    },
}
