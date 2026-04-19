use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "votes";
    params = VoteParams;
    struct Vote {
        @data
        pub user_id: Uuid,
        pub genre: String,
    }
}

impl Vote {
    /// Upsert vote - insert or update if user already voted.
    /// Returns the vote and whether the genre changed (true if new vote or different genre).
    pub async fn upsert(client: &Client, user_id: Uuid, genre: &str) -> Result<(Self, bool)> {
        let row = client
            .query_one(
                "WITH old AS (
                     SELECT genre FROM votes WHERE user_id = $1
                 )
                 INSERT INTO votes (user_id, genre) VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET genre = $2, updated = current_timestamp
                 RETURNING *, (SELECT genre FROM old) AS old_genre",
                &[&user_id, &genre],
            )
            .await?;
        let old_genre: Option<String> = row.get("old_genre");
        let vote = Self::from(row);
        let changed = old_genre.as_deref() != Some(genre);
        Ok((vote, changed))
    }

    /// Get user's current vote
    pub async fn find_by_user(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                &format!("SELECT * FROM {} WHERE user_id = $1", Self::TABLE),
                &[&user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Get vote counts per genre.
    /// Returns `(lofi, classic, ambient, jazz)`.
    pub async fn tally(client: &Client) -> Result<(i64, i64, i64, i64)> {
        let rows = client
            .query(
                "SELECT genre, COUNT(*)::bigint as count FROM votes GROUP BY genre",
                &[],
            )
            .await?;

        let mut lofi = 0i64;
        let mut classic = 0i64;
        let mut ambient = 0i64;
        let mut jazz = 0i64;
        for row in rows {
            let genre: String = row.get("genre");
            let count: i64 = row.get("count");
            match genre.as_str() {
                "lofi" => lofi = count,
                "classic" => classic = count,
                "ambient" => ambient = count,
                "jazz" => jazz = count,
                _ => {}
            }
        }
        Ok((lofi, classic, ambient, jazz))
    }

    /// Clear all votes (optional - for resetting after rotation)
    pub async fn clear_all(client: &Client) -> Result<u64> {
        let count = client.execute("DELETE FROM votes", &[]).await?;
        Ok(count)
    }
}
