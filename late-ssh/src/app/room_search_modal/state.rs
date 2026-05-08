use late_core::models::chat_room::ChatRoom;
use uuid::Uuid;

use crate::app::chat::state::{ChatState, RoomSlot, is_chat_list_room};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoomSearchItem {
    pub slot: RoomSlot,
    pub label: String,
    pub meta: String,
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
        let RoomSlot::Room(room_id) = slot else {
            continue;
        };
        let Some((room, _)) = chat.rooms.iter().find(|(room, _)| room.id == room_id) else {
            continue;
        };
        if !is_chat_list_room(room) {
            continue;
        }
        items.push(RoomSearchItem {
            slot,
            label: room_label(room, current_user_id, &chat.usernames),
            meta: room_meta(room),
        });
    }
    items
}

pub(crate) fn filtered_items(
    chat: &ChatState,
    current_user_id: Uuid,
    query: &str,
) -> Vec<RoomSearchItem> {
    let query = normalize_query(query);
    let all = search_items(chat, current_user_id);
    if query.is_empty() {
        return all;
    }

    all.into_iter()
        .filter(|item| {
            let label = normalize_query(&item.label);
            let meta = normalize_query(&item.meta);
            label.contains(&query) || meta.contains(&query)
        })
        .collect()
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

fn normalize_query(input: &str) -> String {
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

    #[test]
    fn query_ignores_room_prefixes() {
        assert_eq!(normalize_query("#general"), "general");
        assert_eq!(normalize_query("@alice"), "alice");
    }

    #[test]
    fn delete_word_left_stops_at_room_prefix() {
        let mut state = RoomSearchModalState::default();
        state.query = "#general chat".to_string();
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
