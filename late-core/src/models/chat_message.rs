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
        pub reply_to_message_id: Option<Uuid>,
        pub reply_to_user_id: Option<Uuid>;
        @data
        pub room_id: Uuid,
        pub user_id: Uuid,
        pub body: String,
    }
}

/// Which way a history page walks from its cursor: `Older` back toward the
/// start of the room, `Newer` forward toward the tail. Both directions hand
/// their page back oldest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDirection {
    Older,
    Newer,
}

impl ChatMessage {
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

    /// When the user last sent a message in a room, if ever. Drives the
    /// per-room slow-mode cooldown.
    pub async fn last_sent_at_in_room(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "SELECT created
                 FROM chat_messages
                 WHERE room_id = $1 AND user_id = $2
                 ORDER BY created DESC, id DESC
                 LIMIT 1",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(row.map(|row| row.get("created")))
    }

    /// When the user last sent a message in any non-DM room, if ever. Drives
    /// the server-wide slow-mode cooldown.
    pub async fn last_sent_at_in_public_rooms(
        client: &Client,
        user_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "SELECT cm.created
                 FROM chat_messages cm
                 JOIN chat_rooms cr ON cr.id = cm.room_id
                 WHERE cm.user_id = $1 AND cr.kind <> 'dm'
                 ORDER BY cm.created DESC, cm.id DESC
                 LIMIT 1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|row| row.get("created")))
    }

    /// Unread messages authored by other users in a room the user is a member
    /// of, newest first, capped at `limit`. Unread is relative to the member's
    /// `last_read_at`. Used by the login announcements splash.
    pub async fn list_unread_for_member(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<UnreadRoomMessage>> {
        let rows = client
            .query(
                "SELECT msg.id,
                        msg.created,
                        msg.body,
                        u.username AS author
                 FROM chat_room_members member
                 JOIN chat_messages msg ON msg.room_id = member.room_id
                 JOIN users u ON u.id = msg.user_id
                 WHERE member.room_id = $1
                   AND member.user_id = $2
                   AND msg.user_id <> $2
                   AND msg.created > COALESCE(member.last_read_at, '-infinity'::timestamptz)
                 ORDER BY msg.created DESC, msg.id DESC
                 LIMIT $3",
                &[&room_id, &user_id, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| UnreadRoomMessage {
                id: row.get("id"),
                created: row.get("created"),
                author: row.get("author"),
                body: row.get("body"),
            })
            .collect())
    }

    /// One keyset-paginated page of a room's history, always returned oldest
    /// first, as the viewer is allowed to see it.
    ///
    /// The read boundary is in the query, not the caller: members read any
    /// room they belong to, and anyone reads a public non-game room (mention
    /// previews can reference rooms the user never joined, the same rule as
    /// `get_for_viewer`). A caller holding a `room_id` it has no business
    /// reading gets an empty page rather than content, so a new entry point
    /// cannot leak a private room by forgetting a check.
    ///
    /// `cursor` is the `(created, id)` of the message to walk away from,
    /// exclusive. `None` starts at the room's newest message for `Older` and
    /// its oldest for `Newer`. Paging on the `(created, id)` pair rather than
    /// on `created` alone is what keeps messages sharing a timestamp from
    /// being skipped or repeated across page boundaries, and it matches
    /// `idx_chat_messages_room_created` exactly, so a page costs the same
    /// whether the room holds a hundred messages or a hundred thousand, and
    /// whether the cursor sits at the tail or a year back.
    ///
    /// System-feed authors and `exclude_user_ids` (the caller's ignored
    /// users, as authors and as bot-reply targets) are skipped, so a page
    /// reads as conversation rather than feed noise. Note this means a page
    /// can return fewer than `limit` rows without the room being exhausted;
    /// callers detect the end by an empty page, never by a short one.
    pub async fn list_page_for_viewer(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
        cursor: Option<(DateTime<Utc>, Uuid)>,
        direction: HistoryDirection,
        exclude_user_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Self>> {
        let (cursor_created, cursor_id) = match cursor {
            Some((created, id)) => (Some(created), Some(id)),
            None => (None, None),
        };

        // The two arms differ only in comparison and sort order, but both are
        // spelled out: a paging query is worth reading whole rather than
        // reassembling from string fragments.
        let sql = match direction {
            HistoryDirection::Older => {
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN users author ON author.id = msg.user_id
                 JOIN chat_rooms room ON room.id = msg.room_id
                 WHERE msg.room_id = $1
                   AND (
                     (room.visibility = 'public' AND room.kind <> 'game')
                     OR EXISTS (
                        SELECT 1 FROM chat_room_members mem
                        WHERE mem.room_id = $1 AND mem.user_id = $2
                     )
                   )
                   AND ($3::timestamptz IS NULL
                        OR (msg.created, msg.id) < ($3, $4::uuid))
                   AND msg.user_id <> ALL($5::uuid[])
                   AND (msg.reply_to_user_id IS NULL
                        OR msg.reply_to_user_id <> ALL($5::uuid[]))
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created DESC, msg.id DESC
                 LIMIT $6"
            }
            HistoryDirection::Newer => {
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN users author ON author.id = msg.user_id
                 JOIN chat_rooms room ON room.id = msg.room_id
                 WHERE msg.room_id = $1
                   AND (
                     (room.visibility = 'public' AND room.kind <> 'game')
                     OR EXISTS (
                        SELECT 1 FROM chat_room_members mem
                        WHERE mem.room_id = $1 AND mem.user_id = $2
                     )
                   )
                   AND ($3::timestamptz IS NULL
                        OR (msg.created, msg.id) > ($3, $4::uuid))
                   AND msg.user_id <> ALL($5::uuid[])
                   AND (msg.reply_to_user_id IS NULL
                        OR msg.reply_to_user_id <> ALL($5::uuid[]))
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created ASC, msg.id ASC
                 LIMIT $6"
            }
        };

        let rows = client
            .query(
                sql,
                &[
                    &room_id,
                    &user_id,
                    &cursor_created,
                    &cursor_id,
                    &exclude_user_ids,
                    &limit,
                ],
            )
            .await?;
        let mut page: Vec<Self> = rows.into_iter().map(Self::from).collect();
        // `Older` walked backwards to find the page; hand it back in reading
        // order so every caller sees pages the same way round.
        match direction {
            HistoryDirection::Older => page.reverse(),
            HistoryDirection::Newer => {}
        }
        Ok(page)
    }

    /// A public room's messages since `floor`, oldest first, for the
    /// `/summary` AI catch-up. Membership is required in the query (the
    /// command runs from a room the caller sits in), and the room must be
    /// public: private rooms and DMs are deliberately never handed to the
    /// summarizer. System-feed lines and `exclude_user_ids` (ignored
    /// authors, and bot replies aimed at them) are skipped so the summary
    /// describes the conversation the viewer actually sees; the viewer's
    /// own messages stay in, since the thread makes no sense without them.
    ///
    /// The query walks newest-first and the result is reversed, so a backlog
    /// larger than `limit` keeps the newest messages: for catching up, the
    /// old end is the right end to lose.
    pub async fn list_public_room_since(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
        floor: DateTime<Utc>,
        exclude_user_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN users author ON author.id = msg.user_id
                 JOIN chat_rooms room ON room.id = msg.room_id
                 WHERE msg.room_id = $1
                   AND room.visibility = 'public'
                   AND EXISTS (
                        SELECT 1 FROM chat_room_members mem
                        WHERE mem.room_id = $1 AND mem.user_id = $2
                   )
                   AND msg.created > $3
                   AND msg.user_id <> ALL($4::uuid[])
                   AND (msg.reply_to_user_id IS NULL
                        OR msg.reply_to_user_id <> ALL($4::uuid[]))
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created DESC, msg.id DESC
                 LIMIT $5",
                &[&room_id, &user_id, &floor, &exclude_user_ids, &limit],
            )
            .await?;
        let mut page: Vec<Self> = rows.into_iter().map(Self::from).collect();
        page.reverse();
        Ok(page)
    }

    /// The oldest message in a room created after `cutoff` and authored by
    /// someone other than the viewer: the message the `new messages` divider
    /// points at. Read scoping, ignored users, and system-feed exclusion
    /// match `list_page_for_viewer`, so the answer is a message the viewer
    /// would actually see in a history page. Used by the `/history`
    /// open-at-unread path, with the cutoff supplied by the session (its
    /// pre-mark unread marker), never from `last_read_at` directly, which
    /// has usually already advanced by the time the command runs.
    pub async fn first_unread_after(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
        cutoff: DateTime<Utc>,
        exclude_user_ids: &[Uuid],
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT msg.*
                 FROM chat_messages msg
                 JOIN users author ON author.id = msg.user_id
                 JOIN chat_rooms room ON room.id = msg.room_id
                 WHERE msg.room_id = $1
                   AND (
                     (room.visibility = 'public' AND room.kind <> 'game')
                     OR EXISTS (
                        SELECT 1 FROM chat_room_members mem
                        WHERE mem.room_id = $1 AND mem.user_id = $2
                     )
                   )
                   AND msg.created > $3
                   AND msg.user_id <> $2
                   AND msg.user_id <> ALL($4::uuid[])
                   AND (msg.reply_to_user_id IS NULL
                        OR msg.reply_to_user_id <> ALL($4::uuid[]))
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                 ORDER BY msg.created ASC, msg.id ASC
                 LIMIT 1",
                &[&room_id, &user_id, &cutoff, &exclude_user_ids],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Up to `limit_each` messages immediately before and after a message in
    /// its room, both in chronological order. Read scoping, ignored users and
    /// system-feed exclusion all come from `list_page_for_viewer`; this is the
    /// centered-window shape of it, used for the search-hit context pane.
    pub async fn list_around(
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
        created: DateTime<Utc>,
        id: Uuid,
        exclude_user_ids: &[Uuid],
        limit_each: i64,
    ) -> Result<(Vec<Self>, Vec<Self>)> {
        let cursor = Some((created, id));
        let before = Self::list_page_for_viewer(
            client,
            room_id,
            user_id,
            cursor,
            HistoryDirection::Older,
            exclude_user_ids,
            limit_each,
        )
        .await?;
        let after = Self::list_page_for_viewer(
            client,
            room_id,
            user_id,
            cursor,
            HistoryDirection::Newer,
            exclude_user_ids,
            limit_each,
        )
        .await?;
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
    /// carries the caller's ignored users, excluded both as authors and as
    /// bot-reply targets (an ignored user cannot be heard by proxy).
    /// `room_id` scopes to one room.
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
                   AND (msg.reply_to_user_id IS NULL
                        OR msg.reply_to_user_id <> ALL($4::uuid[]))
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

/// A message shown on the login announcements splash: the fields the splash
/// renders, with the author's username resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnreadRoomMessage {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub author: String,
    pub body: String,
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
