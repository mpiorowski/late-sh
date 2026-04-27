use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::mention_feed_read::MentionFeedRead;

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
    pub message_preview: String,
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

        // Build a multi-row INSERT: ($1, $2, $3, $4), ($5, $2, $3, $4), ...
        // where $2=actor_id, $3=message_id, $4=room_id are shared, and each $N is a user_id.
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            Vec::with_capacity(user_ids.len() + 3);
        params.push(&actor_id); // $1
        params.push(&message_id); // $2
        params.push(&room_id); // $3

        let mut value_clauses = Vec::with_capacity(user_ids.len());
        for (i, uid) in user_ids.iter().enumerate() {
            params.push(uid); // $4, $5, $6, ...
            value_clauses.push(format!("(${}, $1, $2, $3)", i + 4));
        }

        let query = format!(
            "INSERT INTO notifications (user_id, actor_id, message_id, room_id) VALUES {} ON CONFLICT DO NOTHING",
            value_clauses.join(", ")
        );

        let count = client.execute(&query, &params).await?;
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
                        LEFT(m.body, 120) AS message_preview
                 FROM notifications n
                 JOIN users u ON u.id = n.actor_id
                 JOIN chat_rooms r ON r.id = n.room_id
                 JOIN chat_messages m ON m.id = n.message_id
                 WHERE n.user_id = $1
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
            })
            .collect())
    }

    /// Count unread notifications for a user.
    pub async fn unread_count(client: &Client, user_id: Uuid) -> Result<i64> {
        MentionFeedRead::unread_count_for_user(client, user_id).await
    }

    /// Mark all unread notifications as read for a user.
    pub async fn mark_all_read(client: &Client, user_id: Uuid) -> Result<()> {
        MentionFeedRead::mark_read_now(client, user_id).await
    }

    pub async fn last_read_at(client: &Client, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        MentionFeedRead::last_read_at(client, user_id).await
    }

    /// Resolve @usernames to user IDs, excluding the actor.
    ///
    /// For DM rooms, only resolves usernames that belong to one of the two DM
    /// participants. For private rooms, only resolves usernames that are
    /// members of the room. Public rooms resolve against all users.
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
                   AND (
                        (r.kind = 'dm' AND u.id IN (r.dm_user_a, r.dm_user_b))
                        OR (r.kind <> 'dm' AND r.visibility = 'private' AND m.user_id IS NOT NULL)
                        OR (r.kind <> 'dm' AND r.visibility = 'public')
                   )",
                &[&lower, &exclude_user_id, &room_id],
            )
            .await?;

        Ok(rows.iter().map(|r| r.get("id")).collect())
    }
}
