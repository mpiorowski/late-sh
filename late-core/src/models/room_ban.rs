use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "room_bans";
    params = RoomBanParams;
    struct RoomBan {
        @data
        pub room_id: Uuid,
        pub target_user_id: Uuid,
        pub actor_user_id: Uuid,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

impl RoomBan {
    pub async fn find_for_room_and_user(
        client: &Client,
        room_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM room_bans
                 WHERE room_id = $1 AND target_user_id = $2",
                &[&room_id, &target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_active_for_room_and_user(
        client: &Client,
        room_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM room_bans
                 WHERE room_id = $1
                   AND target_user_id = $2
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&room_id, &target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn is_active_for_room_and_user(
        client: &Client,
        room_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<bool> {
        Ok(
            Self::find_active_for_room_and_user(client, room_id, target_user_id)
                .await?
                .is_some(),
        )
    }

    pub async fn activate(
        client: &impl GenericClient,
        room_id: Uuid,
        target_user_id: Uuid,
        actor_user_id: Uuid,
        reason: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let reason = reason.into();
        let row = client
            .query_one(
                "INSERT INTO room_bans
                 (room_id, target_user_id, actor_user_id, reason, expires_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (room_id, target_user_id)
                 DO UPDATE SET actor_user_id = EXCLUDED.actor_user_id,
                               reason = EXCLUDED.reason,
                               expires_at = EXCLUDED.expires_at,
                               updated = current_timestamp
                 RETURNING *",
                &[
                    &room_id,
                    &target_user_id,
                    &actor_user_id,
                    &reason,
                    &expires_at,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn delete_for_room_and_user(
        client: &impl GenericClient,
        room_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM room_bans WHERE room_id = $1 AND target_user_id = $2",
                &[&room_id, &target_user_id],
            )
            .await?;
        Ok(count)
    }
}
