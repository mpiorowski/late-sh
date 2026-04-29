use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "server_bans";
    params = ServerBanParams;
    struct ServerBan {
        @data
        pub ban_type: String,
        pub target_user_id: Option<Uuid>,
        pub fingerprint: Option<String>,
        pub ip_address: Option<String>,
        pub snapshot_username: Option<String>,
        pub actor_user_id: Uuid,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

impl ServerBan {
    pub async fn activate(
        client: &Client,
        target_user_id: Uuid,
        fingerprint: &str,
        actor_user_id: Uuid,
        reason: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        Self::create(
            client,
            ServerBanParams {
                ban_type: "user".to_string(),
                target_user_id: Some(target_user_id),
                fingerprint: Some(fingerprint.to_string()),
                ip_address: None,
                snapshot_username: None,
                actor_user_id,
                reason: reason.to_string(),
                expires_at,
            },
        )
        .await
    }

    pub async fn activate_fingerprint(
        client: &Client,
        fingerprint: &str,
        actor_user_id: Uuid,
        reason: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        Self::create(
            client,
            ServerBanParams {
                ban_type: "fingerprint".to_string(),
                target_user_id: None,
                fingerprint: Some(fingerprint.to_string()),
                ip_address: None,
                snapshot_username: None,
                actor_user_id,
                reason: reason.to_string(),
                expires_at,
            },
        )
        .await
    }

    pub async fn activate_ip(
        client: &Client,
        ip_address: &str,
        snapshot_username: Option<&str>,
        snapshot_fingerprint: Option<&str>,
        actor_user_id: Uuid,
        reason: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        Self::create(
            client,
            ServerBanParams {
                ban_type: "ip".to_string(),
                target_user_id: None,
                fingerprint: snapshot_fingerprint.map(str::to_string),
                ip_address: Some(ip_address.to_string()),
                snapshot_username: snapshot_username.map(str::to_string),
                actor_user_id,
                reason: reason.to_string(),
                expires_at,
            },
        )
        .await
    }

    pub async fn find_active_for_user_id(
        client: &Client,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM server_bans
                 WHERE target_user_id = $1
                   AND ban_type = 'user'
                   AND (expires_at IS NULL OR expires_at > current_timestamp)
                 ORDER BY created DESC
                 LIMIT 1",
                &[&target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn active_with_actor_username(
        client: &Client,
    ) -> Result<Vec<(Self, Option<String>)>> {
        let rows = client
            .query(
                "SELECT sb.*, u.username AS actor_username
                 FROM server_bans sb
                 LEFT JOIN users u ON u.id = sb.actor_user_id
                 WHERE sb.target_user_id IS NOT NULL
                   AND sb.ban_type = 'user'
                   AND (sb.expires_at IS NULL OR sb.expires_at > current_timestamp)
                 ORDER BY sb.created DESC",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let actor_username: Option<String> = row.get("actor_username");
                (Self::from(row), actor_username)
            })
            .collect())
    }

    pub async fn find_active_for_fingerprint(
        client: &Client,
        fingerprint: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM server_bans
                 WHERE fingerprint = $1
                   AND ban_type = 'fingerprint'
                   AND (expires_at IS NULL OR expires_at > current_timestamp)
                 ORDER BY created DESC
                 LIMIT 1",
                &[&fingerprint],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_active_for_ip_address(
        client: &Client,
        ip_address: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM server_bans
                 WHERE ip_address = $1
                   AND ban_type = 'ip'
                   AND (expires_at IS NULL OR expires_at > current_timestamp)
                 ORDER BY created DESC
                 LIMIT 1",
                &[&ip_address],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn delete_active_for_user(
        client: &Client,
        target_user_id: Uuid,
        fingerprint: &str,
    ) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM server_bans
                 WHERE (
                       (ban_type = 'user' AND target_user_id = $1)
                       OR (ban_type = 'fingerprint' AND fingerprint = $2)
                   )
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&target_user_id, &fingerprint],
            )
            .await?)
    }

    pub async fn delete_active_for_ip_address(client: &Client, ip_address: &str) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM server_bans
                 WHERE ip_address = $1
                   AND ban_type = 'ip'
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&ip_address],
            )
            .await?)
    }
}
