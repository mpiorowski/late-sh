use anyhow::Result;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use tokio_postgres::{Client, Row};
use uuid::Uuid;

pub struct NativeToken {
    /// SHA-256 hex hash of the raw bearer token. Raw token is never stored.
    pub token_hash: String,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub user_agent: Option<String>,
    pub created_ip: Option<String>,
}

impl From<Row> for NativeToken {
    fn from(row: Row) -> Self {
        Self {
            token_hash: row.get("token"),
            user_id: row.get("user_id"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
            last_used_at: row.get("last_used_at"),
            user_agent: row.get("user_agent"),
            created_ip: row.get("created_ip"),
        }
    }
}

fn hash_token(raw: &str) -> String {
    let hash = Sha256::digest(raw.as_bytes());
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

impl NativeToken {
    pub async fn create(
        client: &Client,
        raw_token: &str,
        user_id: Uuid,
        expires_at: DateTime<Utc>,
        user_agent: Option<&str>,
        created_ip: Option<&str>,
    ) -> Result<Self> {
        let token_hash = hash_token(raw_token);
        let row = client
            .query_one(
                "INSERT INTO native_tokens (token, user_id, expires_at, user_agent, created_ip)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING *",
                &[&token_hash, &user_id, &expires_at, &user_agent, &created_ip],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Returns `(user_id, username)` if the token exists and has not expired.
    /// Also updates `last_used_at` atomically.
    pub async fn find_user_by_token(
        client: &Client,
        raw_token: &str,
    ) -> Result<Option<(Uuid, String)>> {
        let token_hash = hash_token(raw_token);
        let row = client
            .query_opt(
                "WITH updated AS (
                     UPDATE native_tokens SET last_used_at = NOW()
                     WHERE token = $1 AND expires_at > NOW()
                     RETURNING user_id
                 )
                 SELECT u.id, u.username
                 FROM updated
                 JOIN users u ON u.id = updated.user_id",
                &[&token_hash],
            )
            .await?;
        Ok(row.map(|r| (r.get("id"), r.get("username"))))
    }

    pub async fn delete(client: &Client, raw_token: &str) -> Result<()> {
        let token_hash = hash_token(raw_token);
        client
            .execute("DELETE FROM native_tokens WHERE token = $1", &[&token_hash])
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
