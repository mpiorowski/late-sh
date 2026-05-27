use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

const LINK_CODE_LEN: usize = 12;
const LINK_CODE_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

#[derive(Clone, Debug)]
pub struct AccountLinkPeer {
    pub user_id: Uuid,
    pub username: String,
    pub created: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct AccountLinkResult {
    pub kept_user_id: Uuid,
    pub kept_username: String,
    pub abandoned_user_id: Uuid,
    pub abandoned_username: String,
}

pub async fn create_code(client: &Client, user_id: Uuid) -> Result<(String, DateTime<Utc>)> {
    let expires_at = Utc::now() + Duration::minutes(10);
    client
        .execute(
            "UPDATE account_link_codes
             SET consumed_at = current_timestamp, updated = current_timestamp
             WHERE user_id = $1 AND consumed_at IS NULL",
            &[&user_id],
        )
        .await?;

    for _ in 0..8 {
        let code = generate_code();
        let row = client
            .query_opt(
                "INSERT INTO account_link_codes (user_id, code, expires_at)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (code) DO NOTHING
                 RETURNING code",
                &[&user_id, &code, &expires_at],
            )
            .await?;
        if row.is_some() {
            return Ok((code, expires_at));
        }
    }

    bail!("could not allocate account link code")
}

pub async fn peer_for_code(
    client: &Client,
    current_user_id: Uuid,
    code: &str,
) -> Result<AccountLinkPeer> {
    let code = normalize_code(code);
    if code.is_empty() {
        bail!("enter a link code");
    }

    let row = client
        .query_opt(
            "SELECT u.id, u.username, u.created
             FROM account_link_codes c
             JOIN users u ON u.id = c.user_id
             WHERE c.code = $1
               AND c.consumed_at IS NULL
               AND c.expires_at > current_timestamp",
            &[&code],
        )
        .await?;

    let Some(row) = row else {
        bail!("link code expired or not found");
    };
    let user_id = row.get("id");
    if user_id == current_user_id {
        bail!("enter a code from the other account");
    }

    Ok(AccountLinkPeer {
        user_id,
        username: row.get("username"),
        created: row.get("created"),
    })
}

pub async fn complete(
    client: &mut Client,
    current_user_id: Uuid,
    peer_user_id: Uuid,
    peer_code: &str,
    kept_user_id: Uuid,
) -> Result<AccountLinkResult> {
    if current_user_id == peer_user_id {
        bail!("cannot link an account to itself");
    }
    if kept_user_id != current_user_id && kept_user_id != peer_user_id {
        bail!("kept account must be one of the linked accounts");
    }

    let tx = client.transaction().await?;
    let peer_code = normalize_code(peer_code);
    let code_owner = tx
        .query_opt(
            "SELECT user_id
             FROM account_link_codes
             WHERE code = $1
               AND user_id = $2
               AND consumed_at IS NULL
               AND expires_at > current_timestamp
             FOR UPDATE",
            &[&peer_code, &peer_user_id],
        )
        .await?;
    if code_owner.is_none() {
        bail!("link code expired or already used");
    }

    let rows = tx
        .query(
            "SELECT id, username
             FROM users
             WHERE id = ANY($1)
             FOR UPDATE",
            &[&vec![current_user_id, peer_user_id]],
        )
        .await?;
    if rows.len() != 2 {
        bail!("one account no longer exists");
    }

    let mut current_username = None;
    let mut peer_username = None;
    for row in rows {
        let user_id: Uuid = row.get("id");
        let username: String = row.get("username");
        if user_id == current_user_id {
            current_username = Some(username);
        } else if user_id == peer_user_id {
            peer_username = Some(username);
        }
    }
    let current_username = current_username.ok_or_else(|| anyhow::anyhow!("current account missing"))?;
    let peer_username = peer_username.ok_or_else(|| anyhow::anyhow!("peer account missing"))?;

    let abandoned_user_id = if kept_user_id == current_user_id {
        peer_user_id
    } else {
        current_user_id
    };
    let kept_username = if kept_user_id == current_user_id {
        current_username.clone()
    } else {
        peer_username.clone()
    };
    let abandoned_username = if abandoned_user_id == current_user_id {
        current_username
    } else {
        peer_username
    };

    ensure_no_active_link_blocking_bans(&tx, &[current_user_id, peer_user_id]).await?;
    move_ssh_keys(&tx, abandoned_user_id, kept_user_id).await?;
    tx.execute(
        "UPDATE account_link_codes
         SET consumed_at = current_timestamp, updated = current_timestamp
         WHERE user_id = $1 OR user_id = $2",
        &[&current_user_id, &peer_user_id],
    )
    .await?;
    tx.execute("DELETE FROM users WHERE id = $1", &[&abandoned_user_id])
        .await?;
    tx.commit().await?;

    Ok(AccountLinkResult {
        kept_user_id,
        kept_username,
        abandoned_user_id,
        abandoned_username,
    })
}

async fn ensure_no_active_link_blocking_bans(
    client: &impl GenericClient,
    user_ids: &[Uuid],
) -> Result<()> {
    let user_ids = user_ids.to_vec();
    let row = client
        .query_one(
            "SELECT
                 EXISTS (
                     SELECT 1
                     FROM room_bans
                     WHERE target_user_id = ANY($1)
                       AND (expires_at IS NULL OR expires_at > current_timestamp)
                 )
                 OR EXISTS (
                     SELECT 1
                     FROM artboard_bans
                     WHERE target_user_id = ANY($1)
                       AND (expires_at IS NULL OR expires_at > current_timestamp)
                 )
                 OR EXISTS (
                     SELECT 1
                     FROM audio_bans
                     WHERE target_user_id = ANY($1)
                       AND (expires_at IS NULL OR expires_at > current_timestamp)
                 )",
            &[&user_ids],
        )
        .await?;
    let blocked: bool = row.get(0);
    if blocked {
        bail!("account linking is unavailable while either account has an active moderation ban");
    }
    Ok(())
}

async fn move_ssh_keys(
    client: &impl GenericClient,
    from_user_id: Uuid,
    to_user_id: Uuid,
) -> Result<()> {
    client
        .execute(
            "UPDATE user_ssh_keys
             SET user_id = $1, updated = current_timestamp
             WHERE user_id = $2",
            &[&to_user_id, &from_user_id],
        )
        .await?;
    Ok(())
}

fn generate_code() -> String {
    Uuid::new_v4()
        .as_bytes()
        .iter()
        .take(LINK_CODE_LEN)
        .map(|byte| LINK_CODE_ALPHABET[(*byte & 31) as usize] as char)
        .collect()
}

pub fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(16)
        .collect::<String>()
        .to_ascii_uppercase()
}
