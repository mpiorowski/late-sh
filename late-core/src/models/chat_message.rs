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
