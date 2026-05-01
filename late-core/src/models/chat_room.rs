use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
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
    }
}

impl ChatRoom {
    pub async fn ensure_general(client: &Client) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug)
                 VALUES ('general', 'public', true, true, 'general')
                 ON CONFLICT (slug) WHERE kind = 'general'
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

    pub async fn find_general(client: &Client) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM chat_rooms WHERE kind = 'general' AND slug = 'general'",
                &[],
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

    pub async fn create_private_room(client: &Client, slug: &str) -> Result<Self> {
        let slug = normalize_topic_slug(slug)?;

        let row = client
            .query_opt(
                "INSERT INTO chat_rooms (kind, visibility, auto_join, slug)
                 VALUES ('topic', 'private', false, $1)
                 ON CONFLICT (visibility, slug) WHERE kind = 'topic'
                 DO NOTHING
                 RETURNING *",
                &[&slug],
            )
            .await?;

        match row {
            Some(row) => Ok(Self::from(row)),
            None => bail!("private room #{slug} already exists"),
        }
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

    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT r.*
                 FROM chat_rooms r
                 JOIN chat_room_members m ON m.room_id = r.id
                 WHERE m.user_id = $1
                 ORDER BY
                     CASE
                         WHEN r.kind = 'general' AND r.slug = 'general' THEN 0
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

    pub async fn is_kind(client: &Client, room_id: Uuid, kind: &str) -> Result<bool> {
        let row = client
            .query_opt("SELECT kind FROM chat_rooms WHERE id = $1", &[&room_id])
            .await?;
        Ok(row
            .map(|row| row.get::<_, String>(0) == kind)
            .unwrap_or(false))
    }

    pub async fn list_discover_public_topic_rooms(
        client: &Client,
    ) -> Result<Vec<DiscoverPublicTopicRoom>> {
        let rows = client
            .query(
                "SELECT r.id,
                        r.slug,
                        COUNT(DISTINCT m.user_id)::bigint AS member_count,
                        COUNT(DISTINCT msg.id)::bigint AS message_count,
                        MAX(msg.created) AS last_message_at
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 LEFT JOIN chat_messages msg ON msg.room_id = r.id
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                 GROUP BY r.id, r.slug
                 ORDER BY
                    COALESCE(MAX(msg.created), r.created) DESC,
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

    pub async fn touch_updated(client: &Client, room_id: Uuid) -> Result<u64> {
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

    /// Create or update a permanent public room. Permanent rooms are auto-joined
    /// by all users on connect and cannot be left.
    pub async fn ensure_permanent(client: &Client, slug: &str) -> Result<Self> {
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
                 VALUES ('topic', 'public', true, true, $1)
                 RETURNING *",
                &[&slug],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Delete a permanent room by slug. Refuses to delete #general.
    pub async fn delete_permanent(client: &Client, slug: &str) -> Result<u64> {
        let slug = normalize_room_slug(slug)?;
        if slug == "general" {
            bail!("cannot delete #general");
        }
        let count = client
            .execute(
                "DELETE FROM chat_rooms WHERE slug = $1 AND permanent = true",
                &[&slug],
            )
            .await?;
        Ok(count)
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverPublicTopicRoom {
    pub room_id: Uuid,
    pub slug: String,
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
    if slug == "general" {
        bail!("cannot create room with reserved name 'general'");
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
    if slug == "general" {
        bail!("cannot create game room with reserved name 'general'");
    }
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_dm_pair_orders_smaller_first() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        assert_eq!(canonical_dm_pair(a, b), (a, b));
        assert_eq!(canonical_dm_pair(b, a), (a, b));
    }

    #[test]
    fn canonical_dm_pair_equal_uuids() {
        let a = Uuid::from_u128(42);
        let (x, y) = canonical_dm_pair(a, a);
        assert_eq!(x, a);
        assert_eq!(y, a);
    }

    #[test]
    fn normalize_topic_slug_slugifies_room_names() {
        assert_eq!(
            normalize_topic_slug("  Rust Nerds  ").unwrap(),
            "rust-nerds"
        );
        assert_eq!(normalize_topic_slug("room\nname").unwrap(), "room-name");
        assert_eq!(normalize_topic_slug("vps/d9d0").unwrap(), "vps-d9d0");
        assert_eq!(normalize_topic_slug("a___b...c").unwrap(), "a-b-c");
    }

    #[test]
    fn normalize_topic_slug_rejects_empty_or_reserved_names() {
        assert!(normalize_topic_slug("   ").is_err());
        assert!(normalize_topic_slug("!!!").is_err());
        assert!(normalize_topic_slug("general").is_err());
    }

    #[test]
    fn normalize_room_slug_allows_general_for_non_creation_paths() {
        assert_eq!(normalize_room_slug(" General ").unwrap(), "general");
    }
}
