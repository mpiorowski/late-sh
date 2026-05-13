use chrono::{DateTime, Utc};
use late_core::models::chat_room::ChatRoom;
use uuid::Uuid;

use crate::app::chat::state::{ChatState, RoomSlot, is_chat_list_room};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoomSearchItem {
    pub slot: RoomSlot,
    pub label: String,
    pub meta: String,
    pub unread_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
    pub favorite: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RoomSearchModalState {
    open: bool,
    query: String,
    selected: usize,
}

impl RoomSearchModalState {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn push(&mut self, ch: char) {
        if !ch.is_control() {
            self.query.push(ch);
            self.selected = 0;
        }
    }

    pub(crate) fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub(crate) fn delete_word_left(&mut self) {
        let trimmed = self.query.trim_end().len();
        self.query.truncate(trimmed);
        while self
            .query
            .chars()
            .last()
            .is_some_and(|ch| !ch.is_whitespace() && ch != '/' && ch != '#' && ch != '@')
        {
            self.query.pop();
        }
        self.selected = 0;
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        self.selected = next;
    }

    pub(crate) fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(len - 1);
        }
    }
}

pub(crate) fn search_items(chat: &ChatState, current_user_id: Uuid) -> Vec<RoomSearchItem> {
    let mut items = Vec::new();
    for slot in chat.visual_order() {
        match slot {
            RoomSlot::Room(room_id) => {
                let Some((room, messages)) = chat.rooms.iter().find(|(room, _)| room.id == room_id)
                else {
                    continue;
                };
                if !is_chat_list_room(room) {
                    continue;
                }
                items.push(RoomSearchItem {
                    slot,
                    label: room_label(room, current_user_id, &chat.usernames),
                    meta: room_meta(room),
                    unread_count: chat.unread_counts.get(&room.id).copied().unwrap_or(0),
                    last_message_at: messages
                        .first()
                        .map(|message| message.created)
                        .or(Some(room.updated)),
                    favorite: chat.favorite_room_ids().contains(&room.id),
                });
            }
            RoomSlot::Feeds
            | RoomSlot::News
            | RoomSlot::Notifications
            | RoomSlot::Discover
            | RoomSlot::Showcase
            | RoomSlot::Work => {
                items.push(synthetic_item(slot, chat));
            }
        }
    }
    items
}

pub(crate) fn filtered_items(
    chat: &ChatState,
    current_user_id: Uuid,
    query: &str,
) -> Vec<RoomSearchItem> {
    let query = SearchQuery::parse(query);
    let mut all = search_items(chat, current_user_id);
    if query.kind == SearchQueryKind::All && query.text.is_empty() {
        return all;
    }

    let mut items: Vec<_> = all
        .drain(..)
        .filter(|item| item_matches_query(item, &query))
        .collect();

    if matches!(query.kind, SearchQueryKind::Dms) {
        items.sort_by(|a, b| {
            b.unread_count
                .cmp(&a.unread_count)
                .then_with(|| b.last_message_at.cmp(&a.last_message_at))
                .then_with(|| normalize_text(&a.label).cmp(&normalize_text(&b.label)))
        });
    }

    items
}

fn item_matches_query(item: &RoomSearchItem, query: &SearchQuery) -> bool {
    if query.kind == SearchQueryKind::Dms && !item.label.starts_with('@') {
        return false;
    }
    if query.kind == SearchQueryKind::Rooms && !item.label.starts_with('#') {
        return false;
    }
    let label = normalize_text(&item.label);
    let meta = normalize_text(&item.meta);
    query.text.is_empty() || label.contains(&query.text) || meta.contains(&query.text)
}

fn synthetic_item(slot: RoomSlot, chat: &ChatState) -> RoomSearchItem {
    let (label, meta, unread_count) = match slot {
        RoomSlot::Feeds => ("feeds", "rss inbox", chat.feeds.unread_count()),
        RoomSlot::News => ("news", "shared links", chat.news.unread_count()),
        RoomSlot::Notifications => (
            "mentions",
            "notifications",
            chat.notifications.unread_count(),
        ),
        RoomSlot::Discover => ("browse rooms", "custom rooms", 0),
        RoomSlot::Showcase => ("showcases", "projects", chat.showcase.unread_count()),
        RoomSlot::Work => ("work", "profiles", chat.work.unread_count()),
        RoomSlot::Room(_) => unreachable!("real rooms are built from ChatRoom"),
    };

    RoomSearchItem {
        slot,
        label: label.to_string(),
        meta: meta.to_string(),
        unread_count,
        last_message_at: None,
        favorite: false,
    }
}

fn room_label(
    room: &ChatRoom,
    current_user_id: Uuid,
    usernames: &std::collections::HashMap<Uuid, String>,
) -> String {
    if room.kind == "dm" {
        return format!("@{}", dm_peer_label(room, current_user_id, usernames));
    }
    if let Some(slug) = room.slug.as_deref().filter(|slug| !slug.is_empty()) {
        return format!("#{slug}");
    }
    if let Some(code) = room
        .language_code
        .as_deref()
        .filter(|code| !code.is_empty())
    {
        return format!("#lang-{code}");
    }
    format!("#{}", room.kind)
}

fn dm_peer_label(
    room: &ChatRoom,
    current_user_id: Uuid,
    usernames: &std::collections::HashMap<Uuid, String>,
) -> String {
    let peer_id = if room.dm_user_a == Some(current_user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    };
    peer_id
        .and_then(|id| usernames.get(&id).cloned())
        .unwrap_or_else(|| "DM".to_string())
}

fn room_meta(room: &ChatRoom) -> String {
    match room.kind.as_str() {
        "dm" => "direct message".to_string(),
        _ if room.permanent => "core room".to_string(),
        _ if room.visibility == "private" => "private room".to_string(),
        _ if room.visibility == "public" => "public room".to_string(),
        _ => "room".to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchQueryKind {
    All,
    Rooms,
    Dms,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchQuery {
    kind: SearchQueryKind,
    text: String,
}

impl SearchQuery {
    fn parse(input: &str) -> Self {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix('@') {
            return Self {
                kind: SearchQueryKind::Dms,
                text: normalize_text(rest),
            };
        }

        if let Some(rest) = trimmed.strip_prefix('#') {
            return Self {
                kind: SearchQueryKind::Rooms,
                text: normalize_text(rest),
            };
        }

        Self {
            kind: SearchQueryKind::All,
            text: normalize_text(trimmed),
        }
    }
}

fn normalize_text(input: &str) -> String {
    input.trim().trim_start_matches(['#', '@']).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn room(kind: &str, visibility: &str, slug: Option<&str>) -> ChatRoom {
        ChatRoom {
            id: Uuid::from_u128(1),
            created: Utc::now(),
            updated: Utc::now(),
            kind: kind.to_string(),
            visibility: visibility.to_string(),
            auto_join: false,
            permanent: false,
            slug: slug.map(str::to_string),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        }
    }

    fn item(label: &str, meta: &str, unread_count: i64) -> RoomSearchItem {
        RoomSearchItem {
            slot: RoomSlot::Room(Uuid::from_u128(1)),
            label: label.to_string(),
            meta: meta.to_string(),
            unread_count,
            last_message_at: None,
            favorite: false,
        }
    }

    #[test]
    fn query_ignores_room_prefixes() {
        assert_eq!(SearchQuery::parse("#general").text, "general");
        assert_eq!(SearchQuery::parse("@alice").text, "alice");
    }

    #[test]
    fn bare_at_filters_to_dms() {
        assert_eq!(
            SearchQuery::parse("@"),
            SearchQuery {
                kind: SearchQueryKind::Dms,
                text: String::new()
            }
        );
    }

    #[test]
    fn prefixed_queries_select_room_kind() {
        assert_eq!(SearchQuery::parse("@alice").kind, SearchQueryKind::Dms);
        assert_eq!(SearchQuery::parse("#general").kind, SearchQueryKind::Rooms);
        assert_eq!(SearchQuery::parse("general").kind, SearchQueryKind::All);
    }

    #[test]
    fn bare_at_matches_all_dms() {
        let query = SearchQuery::parse("@");
        assert!(item_matches_query(
            &item("@alice", "direct message", 2),
            &query
        ));
        assert!(item_matches_query(
            &item("@bob", "direct message", 0),
            &query
        ));
        assert!(!item_matches_query(
            &item("#general", "core room", 3),
            &query
        ));
    }

    #[test]
    fn named_at_matches_dms_by_name_or_meta() {
        let query = SearchQuery::parse("@ali");
        assert!(item_matches_query(
            &item("@alice", "direct message", 0),
            &query
        ));
        assert!(!item_matches_query(
            &item("#alice", "public room", 0),
            &query
        ));
        assert!(!item_matches_query(
            &item("@bob", "direct message", 0),
            &query
        ));
    }

    #[test]
    fn delete_word_left_stops_at_room_prefix() {
        let mut state = RoomSearchModalState {
            query: "#general chat".to_string(),
            ..RoomSearchModalState::default()
        };
        state.delete_word_left();
        assert_eq!(state.query, "#general ");
        state.delete_word_left();
        assert_eq!(state.query, "#");
    }

    #[test]
    fn room_labels_prefix_rooms_and_dms() {
        let current = Uuid::from_u128(1);
        let peer = Uuid::from_u128(2);
        let mut usernames = std::collections::HashMap::new();
        usernames.insert(peer, "alice".to_string());

        let public = room("topic", "public", Some("rust"));
        assert_eq!(room_label(&public, current, &usernames), "#rust");

        let mut dm = room("dm", "dm", None);
        dm.dm_user_a = Some(current);
        dm.dm_user_b = Some(peer);
        assert_eq!(room_label(&dm, current, &usernames), "@alice");
    }
}
