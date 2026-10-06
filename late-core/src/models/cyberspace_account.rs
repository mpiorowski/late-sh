use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "cyberspace_accounts";
    user_field = user_id;
    params = CyberspaceAccountParams;
    struct CyberspaceAccount {
        @data
        pub user_id: Uuid,
        pub cs_user_id: String,
        pub cs_username: String,
        pub refresh_token: String,
        // Feed read cursor: entries published after this are unread. NULL
        // until the first visit, which reads as nothing unread.
        pub feed_read_at: Option<DateTime<Utc>>,
        // cIRC room slugs this user pinned into the rail, in their order.
        // Their API has no join/leave, so this is our bookmark, not their state.
        pub circ_rooms: Vec<String>,
        // Per-room read cursors: slug -> newest message timestamp seen while
        // the user was in the room (their clock, epoch ms). Read through
        // `room_read_cursors`, written through `mark_circ_room_read`.
        pub circ_room_reads: Value,
        // C-Mail conversations pinned into the rail, in order: a JSON array of
        // `{id, username}`. Read through `cmail_threads()`, written through
        // `set_cmail_threads`. No read cursor beside it: unlike the cIRC
        // roster, their conversation list reports an unread count back.
        pub cmail_threads: Value,
    }
}

/// One pinned C-Mail conversation: their opaque id plus the other
/// participant's username, which is what the rail row is labelled with.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CmailThread {
    pub id: String,
    pub username: String,
}

impl CyberspaceAccount {
    /// Link (or re-link) the user's cyberspace account. One link per user:
    /// a second login replaces the stored refresh token and identity.
    pub async fn upsert_for_user(
        client: &Client,
        user_id: Uuid,
        cs_user_id: &str,
        cs_username: &str,
        refresh_token: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO cyberspace_accounts (user_id, cs_user_id, cs_username, refresh_token)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id)
                 DO UPDATE SET cs_user_id = $2,
                               cs_username = $3,
                               refresh_token = $4,
                               updated = current_timestamp
                 RETURNING *",
                &[&user_id, &cs_user_id, &cs_username, &refresh_token],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Move the feed read cursor forward. Called when the user enters the pane
    /// or asks for a refresh, the two moments they are demonstrably looking at
    /// the feed. A re-link keeps the cursor: it is the same person's reading.
    pub async fn mark_feed_read(client: &Client, user_id: Uuid, at: DateTime<Utc>) -> Result<()> {
        client
            .execute(
                "UPDATE cyberspace_accounts
                 SET feed_read_at = $2, updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &at],
            )
            .await?;
        Ok(())
    }

    /// Replace the pinned cIRC room list wholesale. The list is short and
    /// always written as a whole (add, remove, reorder are all "here is the
    /// new order"), so there is nothing to diff. Read cursors for rooms no
    /// longer pinned go with them: without the prune the map keeps a stamp
    /// for every room ever pinned.
    pub async fn set_circ_rooms(client: &Client, user_id: Uuid, rooms: &[String]) -> Result<()> {
        client
            .execute(
                "UPDATE cyberspace_accounts
                 SET circ_rooms = $2,
                     circ_room_reads = (
                         SELECT coalesce(jsonb_object_agg(entry.key, entry.value), '{}'::jsonb)
                         FROM jsonb_each(circ_room_reads) AS entry
                         WHERE entry.key = ANY($2)
                     ),
                     updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &rooms],
            )
            .await?;
        Ok(())
    }

    /// The per-room read cursors as a typed map, dropping anything that is
    /// not a number. Milliseconds on their clock, comparable to the roster's
    /// `last_message_at` without ever crossing clocks.
    pub fn room_read_cursors(&self) -> HashMap<String, i64> {
        match self.circ_room_reads.as_object() {
            Some(entries) => entries
                .iter()
                .filter_map(|(slug, value)| Some((slug.clone(), value.as_i64()?)))
                .collect(),
            None => HashMap::new(),
        }
    }

    /// Move one room's read cursor to the newest message timestamp the user
    /// saw while inside it. Written when a room's history lands and when the
    /// user leaves it, the two moments the session knows what was on screen.
    pub async fn mark_circ_room_read(
        client: &Client,
        user_id: Uuid,
        slug: &str,
        last_message_ts: i64,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE cyberspace_accounts
                 SET circ_room_reads =
                         jsonb_set(circ_room_reads, ARRAY[$2], to_jsonb($3::bigint), true),
                     updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &slug, &last_message_ts],
            )
            .await?;
        Ok(())
    }

    /// The pinned C-Mail conversations, in rail order. A malformed entry is
    /// dropped rather than failing the read: the rail is a bookmark list, and
    /// one bad row must not cost the user the rest of them.
    pub fn cmail_threads(&self) -> Vec<CmailThread> {
        match self.cmail_threads.as_array() {
            Some(entries) => entries
                .iter()
                .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                .collect(),
            None => Vec::new(),
        }
    }

    /// Replace the pinned C-Mail list wholesale, same contract as
    /// `set_circ_rooms`: the list is short and every change rewrites the whole
    /// order, so there is nothing to diff. No cursor prune to go with it,
    /// because their conversation list carries the unread count itself.
    pub async fn set_cmail_threads(
        client: &Client,
        user_id: Uuid,
        threads: &[CmailThread],
    ) -> Result<()> {
        let value = serde_json::to_value(threads)?;
        client
            .execute(
                "UPDATE cyberspace_accounts
                 SET cmail_threads = $2, updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &value],
            )
            .await?;
        Ok(())
    }

    /// Unlink: forget the account and its token. Returns true if a link existed.
    pub async fn delete_for_user(client: &Client, user_id: Uuid) -> Result<bool> {
        let n = client
            .execute(
                "DELETE FROM cyberspace_accounts WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(n > 0)
    }
}

#[cfg(test)]
#[path = "cyberspace_account_test.rs"]
mod cyberspace_account_test;
