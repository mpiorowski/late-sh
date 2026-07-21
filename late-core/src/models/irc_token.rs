use anyhow::Result;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio_postgres::Client;
use uuid::Uuid;

/// Tokens are `late-irc-` + 32 chars of a 32-symbol alphabet (160 bits of
/// CSPRNG entropy). Token strength, not rate limiting, is the brute-force
/// defense; see devdocs/FRD-IRCD.md §5.
pub const TOKEN_PREFIX: &str = "late-irc-";
const TOKEN_RANDOM_LEN: usize = 32;
const TOKEN_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

crate::model! {
    table = "irc_tokens";
    params = IrcTokenParams;
    struct IrcToken {
        @data
        pub user_id: Uuid,
        pub token_hash: String,
        pub last_used: Option<DateTime<Utc>>,
    }
}

impl IrcToken {
    /// Mint a token for the user, replacing any existing one. Returns the
    /// plaintext token, which is never persisted and must be shown exactly
    /// once.
    pub async fn mint(client: &Client, user_id: Uuid) -> Result<String> {
        let token = generate_token()?;
        client
            .execute(
                "INSERT INTO irc_tokens (user_id, token_hash)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id)
                 DO UPDATE SET token_hash = $2,
                               last_used = NULL,
                               created = current_timestamp,
                               updated = current_timestamp",
                &[&user_id, &hash_token(&token)],
            )
            .await?;
        Ok(token)
    }

    /// Revoke the user's token. Returns true if a token existed.
    pub async fn revoke(client: &Client, user_id: Uuid) -> Result<bool> {
        let n = client
            .execute("DELETE FROM irc_tokens WHERE user_id = $1", &[&user_id])
            .await?;
        Ok(n > 0)
    }

    /// Status row for the settings UI (token value is unrecoverable).
    pub async fn find_for_user(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM irc_tokens WHERE user_id = $1", &[&user_id])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Look up a token row by plaintext token (hash lookup).
    pub async fn find_by_token(client: &Client, token: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM irc_tokens WHERE token_hash = $1",
                &[&hash_token(token)],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn touch_last_used(client: &Client, id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE irc_tokens
                 SET last_used = current_timestamp, updated = current_timestamp
                 WHERE id = $1",
                &[&id],
            )
            .await?;
        Ok(())
    }
}

fn generate_token() -> Result<String> {
    let mut bytes = [0u8; TOKEN_RANDOM_LEN];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("rng failure: {e}"))?;
    let random: String = bytes
        .iter()
        .map(|byte| TOKEN_ALPHABET[(*byte & 31) as usize] as char)
        .collect();
    Ok(format!("{TOKEN_PREFIX}{random}"))
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
#[path = "irc_token_test.rs"]
mod irc_token_test;
