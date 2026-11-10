use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "notifications";
    user_field = user_id;
    params = NotificationParams;
    struct Notification {
        @data
        pub user_id: Uuid,
        pub actor_id: Uuid,
        pub message_id: Uuid,
        pub room_id: Uuid
    }
}

/// Joined view used for display in the notifications list.
#[derive(Debug, Clone)]
pub struct NotificationView {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub user_id: Uuid,
    pub actor_id: Uuid,
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub actor_username: String,
    pub room_slug: Option<String>,
    /// The mentioning message's whole body: the reader cuts it down, after
    /// masking what the cut must not split (late-ssh's `NotificationService`).
    pub message_preview: String,
    /// When this mention was read, `None` while it is unread: the one signal
    /// behind [`Notification::unread_count`], the list's dot and the attention
    /// guest's rows.
    pub read_at: Option<DateTime<Utc>>,
}

impl Notification {
    /// Bulk-insert mention notifications for multiple users.
    pub async fn create_mentions_batch(
        client: &Client,
        user_ids: &[Uuid],
        actor_id: Uuid,
        message_id: Uuid,
        room_id: Uuid,
    ) -> Result<u64> {
        if user_ids.is_empty() {
            return Ok(0);
        }

        let count = client
            .execute(
                "INSERT INTO notifications (user_id, actor_id, message_id, room_id)
                 SELECT mentioned.user_id, $1, $2, $3
                 FROM UNNEST($4::uuid[]) AS mentioned(user_id)
                 JOIN users recipient ON recipient.id = mentioned.user_id
                 WHERE NOT (
                    COALESCE(recipient.settings, '{}'::jsonb)
                    @> jsonb_build_object('ignored_user_ids', jsonb_build_array(($1::uuid)::text))
                 )
                 ON CONFLICT DO NOTHING",
                &[&actor_id, &message_id, &room_id, &user_ids],
            )
            .await?;
        Ok(count)
    }

    /// List recent notifications for a user with joined display data.
    pub async fn list_for_user(
        client: &Client,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<NotificationView>> {
        let rows = client
            .query(
                "SELECT n.id, n.created, n.user_id, n.actor_id, n.message_id, n.room_id,
                        COALESCE(u.username, '') AS actor_username,
                        r.slug AS room_slug,
                        m.body AS message_preview,
                        n.read_at
                 FROM notifications n
                 JOIN users u ON u.id = n.actor_id
                 JOIN users recipient ON recipient.id = n.user_id
                 JOIN chat_rooms r ON r.id = n.room_id
                 JOIN chat_messages m ON m.id = n.message_id
                 WHERE n.user_id = $1
                   AND NOT (
                        COALESCE(recipient.settings, '{}'::jsonb)
                        @> jsonb_build_object(
                            'ignored_user_ids',
                            jsonb_build_array(n.actor_id::text)
                        )
                   )
                   AND r.kind <> 'game'
                   AND (
                        r.kind = 'dm'
                        OR r.permanent = true
                        OR r.visibility IN ('public', 'private')
                   )
                 ORDER BY n.created DESC
                 LIMIT $2",
                &[&user_id, &limit],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| NotificationView {
                id: row.get("id"),
                created: row.get("created"),
                user_id: row.get("user_id"),
                actor_id: row.get("actor_id"),
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                actor_username: row.get("actor_username"),
                room_slug: row.get("room_slug"),
                message_preview: row.get("message_preview"),
                read_at: row.get("read_at"),
            })
            .collect())
    }

    /// Count unread notifications for a user.
    ///
    /// A mention is unread while its `read_at` is empty, and that is the only
    /// signal. Two acts stamp it: opening the Mentions entry
    /// ([`Self::mark_all_read`]) and the message it rides on being rendered in
    /// its room ([`Self::mark_read_for_messages`]). The room's coarse
    /// `chat_room_members.last_read_at` cursor deliberately plays no part
    /// here: it moves whenever the room is merely opened, which would clear
    /// mentions sitting above the loaded tail that were never on screen.
    pub async fn unread_count(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(n.id)::bigint AS unread_count
                 FROM notifications n
                 JOIN chat_rooms r ON r.id = n.room_id
                 JOIN users recipient ON recipient.id = n.user_id
                 WHERE n.user_id = $1
                   AND n.read_at IS NULL
                   AND NOT (
                        COALESCE(recipient.settings, '{}'::jsonb)
                        @> jsonb_build_object(
                            'ignored_user_ids',
                            jsonb_build_array(n.actor_id::text)
                        )
                   )
                   AND r.kind <> 'game'
                   AND (
                        r.kind = 'dm'
                        OR r.permanent = true
                        OR r.visibility IN ('public', 'private')
                   )",
                &[&user_id],
            )
            .await?;
        Ok(row.get("unread_count"))
    }

    /// Stamp `read_at` on every mention this user still has unread. Returns
    /// the ids it stamped, so the Mentions list can keep its dots on them for
    /// the visit that read them.
    pub async fn mark_all_read(client: &Client, user_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = client
            .query(
                "UPDATE notifications
                 SET read_at = current_timestamp, updated = current_timestamp
                 WHERE user_id = $1
                   AND read_at IS NULL
                 RETURNING id",
                &[&user_id],
            )
            .await?;
        Ok(rows.iter().map(|row| row.get("id")).collect())
    }

    /// Stamp `read_at` on this user's mentions riding on the given messages.
    /// Called when those messages are rendered in their own room; a message id
    /// with no mention row for this user is a no-op. Returns how many mentions
    /// were newly marked read, so the caller knows whether the unread count
    /// moved.
    pub async fn mark_read_for_messages(
        client: &Client,
        user_id: Uuid,
        message_ids: &[Uuid],
    ) -> Result<u64> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let count = client
            .execute(
                "UPDATE notifications
                 SET read_at = current_timestamp, updated = current_timestamp
                 WHERE user_id = $1
                   AND message_id = ANY($2)
                   AND read_at IS NULL",
                &[&user_id, &message_ids],
            )
            .await?;
        Ok(count)
    }

    /// Resolve @usernames to user IDs, excluding the actor.
    ///
    /// For DM rooms, only resolves usernames that belong to one of the two DM
    /// participants. For private rooms, only resolves usernames that are
    /// members of the room. Public rooms resolve against all users, except
    /// the hidden kinds (`chat_room::HIDDEN_ROOM_KINDS`), where a name said
    /// out loud mentions nobody: the room is only ever seen from its own
    /// screen, so a mention would point at a place the rail cannot open.
    pub async fn resolve_mentioned_user_ids(
        client: &Client,
        usernames: &[String],
        exclude_user_id: Uuid,
        room_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        if usernames.is_empty() {
            return Ok(Vec::new());
        }

        let lower: Vec<String> = usernames.iter().map(|u| u.to_ascii_lowercase()).collect();
        let rows = client
            .query(
                "SELECT u.id \
                 FROM users u \
                 JOIN chat_rooms r ON r.id = $3 \
                 LEFT JOIN chat_room_members m \
                   ON m.room_id = r.id AND m.user_id = u.id \
                 WHERE LOWER(u.username) = ANY($1) \
                   AND u.id <> $2 \
                   AND NOT (
                        COALESCE(u.settings, '{}'::jsonb)
                        @> jsonb_build_object(
                            'ignored_user_ids',
                            jsonb_build_array(($2::uuid)::text)
                        )
                   ) \
                   AND r.kind <> 'game' \
                   AND r.kind <> ALL($4::text[]) \
                   AND (
                        (r.kind = 'dm' AND u.id IN (r.dm_user_a, r.dm_user_b))
                        OR (r.kind <> 'dm' AND r.visibility = 'private' AND m.user_id IS NOT NULL)
                        OR (r.kind <> 'dm' AND r.visibility = 'public')
                   )",
                &[
                    &lower,
                    &exclude_user_id,
                    &room_id,
                    &crate::models::chat_room::HIDDEN_ROOM_KINDS,
                ],
            )
            .await?;

        Ok(rows.iter().map(|r| r.get("id")).collect())
    }
}
