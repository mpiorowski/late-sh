use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use std::collections::HashMap;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "chat_messages";
    params = ChatMessageParams;
    struct ChatMessage {
        @generated
        pub pinned: bool,
        pub reply_to_message_id: Option<Uuid>,
        pub reply_to_user_id: Option<Uuid>;
        @data
        pub room_id: Uuid,
        pub user_id: Uuid,
        pub body: String,
    }
}

impl ChatMessage {
    pub async fn last_message_at_for_rooms(
        client: &Client,
        room_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Option<DateTime<Utc>>>> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT room_ids.room_id,
                        latest.created AS last_message_at
                 FROM unnest($1::uuid[]) AS room_ids(room_id)
                 LEFT JOIN LATERAL (
                    SELECT created
                    FROM chat_messages
                    WHERE room_id = room_ids.room_id
                    ORDER BY created DESC, id DESC
                    LIMIT 1
                 ) latest ON true",
                &[&room_ids],
            )
            .await?;

        let mut last_message_at = HashMap::with_capacity(rows.len());
        for row in rows {
            last_message_at.insert(row.get("room_id"), row.get("last_message_at"));
        }

        Ok(last_message_at)
    }

    pub async fn list_recent_for_rooms(
        client: &Client,
        room_ids: &[Uuid],
        limit_per_room: i64,
    ) -> Result<HashMap<Uuid, Vec<Self>>> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT cm.*
                 FROM (
                    SELECT DISTINCT room_id
                    FROM unnest($1::uuid[]) AS room_ids(room_id)
                 ) room_ids
                 JOIN LATERAL (
                    SELECT *
                    FROM chat_messages cm
                    WHERE cm.room_id = room_ids.room_id
                    ORDER BY cm.created DESC, cm.id DESC
                    LIMIT $2
                 ) cm ON true
                 ORDER BY cm.room_id, cm.created DESC, cm.id DESC",
                &[&room_ids, &limit_per_room],
            )
            .await?;

        let mut messages_by_room: HashMap<Uuid, Vec<Self>> = HashMap::new();
        for row in rows {
            let msg = Self::from(row);
            messages_by_room.entry(msg.room_id).or_default().push(msg);
        }

        Ok(messages_by_room)
    }

    pub async fn list_recent(client: &Client, room_id: Uuid, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM chat_messages
                 WHERE room_id = $1
                 ORDER BY created DESC, id DESC
                 LIMIT $2",
                &[&room_id, &limit],
            )
            .await?;

        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_pinned(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM chat_messages
                 WHERE pinned = true
                 ORDER BY created DESC, id DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;

        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_before(
        client: &Client,
        room_id: Uuid,
        before_created: DateTime<Utc>,
        before_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM chat_messages
                 WHERE room_id = $1
                   AND (created, id) < ($2, $3)
                 ORDER BY created DESC, id DESC
                 LIMIT $4",
                &[&room_id, &before_created, &before_id, &limit],
            )
            .await?;

        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_after(
        client: &Client,
        room_id: Uuid,
        after_created: DateTime<Utc>,
        after_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM chat_messages
                 WHERE room_id = $1
                   AND (created, id) > ($2, $3)
                 ORDER BY created ASC, id ASC
                 LIMIT $4",
                &[&room_id, &after_created, &after_id, &limit],
            )
            .await?;

        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Up to `limit_each` messages immediately before and after a message in
    /// its room, both in chronological order. System-feed authors and
    /// `exclude_user_ids` (the caller's ignored users) are skipped so the
    /// window shows conversation, not feed noise. Callers must verify room
    /// membership first; used for the search-hit context window.
    pub async fn list_around(
        client: &Client,
        room_id: Uuid,
        created: DateTime<Utc>,
        id: Uuid,
        exclude_user_ids: &[Uuid],
        limit_each: i64,
    ) -> Result<(Vec<Self>, Vec<Self>)> {
        let before_rows = client
            .query(
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN users author ON author.id = msg.user_id
                 WHERE msg.room_id = $1
                   AND (msg.created, msg.id) < ($2, $3)
                   AND msg.user_id <> ALL($4::uuid[])
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created DESC, msg.id DESC
                 LIMIT $5",
                &[&room_id, &created, &id, &exclude_user_ids, &limit_each],
            )
            .await?;
        let mut before: Vec<Self> = before_rows.into_iter().map(Self::from).collect();
        before.reverse();

        let after_rows = client
            .query(
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN users author ON author.id = msg.user_id
                 WHERE msg.room_id = $1
                   AND (msg.created, msg.id) > ($2, $3)
                   AND msg.user_id <> ALL($4::uuid[])
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created ASC, msg.id ASC
                 LIMIT $5",
                &[&room_id, &created, &id, &exclude_user_ids, &limit_each],
            )
            .await?;
        let after: Vec<Self> = after_rows.into_iter().map(Self::from).collect();

        Ok((before, after))
    }

    /// Fetch one message the viewer is allowed to preview: any message in a
    /// room they are a member of, or in a public non-game room (Discover
    /// already shows recent messages of those to non-members). Used by the
    /// Ctrl+/ modal to preview a mention whose message is older than the
    /// loaded history; public-room mentions can target non-members, so
    /// membership alone would wrongly reject them.
    pub async fn get_for_viewer(
        client: &Client,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN chat_rooms room ON room.id = msg.room_id
                 WHERE msg.id = $1
                   AND (
                     (room.visibility = 'public' AND room.kind <> 'game')
                     OR EXISTS (
                        SELECT 1 FROM chat_room_members mem
                        WHERE mem.room_id = msg.room_id AND mem.user_id = $2
                     )
                   )",
                &[&message_id, &user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Substring search over message bodies across every room the user is a
    /// member of (the membership join is the authorization boundary), newest
    /// first. Game rooms are excluded to match their invisibility elsewhere,
    /// and system-feed bot lines (users.settings.system) are excluded so the
    /// #lounge activity feed cannot drown real results. `exclude_user_ids`
    /// carries the caller's ignored users. `room_id` scopes to one room.
    pub async fn search_for_user(
        client: &Client,
        user_id: Uuid,
        query: &str,
        room_id: Option<Uuid>,
        exclude_user_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Self>> {
        let pattern = format!("%{}%", escape_like_pattern(query));
        let rows = client
            .query(
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN chat_room_members mem
                   ON mem.room_id = msg.room_id AND mem.user_id = $1
                 JOIN chat_rooms room ON room.id = msg.room_id
                 JOIN users author ON author.id = msg.user_id
                 WHERE msg.body ILIKE $2 ESCAPE '\\'
                   AND room.kind <> 'game'
                   AND ($3::uuid IS NULL OR msg.room_id = $3)
                   AND msg.user_id <> ALL($4::uuid[])
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created DESC, msg.id DESC
                 LIMIT $5",
                &[&user_id, &pattern, &room_id, &exclude_user_ids, &limit],
            )
            .await?;

        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn create_with_reply_to(
        client: &impl GenericClient,
        params: ChatMessageParams,
        reply_to_message_id: Option<Uuid>,
    ) -> Result<Self> {
        Self::create_with_reply_targets(client, params, reply_to_message_id, None).await
    }

    /// Create a message, optionally recording both the replied-to message and
    /// the user this message is a response to. `reply_to_user_id` is used to
    /// filter bot replies for viewers who ignore the triggering user.
    pub async fn create_with_reply_targets(
        client: &impl GenericClient,
        params: ChatMessageParams,
        reply_to_message_id: Option<Uuid>,
        reply_to_user_id: Option<Uuid>,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO chat_messages (room_id, user_id, body, reply_to_message_id, reply_to_user_id)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING *",
                &[
                    &params.room_id,
                    &params.user_id,
                    &params.body,
                    &reply_to_message_id,
                    &reply_to_user_id,
                ],
            )
            .await?;

        Ok(Self::from(row))
    }

    pub async fn edit_by_author(
        client: &impl GenericClient,
        message_id: Uuid,
        user_id: Uuid,
        body: &str,
    ) -> Result<Option<Self>> {
        let body = body.trim();
        if body.is_empty() {
            bail!("message body cannot be empty");
        }

        let row = client
            .query_opt(
                "UPDATE chat_messages
                 SET body = $1, updated = current_timestamp
                 WHERE id = $2 AND user_id = $3
                 RETURNING *",
                &[&body, &message_id, &user_id],
            )
            .await?;

        Ok(row.map(Self::from))
    }

    pub async fn edit_after_authorization(
        client: &impl GenericClient,
        message_id: Uuid,
        body: &str,
    ) -> Result<Self> {
        let body = body.trim();
        if body.is_empty() {
            bail!("message body cannot be empty");
        }

        let row = client
            .query_one(
                "UPDATE chat_messages
                 SET body = $1, updated = current_timestamp
                 WHERE id = $2
                 RETURNING *",
                &[&body, &message_id],
            )
            .await?;

        Ok(Self::from(row))
    }

    pub async fn delete_by_author(
        client: &impl GenericClient,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM chat_messages WHERE id = $1 AND user_id = $2",
                &[&message_id, &user_id],
            )
            .await?;
        Ok(count)
    }

    pub async fn delete_by_admin(client: &impl GenericClient, message_id: Uuid) -> Result<u64> {
        let count = client
            .execute("DELETE FROM chat_messages WHERE id = $1", &[&message_id])
            .await?;
        Ok(count)
    }

    pub async fn set_pinned(client: &Client, message_id: Uuid, pinned: bool) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE chat_messages
                 SET pinned = $2, updated = current_timestamp
                 WHERE id = $1
                 RETURNING *",
                &[&message_id, &pinned],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Delete news announcement chat messages posted by a specific user
    /// that contain the given marker and URL, returning `(room_id, message_id)`
    /// for each removed row.
    pub async fn delete_news_by_user_and_url(
        client: &impl GenericClient,
        user_id: Uuid,
        news_marker: &str,
        url: &str,
    ) -> Result<Vec<(Uuid, Uuid)>> {
        let rows = client
            .query(
                "DELETE FROM chat_messages
                 WHERE user_id = $1
                   AND strpos(body, $2) > 0
                   AND strpos(body, $3) > 0
                 RETURNING room_id, id",
                &[&user_id, &news_marker, &url],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("room_id"), row.get("id")))
            .collect())
    }
}

/// Escape `%`, `_`, and `\` in a user-supplied query so it matches literally
/// inside an ILIKE `%...%` pattern (paired with `ESCAPE '\'` in the SQL).
pub fn escape_like_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for ch in query.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_like_pattern;

    #[test]
    fn escape_like_pattern_escapes_metacharacters() {
        assert_eq!(escape_like_pattern("plain query"), "plain query");
        assert_eq!(escape_like_pattern("100%"), "100\\%");
        assert_eq!(escape_like_pattern("snake_case"), "snake\\_case");
        assert_eq!(escape_like_pattern("back\\slash"), "back\\\\slash");
    }
}
