use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use std::collections::HashMap;
use tokio_postgres::Client;
use uuid::Uuid;

use super::game_room::GameKind;

crate::model! {
    table = "chat_rooms";
    params = ChatRoomParams;
    struct ChatRoom {
        @data
        pub kind: String,
        pub visibility: String,
        pub auto_join: bool,
        pub permanent: bool,
        pub slug: Option<String>,
        pub language_code: Option<String>,
        pub dm_user_a: Option<Uuid>,
        pub dm_user_b: Option<Uuid>,
        // What this room is about, one line. Projected as the IRC topic.
        pub topic: Option<String>,
        // The room's general rules, shown on request.
        pub rules: Option<String>,
        // Who opened this room. Written by the create paths only, never
        // back-filled; see `owner_id` for the ownership actually in force.
        pub created_by: Option<Uuid>,
    }
}

/// Trim a room-info field and treat an empty result as "unset" (NULL).
fn clean_info(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// One user's rooms and the per-room counters that come back with them.
/// Both maps are keyed by room id and always carry an entry for every room
/// in `rooms`, so callers never have to distinguish "absent" from "zero".
pub struct UserRoomState {
    pub rooms: Vec<ChatRoom>,
    pub last_message_at: HashMap<Uuid, Option<DateTime<Utc>>>,
    pub unread_counts: HashMap<Uuid, i64>,
}

impl ChatRoom {
    pub async fn ensure_lounge(client: &Client) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug)
                 VALUES ('lounge', 'public', true, true, 'lounge')
                 ON CONFLICT (slug) WHERE kind = 'lounge'
                 DO UPDATE
                    SET visibility = 'public',
                        auto_join = true,
                        permanent = true,
                        updated = current_timestamp
                 RETURNING *",
                &[],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn find_lounge(client: &Client) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM chat_rooms WHERE kind = 'lounge' AND slug = 'lounge'",
                &[],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_non_dm_by_slug(client: &Client, slug: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM chat_rooms WHERE slug = $1 AND kind <> 'dm' LIMIT 1",
                &[&slug],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// The public, non-DM room for a slug, preferring a permanent room and
    /// then the oldest match. Used by the login announcements splash, which
    /// must resolve `#announcements` to the same room `auto_join_public_rooms`
    /// joins users to.
    pub async fn find_public_non_dm_by_slug(client: &Client, slug: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM chat_rooms
                 WHERE slug = $1
                   AND kind <> 'dm'
                   AND visibility = 'public'
                 ORDER BY permanent DESC, created ASC, id ASC
                 LIMIT 1",
                &[&slug],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_irc_channel_by_slug_for_user(
        client: &Client,
        slug: &str,
        user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT r.*
                 FROM chat_rooms r
                 WHERE r.slug = $1
                   AND (r.kind IN ('lounge', 'language')
                        OR (r.kind = 'topic' AND r.visibility = 'public')
                        OR (r.kind = 'topic' AND r.visibility = 'private' AND EXISTS (
                              SELECT 1 FROM chat_room_members m
                              WHERE m.room_id = r.id AND m.user_id = $2)))
                 ORDER BY CASE
                    WHEN r.kind = 'lounge' THEN 0
                    WHEN r.kind = 'language' THEN 1
                    WHEN r.kind = 'topic' AND r.visibility = 'public' THEN 2
                    WHEN r.kind = 'topic' AND r.visibility = 'private' THEN 3
                    ELSE 4
                 END
                 LIMIT 1",
                &[&slug, &user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn get_or_create_language(client: &Client, language_code: &str) -> Result<Self> {
        let language_code = language_code.trim().to_lowercase();
        if language_code.is_empty() {
            bail!("language code cannot be empty");
        }
        let slug = format!("lang-{language_code}");

        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, slug, language_code)
                 VALUES ('language', 'public', false, $1, $2)
                 ON CONFLICT (language_code) WHERE kind = 'language'
                 DO UPDATE
                    SET visibility = 'public',
                        auto_join = false,
                        slug = EXCLUDED.slug,
                        updated = current_timestamp
                 RETURNING *",
                &[&slug, &language_code],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn find_topic_room(
        client: &Client,
        visibility: &str,
        slug: &str,
    ) -> Result<Option<Self>> {
        let slug = normalize_topic_slug(slug)?;
        let row = client
            .query_opt(
                "SELECT *
                 FROM chat_rooms
                 WHERE kind = 'topic' AND visibility = $1 AND slug = $2",
                &[&visibility, &slug],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn get_or_create_public_room(client: &Client, slug: &str) -> Result<Self> {
        let slug = normalize_topic_slug(slug)?;
        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, slug)
                 VALUES ('topic', 'public', false, $1)
                 ON CONFLICT (visibility, slug) WHERE kind = 'topic'
                 DO UPDATE SET updated = current_timestamp
                 RETURNING *",
                &[&slug],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn get_or_create_game_room(
        client: &Client,
        game_kind: GameKind,
        slug: &str,
    ) -> Result<Self> {
        let game_kind = game_kind.as_str();
        let slug = normalize_game_slug(slug)?;
        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, slug, game_kind)
                 VALUES ('game', 'public', false, $1, $2)
                 ON CONFLICT (game_kind, slug) WHERE kind = 'game'
                 DO UPDATE SET updated = current_timestamp
                 RETURNING *",
                &[&slug, &game_kind],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Private two-player chat room for a claimed daily match, plus both
    /// memberships, in one statement. `kind = 'game'` (hidden from the Home
    /// rail, no Mentions, no IRC) but `visibility = 'private'`: only the two
    /// players are ever members, and the public game-room join path rejects
    /// private rooms. `game_kind` is the daily roster kind string; the slug
    /// is `daily-{match_id}`, unique per match. No ON CONFLICT: a duplicate
    /// slug means a bug, not a race to absorb.
    pub async fn create_daily_match_room(
        client: &impl GenericClient,
        game_kind: &str,
        slug: &str,
        player_a: Uuid,
        player_b: Uuid,
    ) -> Result<Self> {
        let slug = normalize_game_slug(slug)?;
        let row = client
            .query_one(
                "WITH room AS (
                     INSERT INTO chat_rooms (kind, visibility, auto_join, slug, game_kind)
                     VALUES ('game', 'private', false, $1, $2)
                     RETURNING *
                 ),
                 members AS (
                     INSERT INTO chat_room_members (room_id, user_id)
                     SELECT room.id, member_id
                     FROM room, unnest(ARRAY[$3, $4]::uuid[]) AS member_id
                 )
                 SELECT * FROM room",
                &[&slug, &game_kind, &player_a, &player_b],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Create a private room owned by `created_by`. The creator is recorded in
    /// the same statement, so a private room is never momentarily ownerless.
    pub async fn create_private_room(
        client: &Client,
        slug: &str,
        created_by: Uuid,
    ) -> Result<Self> {
        let slug = normalize_topic_slug(slug)?;

        let row = client
            .query_opt(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, slug, created_by)
                 VALUES ('topic', 'private', false, $1, $2)
                 ON CONFLICT (visibility, slug) WHERE kind = 'topic'
                 DO NOTHING
                 RETURNING *",
                &[&slug, &created_by],
            )
            .await?;

        match row {
            Some(row) => Ok(Self::from(row)),
            None => bail!("private room #{slug} already exists"),
        }
    }

    /// Record who opened a room, from the create path only. The `created_by IS
    /// NULL` guard is a concurrency guard, not a back-fill: two sessions racing
    /// `/public #foo` both land here, and the loser must not overwrite the
    /// winner. Never call this to grant ownership of an existing room.
    pub async fn set_creator(client: &Client, room_id: Uuid, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE chat_rooms SET created_by = $2
                 WHERE id = $1 AND created_by IS NULL",
                &[&room_id, &user_id],
            )
            .await?;
        Ok(())
    }

    /// Owners of `room_ids`, derived rather than stored: the recorded creator
    /// while they are still a member, otherwise the earliest remaining member
    /// (ties broken by user id so the answer is stable). Ownership therefore
    /// succeeds on its own when a creator leaves, and rooms that predate
    /// `created_by` resolve to their first member. A room with no members left
    /// is absent from the map.
    pub async fn owner_ids_for_rooms(
        client: &Client,
        room_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Uuid>> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = client
            .query(
                "SELECT DISTINCT ON (m.room_id) m.room_id, m.user_id
                   FROM chat_room_members m
                   JOIN chat_rooms r ON r.id = m.room_id
                  WHERE m.room_id = ANY($1)
                  ORDER BY m.room_id,
                           COALESCE(m.user_id = r.created_by, false) DESC,
                           m.joined_at ASC,
                           m.user_id ASC",
                &[&room_ids],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("room_id"), row.get("user_id")))
            .collect())
    }

    /// The owner of one room. Same derivation as `owner_ids_for_rooms`.
    pub async fn owner_id(client: &Client, room_id: Uuid) -> Result<Option<Uuid>> {
        Ok(Self::owner_ids_for_rooms(client, &[room_id])
            .await?
            .remove(&room_id))
    }

    /// Set what the room is about and its rules. Empty strings are stored as
    /// NULL so a cleared field reads as "unset" rather than blank. Authority is
    /// resolved by the caller (service layer).
    pub async fn set_topic_and_rules(
        client: &Client,
        room_id: Uuid,
        topic: Option<&str>,
        rules: Option<&str>,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE chat_rooms
                    SET topic = $2, rules = $3, updated = current_timestamp
                  WHERE id = $1
                 RETURNING *",
                &[&room_id, &clean_info(topic), &clean_info(rules)],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn get_or_create_room(client: &Client, slug: &str) -> Result<Self> {
        Self::get_or_create_public_room(client, slug).await
    }

    pub async fn get_or_create_dm(client: &Client, user_a: Uuid, user_b: Uuid) -> Result<Self> {
        if user_a == user_b {
            bail!("cannot create DM room with the same user");
        }

        let (dm_user_a, dm_user_b) = canonical_dm_pair(user_a, user_b);

        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, dm_user_a, dm_user_b)
                 VALUES ('dm', 'dm', false, $1, $2)
                 ON CONFLICT (dm_user_a, dm_user_b) WHERE kind = 'dm'
                 DO UPDATE SET visibility = 'dm', auto_join = false, updated = current_timestamp
                 RETURNING *",
                &[&dm_user_a, &dm_user_b],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Existing DM room between two users, if one has been created.
    pub async fn get_dm(client: &Client, user_a: Uuid, user_b: Uuid) -> Result<Option<Self>> {
        let (dm_user_a, dm_user_b) = canonical_dm_pair(user_a, user_b);
        let row = client
            .query_opt(
                "SELECT * FROM chat_rooms
                 WHERE kind = 'dm' AND dm_user_a = $1 AND dm_user_b = $2",
                &[&dm_user_a, &dm_user_b],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT r.*
                 FROM chat_rooms r
                 JOIN chat_room_members m ON m.room_id = r.id
                 WHERE m.user_id = $1
                 ORDER BY
                     CASE
                         WHEN r.kind = 'lounge' AND r.slug = 'lounge' THEN 0
                         WHEN r.permanent THEN 1
                         WHEN r.visibility = 'public' THEN 2
                         WHEN r.kind = 'dm' THEN 4
                         ELSE 3
                     END ASC,
                     COALESCE(r.slug, COALESCE(r.language_code, '')) ASC,
                     r.created ASC,
                     r.id ASC",
                &[&user_id],
            )
            .await?;

        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// The rooms a user is in, together with the two per-room counters the
    /// chat rail renders from.
    ///
    /// One query rather than three. `last_message_at` and the unread count
    /// both key off the very `chat_room_members` row this already walks, so
    /// folding them in as laterals removes two redundant scans of the user's
    /// membership set on every snapshot pass. The snapshot poll runs per
    /// session every `CHAT_REFRESH_INTERVAL`, so those scans were the same
    /// work repeated millions of times a day.
    ///
    /// `system_user_id` is the #lounge feed bot (`app/activity/lounge.rs`),
    /// whose `· `-prefixed activity lines must never light an unread badge.
    /// It is passed in rather than re-derived per row: the old shape joined
    /// `users` and read `settings->>'system'` out of JSONB for every message,
    /// which made Postgres hash the entire users table to answer a question
    /// with one constant answer. `None` means no system user exists yet, in
    /// which case nothing is excluded.
    pub async fn list_for_user_with_state(
        client: &Client,
        user_id: Uuid,
        system_user_id: Option<Uuid>,
    ) -> Result<UserRoomState> {
        let rows = client
            .query(
                // The system-feed bot posts ambient activity lines prefixed
                // with `· `; those must never light anyone's unread badge. The
                // same author also posts real messages (a new public room
                // reported to #moderators), which must. The prefix is the
                // discriminator everywhere else too, see
                // `chat/state.rs::system_line_text_in`.
                //
                // IS NOT DISTINCT FROM keeps a NULL $2 (no system user) from
                // nulling out the whole predicate and dropping every row.
                //
                // The unread count stops at $3; see `UNREAD_COUNT_CAP` for why
                // an exact total is neither needed nor affordable.
                "SELECT r.*,
                        latest.created AS last_message_at,
                        COALESCE(unread.unread_count, 0)::bigint AS unread_count
                 FROM chat_rooms r
                 JOIN chat_room_members m ON m.room_id = r.id
                 LEFT JOIN LATERAL (
                    SELECT created
                    FROM chat_messages
                    WHERE room_id = r.id
                    ORDER BY created DESC, id DESC
                    LIMIT 1
                 ) latest ON true
                 LEFT JOIN LATERAL (
                    SELECT COUNT(*)::bigint AS unread_count
                    FROM (
                       SELECT 1
                       FROM chat_messages msg
                       WHERE msg.room_id = r.id
                         AND msg.user_id <> m.user_id
                         AND NOT (
                             msg.user_id IS NOT DISTINCT FROM $2::uuid
                             AND msg.body LIKE '· %'
                         )
                         AND msg.created > COALESCE(m.last_read_at, '-infinity'::timestamptz)
                       LIMIT $3
                    ) capped
                 ) unread ON true
                 WHERE m.user_id = $1
                 ORDER BY
                     CASE
                         WHEN r.kind = 'lounge' AND r.slug = 'lounge' THEN 0
                         WHEN r.permanent THEN 1
                         WHEN r.visibility = 'public' THEN 2
                         WHEN r.kind = 'dm' THEN 4
                         ELSE 3
                     END ASC,
                     COALESCE(r.slug, COALESCE(r.language_code, '')) ASC,
                     r.created ASC,
                     r.id ASC",
                &[
                    &user_id,
                    &system_user_id,
                    &crate::models::chat_room_member::ChatRoomMember::UNREAD_COUNT_CAP,
                ],
            )
            .await?;

        let mut state = UserRoomState {
            rooms: Vec::with_capacity(rows.len()),
            last_message_at: HashMap::with_capacity(rows.len()),
            unread_counts: HashMap::with_capacity(rows.len()),
        };
        for row in rows {
            // Read the joined columns before `from` consumes the row.
            let last_message_at: Option<DateTime<Utc>> = row.get("last_message_at");
            let unread_count: i64 = row.get("unread_count");
            let room = Self::from(row);
            state.last_message_at.insert(room.id, last_message_at);
            state.unread_counts.insert(room.id, unread_count);
            state.rooms.push(room);
        }
        Ok(state)
    }

    pub async fn get_target_user_ids(client: &Client, room_id: Uuid) -> Result<Option<Vec<Uuid>>> {
        let visibility: String = client
            .query_one(
                "SELECT visibility FROM chat_rooms WHERE id = $1",
                &[&room_id],
            )
            .await?
            .get(0);

        if visibility == "dm" || visibility == "private" {
            Ok(Some(
                crate::models::chat_room_member::ChatRoomMember::list_user_ids(client, room_id)
                    .await?,
            ))
        } else {
            Ok(None)
        }
    }

    pub async fn list_by_ids(client: &Client, room_ids: &[Uuid]) -> Result<Vec<Self>> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = client
            .query("SELECT * FROM chat_rooms WHERE id = ANY($1)", &[&room_ids])
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn is_kind(client: &Client, room_id: Uuid, kind: &str) -> Result<bool> {
        let row = client
            .query_opt("SELECT kind FROM chat_rooms WHERE id = $1", &[&room_id])
            .await?;
        Ok(row
            .map(|row| row.get::<_, String>(0) == kind)
            .unwrap_or(false))
    }

    /// Rooms visible to the user as IRC channels: the lounge, language rooms,
    /// public topic rooms, plus private topic rooms the user is a member of.
    /// See devdocs/FRD-IRCD.md §6.
    pub async fn list_irc_channels(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "WITH visible AS (
                    SELECT r.*,
                           ROW_NUMBER() OVER (
                               PARTITION BY r.slug
                               ORDER BY CASE
                                   WHEN r.kind = 'lounge' THEN 0
                                   WHEN r.kind = 'language' THEN 1
                                   WHEN r.kind = 'topic' AND r.visibility = 'public' THEN 2
                                   WHEN r.kind = 'topic' AND r.visibility = 'private' THEN 3
                                   ELSE 4
                               END
                           ) AS irc_rank
                    FROM chat_rooms r
                    WHERE r.slug IS NOT NULL
                      AND (r.kind IN ('lounge', 'language')
                           OR (r.kind = 'topic' AND r.visibility = 'public')
                           OR (r.kind = 'topic' AND r.visibility = 'private' AND EXISTS (
                                 SELECT 1 FROM chat_room_members m
                                 WHERE m.room_id = r.id AND m.user_id = $1)))
                 )
                 SELECT *
                 FROM visible
                 WHERE irc_rank = 1
                 ORDER BY (kind = 'lounge') DESC, slug",
                &[&user_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_irc_channel_summaries(
        client: &Client,
        user_id: Uuid,
    ) -> Result<Vec<(Self, i64)>> {
        let rows = client
            .query(
                "WITH visible AS (
                    SELECT r.*,
                           ROW_NUMBER() OVER (
                               PARTITION BY r.slug
                               ORDER BY CASE
                                   WHEN r.kind = 'lounge' THEN 0
                                   WHEN r.kind = 'language' THEN 1
                                   WHEN r.kind = 'topic' AND r.visibility = 'public' THEN 2
                                   WHEN r.kind = 'topic' AND r.visibility = 'private' THEN 3
                                   ELSE 4
                               END
                           ) AS irc_rank
                    FROM chat_rooms r
                    WHERE r.slug IS NOT NULL
                      AND (r.kind IN ('lounge', 'language')
                           OR (r.kind = 'topic' AND r.visibility = 'public')
                           OR (r.kind = 'topic' AND r.visibility = 'private' AND EXISTS (
                                 SELECT 1 FROM chat_room_members m
                                 WHERE m.room_id = r.id AND m.user_id = $1)))
                 )
                 SELECT r.*, COALESCE(m.member_count, 0)::bigint AS member_count
                 FROM visible r
                 LEFT JOIN LATERAL (
                    SELECT COUNT(*)::bigint AS member_count
                    FROM chat_room_members m
                    WHERE m.room_id = r.id
                 ) m ON true
                 WHERE r.irc_rank = 1
                 ORDER BY (r.kind = 'lounge') DESC, r.slug",
                &[&user_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let member_count = row.get("member_count");
                (Self::from(row), member_count)
            })
            .collect())
    }

    pub async fn list_discover_public_topic_rooms(
        client: &Client,
    ) -> Result<Vec<DiscoverPublicTopicRoom>> {
        let rows = client
            .query(
                "SELECT r.id,
                        r.slug,
                        r.topic,
                        COALESCE(m.member_count, 0)::bigint AS member_count,
                        COALESCE(msg.message_count, 0)::bigint AS message_count,
                        msg.last_message_at
                 FROM chat_rooms r
                 LEFT JOIN LATERAL (
                    SELECT COUNT(*)::bigint AS member_count
                    FROM chat_room_members m
                    WHERE m.room_id = r.id
                 ) m ON true
                 LEFT JOIN LATERAL (
                    SELECT COUNT(*)::bigint AS message_count,
                           MAX(created) AS last_message_at
                    FROM chat_messages msg
                    WHERE msg.room_id = r.id
                 ) msg ON true
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                 ORDER BY
                    COALESCE(msg.last_message_at, r.created) DESC,
                    message_count DESC,
                    member_count DESC,
                    r.slug ASC",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let slug: Option<String> = row.get("slug");
                slug.map(|slug| DiscoverPublicTopicRoom {
                    room_id: row.get("id"),
                    slug,
                    topic: row.get("topic"),
                    member_count: row.get("member_count"),
                    message_count: row.get("message_count"),
                    last_message_at: row.get("last_message_at"),
                })
            })
            .collect())
    }

    pub async fn list_public_topic_room_summaries(
        client: &Client,
    ) -> Result<Vec<PublicTopicRoomSummary>> {
        let rows = client
            .query(
                "SELECT r.kind,
                        r.slug,
                        r.language_code,
                        COUNT(m.user_id)::bigint AS member_count
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                 GROUP BY r.id, r.kind, r.slug, r.language_code, r.created
                 ORDER BY
                    member_count DESC,
                    COALESCE(r.slug, COALESCE(r.language_code, '')) ASC,
                    r.created ASC,
                    r.id ASC",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| PublicTopicRoomSummary {
                kind: row.get("kind"),
                slug: row.get("slug"),
                language_code: row.get("language_code"),
                member_count: row.get("member_count"),
            })
            .collect())
    }

    pub async fn touch_updated(client: &impl GenericClient, room_id: Uuid) -> Result<u64> {
        let rows = client
            .execute(
                "UPDATE chat_rooms SET updated = current_timestamp WHERE id = $1",
                &[&room_id],
            )
            .await?;
        Ok(rows)
    }

    /// Create or update a public auto-join room. Auto-join rooms are joined by
    /// all users on connect but can be left.
    pub async fn ensure_auto_join(client: &Client, slug: &str) -> Result<Self> {
        let slug = normalize_topic_slug(slug)?;

        let existing = client
            .query_opt(
                "SELECT id
                 FROM chat_rooms
                 WHERE slug = $1 AND kind = 'topic' AND visibility = 'public'",
                &[&slug],
            )
            .await?;
        if existing.is_some() {
            bail!("room #{slug} already exists");
        }

        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug)
                 VALUES ('topic', 'public', true, false, $1)
                 RETURNING *",
                &[&slug],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Create a permanent public room. Permanent rooms are auto-joined by all
    /// users on connect and cannot be left. Re-running this on a room that is
    /// already permanent is a no-op, so the caller can retry safely; an
    /// existing *non*-permanent room is left alone and the call fails, because
    /// promoting it would bulk-add every user to an unleaveable room with no
    /// undo — a mistyped slug must not do that.
    pub async fn ensure_permanent(client: &Client, slug: &str) -> Result<Self> {
        let slug = normalize_topic_slug(slug)?;

        let existing = client
            .query_opt(
                "SELECT *
                 FROM chat_rooms
                 WHERE slug = $1 AND kind = 'topic' AND visibility = 'public'",
                &[&slug],
            )
            .await?;
        if let Some(existing) = existing {
            let room = Self::from(existing);
            if room.permanent {
                return Ok(room);
            }
            // Promote an existing non-permanent public room (e.g. a user-created
            // `/public` room) to a permanent auto-join room. Callers bulk-add all
            // users afterwards, so a mistyped slug will bulk-add everyone to an
            // unleavable room — `/create-room` is admin-only for that reason.
            let row = client
                .query_one(
                    "UPDATE chat_rooms
                     SET permanent = true, auto_join = true, updated = now()
                     WHERE id = $1
                     RETURNING *",
                    &[&room.id],
                )
                .await?;
            return Ok(Self::from(row));
        }

        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug)
                 VALUES ('topic', 'public', true, true, $1)
                 RETURNING *",
                &[&slug],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Delete a permanent room by slug. Refuses to delete #lounge.
    pub async fn delete_permanent(client: &Client, slug: &str) -> Result<u64> {
        let slug = normalize_room_slug(slug)?;
        if slug == "lounge" {
            bail!("cannot delete #lounge");
        }
        let row = client
            .query_one(
                "WITH target AS (
                     SELECT id
                     FROM chat_rooms
                     WHERE slug = $1 AND permanent = true
                 ),
                 deleted_voice AS (
                     DELETE FROM voice_channels v
                     USING target t
                     WHERE v.target_kind = 'chat_room'
                       AND v.target_id = t.id
                     RETURNING v.id
                 ),
                 deleted AS (
                     DELETE FROM chat_rooms c
                     USING target t
                     WHERE c.id = t.id
                     RETURNING c.id
                 )
                 SELECT COUNT(*)::bigint AS count FROM deleted",
                &[&slug],
            )
            .await?;
        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    /// Bulk-add all existing users to a room (idempotent).
    pub async fn add_all_users(client: &Client, room_id: Uuid) -> Result<u64> {
        let count = client
            .execute(
                "INSERT INTO chat_room_members (room_id, user_id)
                 SELECT $1, id
                 FROM users
                 WHERE NOT EXISTS (
                     SELECT 1
                     FROM room_bans
                     WHERE room_bans.room_id = $1
                       AND room_bans.target_user_id = users.id
                       AND (room_bans.expires_at IS NULL OR room_bans.expires_at > current_timestamp)
                 )
                 ON CONFLICT (room_id, user_id) DO NOTHING",
                &[&room_id],
            )
            .await?;
        Ok(count)
    }

    /// Update the auto-join flag for a room.
    pub async fn set_auto_join(client: &Client, room_id: Uuid, auto_join: bool) -> Result<u64> {
        let count = client
            .execute(
                "UPDATE chat_rooms
                 SET auto_join = $2, updated = current_timestamp
                 WHERE id = $1",
                &[&room_id, &auto_join],
            )
            .await?;
        Ok(count)
    }

    pub async fn rename_non_dm_slug(
        client: &impl GenericClient,
        room_id: Uuid,
        new_slug: &str,
    ) -> Result<u64> {
        let conflict = client
            .query_opt(
                "SELECT id FROM chat_rooms
                 WHERE slug = $1 AND kind <> 'dm' AND id <> $2
                 LIMIT 1",
                &[&new_slug, &room_id],
            )
            .await?;
        if conflict.is_some() {
            bail!("room #{new_slug} already exists");
        }

        let updated = client
            .execute(
                "UPDATE chat_rooms
                 SET slug = $2, updated = current_timestamp
                 WHERE id = $1 AND kind <> 'dm'",
                &[&room_id, &new_slug],
            )
            .await?;
        Ok(updated)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverPublicTopicRoom {
    pub room_id: Uuid,
    pub slug: String,
    /// What the room is about, when a mod has set it. This is the one line
    /// someone reads before deciding whether to join.
    pub topic: Option<String>,
    pub member_count: i64,
    pub message_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicTopicRoomSummary {
    pub kind: String,
    pub slug: Option<String>,
    pub language_code: Option<String>,
    pub member_count: i64,
}

pub fn canonical_dm_pair(user_a: Uuid, user_b: Uuid) -> (Uuid, Uuid) {
    if user_a.as_u128() < user_b.as_u128() {
        (user_a, user_b)
    } else {
        (user_b, user_a)
    }
}

fn normalize_topic_slug(slug: &str) -> Result<String> {
    let slug = normalize_room_slug(slug)?;
    if slug == "lounge" {
        bail!("cannot create room with reserved name 'lounge'");
    }
    Ok(slug)
}

fn normalize_room_slug(slug: &str) -> Result<String> {
    let trimmed = slug.trim().to_lowercase();
    let mut normalized = String::with_capacity(trimmed.len());
    let mut last_was_dash = false;

    for ch in trimmed.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            normalized.push(ch);
            last_was_dash = false;
        } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '.' | '/' | '\\') {
            if !normalized.is_empty() && !last_was_dash {
                normalized.push('-');
                last_was_dash = true;
            }
        } else if !normalized.is_empty() && !last_was_dash {
            normalized.push('-');
            last_was_dash = true;
        }
    }

    let slug = normalized.trim_matches('-').to_string();
    if slug.is_empty() {
        bail!("room name cannot be empty");
    }
    Ok(slug)
}

fn normalize_game_slug(slug: &str) -> Result<String> {
    let slug = normalize_room_slug(slug)?;
    if slug == "lounge" {
        bail!("cannot create game room with reserved name 'lounge'");
    }
    Ok(slug)
}

#[cfg(test)]
#[path = "chat_room_internal_test.rs"]
mod chat_room_internal_test;
