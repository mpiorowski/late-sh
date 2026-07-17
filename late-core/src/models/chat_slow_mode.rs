use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "chat_slow_modes";
    params = ChatSlowModeParams;
    struct ChatSlowMode {
        @data
        pub room_id: Option<Uuid>,
        pub target_user_id: Uuid,
        pub actor_user_id: Uuid,
        pub interval_secs: i32,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

pub struct ChatSlowModeListItem {
    pub slow_mode: ChatSlowMode,
    pub room_slug: Option<String>,
    pub target_username: Option<String>,
    pub actor_username: Option<String>,
}

impl ChatSlowMode {
    pub async fn find_active_for_room_and_user(
        client: &Client,
        room_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM chat_slow_modes
                 WHERE room_id = $1
                   AND target_user_id = $2
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&room_id, &target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_active_server_for_user(
        client: &Client,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM chat_slow_modes
                 WHERE room_id IS NULL
                   AND target_user_id = $1
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn active_with_usernames_page(
        client: &Client,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ChatSlowModeListItem>> {
        let rows = client
            .query(
                "SELECT csm.*, room.slug AS room_slug,
                        target.username AS target_username,
                        actor.username AS actor_username
                 FROM chat_slow_modes csm
                 LEFT JOIN chat_rooms room ON room.id = csm.room_id
                 LEFT JOIN users target ON target.id = csm.target_user_id
                 LEFT JOIN users actor ON actor.id = csm.actor_user_id
                 WHERE csm.expires_at IS NULL OR csm.expires_at > current_timestamp
                 ORDER BY csm.created DESC
                 LIMIT $1 OFFSET $2",
                &[&limit, &offset],
            )
            .await?;
        Ok(rows.into_iter().map(Self::list_item_from_row).collect())
    }

    pub async fn active_for_room_with_usernames_page(
        client: &Client,
        room_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ChatSlowModeListItem>> {
        let rows = client
            .query(
                "SELECT csm.*, room.slug AS room_slug,
                        target.username AS target_username,
                        actor.username AS actor_username
                 FROM chat_slow_modes csm
                 LEFT JOIN chat_rooms room ON room.id = csm.room_id
                 LEFT JOIN users target ON target.id = csm.target_user_id
                 LEFT JOIN users actor ON actor.id = csm.actor_user_id
                 WHERE csm.room_id = $1
                   AND (csm.expires_at IS NULL OR csm.expires_at > current_timestamp)
                 ORDER BY csm.created DESC
                 LIMIT $2 OFFSET $3",
                &[&room_id, &limit, &offset],
            )
            .await?;
        Ok(rows.into_iter().map(Self::list_item_from_row).collect())
    }

    pub async fn active_server_with_usernames_page(
        client: &Client,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ChatSlowModeListItem>> {
        let rows = client
            .query(
                "SELECT csm.*, room.slug AS room_slug,
                        target.username AS target_username,
                        actor.username AS actor_username
                 FROM chat_slow_modes csm
                 LEFT JOIN chat_rooms room ON room.id = csm.room_id
                 LEFT JOIN users target ON target.id = csm.target_user_id
                 LEFT JOIN users actor ON actor.id = csm.actor_user_id
                 WHERE csm.room_id IS NULL
                   AND (csm.expires_at IS NULL OR csm.expires_at > current_timestamp)
                 ORDER BY csm.created DESC
                 LIMIT $1 OFFSET $2",
                &[&limit, &offset],
            )
            .await?;
        Ok(rows.into_iter().map(Self::list_item_from_row).collect())
    }

    pub async fn activate(
        client: &impl GenericClient,
        room_id: Uuid,
        target_user_id: Uuid,
        actor_user_id: Uuid,
        interval_secs: i32,
        reason: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let reason = reason.into();
        let row = client
            .query_one(
                "INSERT INTO chat_slow_modes
                 (room_id, target_user_id, actor_user_id, interval_secs, reason, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (room_id, target_user_id) WHERE room_id IS NOT NULL
                 DO UPDATE SET actor_user_id = EXCLUDED.actor_user_id,
                               interval_secs = EXCLUDED.interval_secs,
                               reason = EXCLUDED.reason,
                               expires_at = EXCLUDED.expires_at,
                               updated = current_timestamp
                 RETURNING *",
                &[
                    &room_id,
                    &target_user_id,
                    &actor_user_id,
                    &interval_secs,
                    &reason,
                    &expires_at,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn activate_server(
        client: &impl GenericClient,
        target_user_id: Uuid,
        actor_user_id: Uuid,
        interval_secs: i32,
        reason: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let reason = reason.into();
        let row = client
            .query_one(
                "INSERT INTO chat_slow_modes
                 (room_id, target_user_id, actor_user_id, interval_secs, reason, expires_at)
                 VALUES (NULL, $1, $2, $3, $4, $5)
                 ON CONFLICT (target_user_id) WHERE room_id IS NULL
                 DO UPDATE SET actor_user_id = EXCLUDED.actor_user_id,
                               interval_secs = EXCLUDED.interval_secs,
                               reason = EXCLUDED.reason,
                               expires_at = EXCLUDED.expires_at,
                               updated = current_timestamp
                 RETURNING *",
                &[
                    &target_user_id,
                    &actor_user_id,
                    &interval_secs,
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
                "DELETE FROM chat_slow_modes WHERE room_id = $1 AND target_user_id = $2",
                &[&room_id, &target_user_id],
            )
            .await?;
        Ok(count)
    }

    pub async fn delete_server_for_user(
        client: &impl GenericClient,
        target_user_id: Uuid,
    ) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM chat_slow_modes WHERE room_id IS NULL AND target_user_id = $1",
                &[&target_user_id],
            )
            .await?;
        Ok(count)
    }

    fn list_item_from_row(row: tokio_postgres::Row) -> ChatSlowModeListItem {
        let room_slug: Option<String> = row.get("room_slug");
        let target_username: Option<String> = row.get("target_username");
        let actor_username: Option<String> = row.get("actor_username");
        ChatSlowModeListItem {
            slow_mode: Self::from(row),
            room_slug,
            target_username,
            actor_username,
        }
    }
}
