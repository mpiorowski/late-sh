use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::{Client, Row};
use uuid::Uuid;

pub struct NativeToken {
    pub token: String,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl From<Row> for NativeToken {
    fn from(row: Row) -> Self {
        Self {
            token: row.get("token"),
            user_id: row.get("user_id"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
        }
    }
}

impl NativeToken {
    pub async fn create(
        client: &Client,
        token: &str,
        user_id: Uuid,
        expires_at: DateTime<Utc>,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO native_tokens (token, user_id, expires_at)
                 VALUES ($1, $2, $3)
                 RETURNING *",
                &[&token, &user_id, &expires_at],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Returns `(user_id, username)` if the token exists and has not expired.
    pub async fn find_user_by_token(
        client: &Client,
        token: &str,
    ) -> Result<Option<(Uuid, String)>> {
        let row = client
            .query_opt(
                "SELECT u.id, u.username
                 FROM native_tokens t
                 JOIN users u ON u.id = t.user_id
                 WHERE t.token = $1 AND t.expires_at > NOW()",
                &[&token],
            )
            .await?;
        Ok(row.map(|r| (r.get("id"), r.get("username"))))
    }

    pub async fn delete(client: &Client, token: &str) -> Result<()> {
        client
            .execute("DELETE FROM native_tokens WHERE token = $1", &[&token])
            .await?;
        Ok(())
    }

    pub async fn purge_expired(client: &Client) -> Result<u64> {
        let n = client
            .execute("DELETE FROM native_tokens WHERE expires_at <= NOW()", &[])
            .await?;
        Ok(n)
    }
}
