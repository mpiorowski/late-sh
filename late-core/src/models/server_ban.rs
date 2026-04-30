use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "server_bans";
    params = ServerBanParams;
    struct ServerBan {
        @data
        pub target_user_id: Uuid,
        pub fingerprint: Option<String>,
        pub ip_address: Option<String>,
        pub snapshot_username: Option<String>,
        pub actor_user_id: Uuid,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

pub struct ServerBanActivation<'a> {
    pub target_user_id: Uuid,
    pub fingerprint: Option<&'a str>,
    pub ip_address: Option<&'a str>,
    pub snapshot_username: Option<&'a str>,
    pub actor_user_id: Uuid,
    pub reason: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ServerBan {
    pub async fn activate(
        client: &impl GenericClient,
        activation: ServerBanActivation<'_>,
    ) -> Result<Self> {
        let fingerprint = activation.fingerprint.map(str::to_string);
        let ip_address = activation.ip_address.map(str::to_string);
        let snapshot_username = activation.snapshot_username.map(str::to_string);
        let reason = activation.reason.to_string();
        let row = client
            .query_one(
                "INSERT INTO server_bans
                 (target_user_id, fingerprint, ip_address, snapshot_username,
                  actor_user_id, reason, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING *",
                &[
                    &activation.target_user_id,
                    &fingerprint,
                    &ip_address,
                    &snapshot_username,
                    &activation.actor_user_id,
                    &reason,
                    &activation.expires_at,
                ],
            )
            .await?;
        Ok(Self::from(row))
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
                 WHERE sb.expires_at IS NULL OR sb.expires_at > current_timestamp
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
                   AND (expires_at IS NULL OR expires_at > current_timestamp)
                 ORDER BY created DESC
                 LIMIT 1",
                &[&ip_address],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn delete_active_for_user(
        client: &impl GenericClient,
        target_user_id: Uuid,
        fingerprint: &str,
    ) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM server_bans
                 WHERE (
                       target_user_id = $1
                       OR fingerprint = $2
                   )
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&target_user_id, &fingerprint],
            )
            .await?)
    }

    pub async fn delete_active_for_ip_address(
        client: &impl GenericClient,
        ip_address: &str,
    ) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM server_bans
                 WHERE ip_address = $1
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&ip_address],
            )
            .await?)
    }
}
