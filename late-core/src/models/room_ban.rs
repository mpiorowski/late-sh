use anyhow::Result;
use chrono::{DateTime, Utc};
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
        client: &Client,
        room_id: Uuid,
        target_user_id: Uuid,
        actor_user_id: Uuid,
        reason: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let params = RoomBanParams {
            room_id,
            target_user_id,
            actor_user_id,
            reason: reason.into(),
            expires_at,
        };

        if let Some(existing) =
            Self::find_for_room_and_user(client, room_id, target_user_id).await?
        {
            Self::update(client, existing.id, params).await
        } else {
            Self::create(client, params).await
        }
    }

    pub async fn delete_for_room_and_user(
        client: &Client,
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
