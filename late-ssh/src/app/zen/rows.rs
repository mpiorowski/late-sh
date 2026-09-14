//! The rows the Inbox and Headlines tiles list, derived from the chat state.
//! Pure: the renderer draws these rows and Enter on the Inbox reads the same
//! ones, so the row under the marker is the row that opens.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use late_core::models::{
    article::ArticleFeedItem, chat_message::ChatMessage, chat_room::ChatRoom,
    notification::NotificationView, rss_entry::RssEntryView,
};
use uuid::Uuid;

use crate::app::chat::state::dm_peer_is_ignored;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InboxRow {
    /// A DM with unread messages; `peer` is the other person's name.
    Dm {
        room_id: Uuid,
        peer: String,
        unread: i64,
    },
    /// A mention, read or not.
    Mention {
        room_id: Uuid,
        message_id: Uuid,
        actor: String,
        room: Option<String>,
        preview: String,
        at: DateTime<Utc>,
        unread: bool,
    },
}

/// Unread DMs first, the most unread on top, then the mentions, newest
/// first. A DM with an ignored peer never shows, as in the room rail.
pub fn inbox_rows(
    user_id: Uuid,
    rooms: &[(ChatRoom, Vec<ChatMessage>)],
    unread_counts: &HashMap<Uuid, i64>,
    usernames: &HashMap<Uuid, String>,
    ignored: &HashSet<Uuid>,
    mentions: &[NotificationView],
) -> Vec<InboxRow> {
    let mut dms: Vec<(i64, String, Uuid)> = Vec::new();
    for (room, _) in rooms {
        if room.kind != "dm" || dm_peer_is_ignored(room, user_id, ignored) {
            continue;
        }
        let unread = unread_counts.get(&room.id).copied().unwrap_or(0);
        if unread == 0 {
            continue;
        }
        let peer_id = if room.dm_user_a == Some(user_id) {
            room.dm_user_b
        } else {
            room.dm_user_a
        };
        let peer = match peer_id.and_then(|id| usernames.get(&id)) {
            Some(name) => name.clone(),
            None => "someone".to_string(),
        };
        dms.push((unread, peer, room.id));
    }
    dms.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    let mut mentions: Vec<&NotificationView> = mentions.iter().collect();
    mentions.sort_by(|left, right| right.created.cmp(&left.created));

    dms.into_iter()
        .map(|(unread, peer, room_id)| InboxRow::Dm {
            room_id,
            peer,
            unread,
        })
        .chain(mentions.into_iter().map(|mention| InboxRow::Mention {
            room_id: mention.room_id,
            message_id: mention.message_id,
            actor: mention.actor_username.clone(),
            room: mention.room_slug.clone(),
            preview: one_line(&mention.message_preview),
            at: mention.created,
            unread: mention.read_at.is_none(),
        }))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Headline {
    pub title: String,
    pub source: String,
    pub at: DateTime<Utc>,
}

/// News articles and the viewer's RSS entries, newest first. An entry that
/// was shared to News is the same link, so it lists once, as the article.
pub fn headlines(articles: &[ArticleFeedItem], entries: &[RssEntryView]) -> Vec<Headline> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut rows = Vec::with_capacity(articles.len() + entries.len());
    for item in articles {
        if seen.insert(item.article.url.as_str()) {
            rows.push(Headline {
                title: title_or_url(&item.article.title, &item.article.url),
                source: format!("news · {}", item.author_username),
                at: item.article.created,
            });
        }
    }
    for view in entries {
        if seen.insert(view.entry.url.as_str()) {
            rows.push(Headline {
                title: title_or_url(&view.entry.title, &view.entry.url),
                source: view.feed_title.clone(),
                at: view.entry.published_at.unwrap_or(view.entry.created),
            });
        }
    }
    rows.sort_by(|left, right| right.at.cmp(&left.at));
    rows
}

fn title_or_url(title: &str, url: &str) -> String {
    let title = one_line(title);
    if title.is_empty() {
        url.to_string()
    } else {
        title
    }
}

/// A list row is one line: newlines and runs of spaces fold to one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "rows_test.rs"]
mod rows_test;
