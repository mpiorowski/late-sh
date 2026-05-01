use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio_postgres::{Client, Row};
use uuid::Uuid;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ChatRoomMember {
    pub room_id: Uuid,
    pub user_id: Uuid,
    pub joined_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
}

impl From<Row> for ChatRoomMember {
    fn from(row: Row) -> Self {
        Self {
            room_id: row.get("room_id"),
            user_id: row.get("user_id"),
            joined_at: row.get("joined_at"),
            last_read_at: row.get("last_read_at"),
        }
    }
}

impl ChatRoomMember {
    pub async fn join(client: &Client, room_id: Uuid, user_id: Uuid) -> Result<Self> {
        if Self::is_banned_from_room(client, room_id, user_id).await? {
            bail!("user is banned from this room");
        }
        let row = client
            .query_one(
                "INSERT INTO chat_room_members (room_id, user_id)
                 VALUES ($1, $2)
                 ON CONFLICT (room_id, user_id)
                 DO UPDATE SET room_id = EXCLUDED.room_id
                 RETURNING *",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn is_banned_from_room(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT EXISTS(
                    SELECT 1
                    FROM room_bans
                    WHERE room_id = $1
                      AND target_user_id = $2
                      AND (expires_at IS NULL OR expires_at > current_timestamp)
                 )",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn join_user_by_fingerprint(
        client: &Client,
        room_id: Uuid,
        fingerprint: &str,
    ) -> Result<u64> {
        let count = client
            .execute(
                "INSERT INTO chat_room_members (room_id, user_id)
                 SELECT $1, id
                 FROM users
                 WHERE fingerprint = $2
                 ON CONFLICT (room_id, user_id) DO NOTHING",
                &[&room_id, &fingerprint],
            )
            .await?;
        Ok(count)
    }

    pub async fn mark_read_now(client: &Client, room_id: Uuid, user_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE chat_room_members
                 SET last_read_at = current_timestamp
                 WHERE room_id = $1 AND user_id = $2",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(count)
    }

    pub async fn is_member(client: &Client, room_id: Uuid, user_id: Uuid) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM chat_room_members WHERE room_id = $1 AND user_id = $2
                 )",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn list_user_ids(client: &Client, room_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = client
            .query(
                "SELECT user_id FROM chat_room_members WHERE room_id = $1 ORDER BY joined_at ASC",
                &[&room_id],
            )
            .await?;
        Ok(rows.into_iter().map(|r| r.get("user_id")).collect())
    }

    pub async fn leave(client: &Client, room_id: Uuid, user_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM chat_room_members WHERE room_id = $1 AND user_id = $2",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(count)
    }

    pub async fn auto_join_public_rooms(client: &Client, user_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "INSERT INTO chat_room_members (room_id, user_id)
                 SELECT id, $1
                 FROM chat_rooms
                 WHERE visibility = 'public' AND auto_join = true
                   AND NOT EXISTS (
                       SELECT 1
                       FROM room_bans
                       WHERE room_bans.room_id = chat_rooms.id
                         AND room_bans.target_user_id = $1
                         AND (room_bans.expires_at IS NULL OR room_bans.expires_at > current_timestamp)
                   )
                 ON CONFLICT (room_id, user_id) DO NOTHING",
                &[&user_id],
            )
            .await?;
        Ok(count)
    }

    pub async fn unread_counts_for_user(
        client: &Client,
        user_id: Uuid,
    ) -> Result<HashMap<Uuid, i64>> {
        let rows = client
            .query(
                "SELECT m.room_id, COUNT(msg.id)::bigint AS unread_count
                 FROM chat_room_members m
                 LEFT JOIN chat_messages msg
                   ON msg.room_id = m.room_id
                  AND msg.user_id <> m.user_id
                  AND msg.created > COALESCE(m.last_read_at, '-infinity'::timestamptz)
                 WHERE m.user_id = $1
                 GROUP BY m.room_id",
                &[&user_id],
            )
            .await?;

        let mut counts = HashMap::with_capacity(rows.len());
        for row in rows {
            counts.insert(row.get("room_id"), row.get("unread_count"));
        }
        Ok(counts)
    }
}
