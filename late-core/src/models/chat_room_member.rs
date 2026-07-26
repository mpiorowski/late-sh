use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
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
                 SELECT $1, resolved.user_id
                 FROM (
                     SELECT k.user_id
                     FROM user_ssh_keys k
                     WHERE k.fingerprint = $2
                     UNION
                     SELECT u.id
                     FROM users u
                     WHERE u.fingerprint = $2
                       AND NOT EXISTS (
                           SELECT 1
                           FROM user_ssh_keys k
                           WHERE k.fingerprint = $2
                       )
                 ) resolved
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

    /// Advance the member's read cursor to `read_at`, never moving it
    /// backwards. Returns 0 if the user is not a member of the room.
    pub async fn mark_read_at(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
        read_at: DateTime<Utc>,
    ) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE chat_room_members
                 SET last_read_at = GREATEST(
                    COALESCE(last_read_at, '-infinity'::timestamptz),
                    $3
                 )
                 WHERE room_id = $1 AND user_id = $2",
                &[&room_id, &user_id, &read_at],
            )
            .await?;
        Ok(count)
    }

    /// The member's read cursor, `None` if never read or not a member.
    pub async fn last_read_at(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "SELECT last_read_at
                 FROM chat_room_members
                 WHERE room_id = $1 AND user_id = $2",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(row.and_then(|row| row.get("last_read_at")))
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

    pub async fn list_memberships_for_users_in_rooms(
        client: &Client,
        user_ids: &[Uuid],
        room_ids: &[Uuid],
    ) -> Result<Vec<(Uuid, Uuid)>> {
        if user_ids.is_empty() || room_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = client
            .query(
                "SELECT user_id, room_id
                 FROM chat_room_members
                 WHERE user_id = ANY($1) AND room_id = ANY($2)
                 ORDER BY user_id, room_id",
                &[&user_ids, &room_ids],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("user_id"), row.get("room_id")))
            .collect())
    }

    pub async fn count_for_room(client: &Client, room_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint FROM chat_room_members WHERE room_id = $1",
                &[&room_id],
            )
            .await?;
        Ok(row.get(0))
    }

    pub async fn leave(client: &impl GenericClient, room_id: Uuid, user_id: Uuid) -> Result<u64> {
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
                // Auto-joined rooms start "read" so new users aren't flooded
                // with unread badges - EXCEPT #announcements, which is joined
                // with a NULL cursor so the login splash surfaces the recent
                // announcements the user has never seen.
                "INSERT INTO chat_room_members (room_id, user_id, last_read_at)
                 SELECT id, $1,
                        CASE WHEN slug = 'announcements' THEN NULL
                             ELSE current_timestamp END
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

    /// Unread counting stops here. A room the user never opened accumulates
    /// unread forever, and `#lounge` (auto-join, every user, 127k messages as
    /// of 2026-07-26) made the uncapped count 578 ms per user per pass and 43%
    /// of all database execution time. Nobody reads an exact four-digit badge,
    /// the room tail only ever loads `HISTORY_LIMIT` (500) messages anyway, and
    /// room ordering only tests `unread > 0`. So the count walks the index
    /// forward from `last_read_at` and stops here; the UI renders anything at
    /// the cap as `99+` (`chat/ui.rs::format_unread_badge`).
    ///
    /// The counting itself lives in `ChatRoom::list_for_user_with_state`, which
    /// returns the counts alongside the rooms they belong to in one query.
    pub const UNREAD_COUNT_CAP: i64 = 100;
}
