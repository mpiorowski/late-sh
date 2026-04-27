use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

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
    },
    Failed {
        user_id: Uuid,
        error: String,
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
