use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use late_core::models::chat_room::ChatRoom;
use uuid::Uuid;

use crate::app::chat::state::{ChatState, RoomSlot, is_chat_list_room, room_activity_at};
use crate::app::chat::svc::SEARCH_MIN_CHARS;

/// Quiet time after the last keystroke before a message search fires.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

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
    /// Time of the last query edit; message searches fire only after
    /// `SEARCH_DEBOUNCE` of quiet.
    last_edit: Option<Instant>,
    /// The `(scope, text)` of the last fired search, so an unchanged query
    /// never refires.
    last_fired_key: Option<(Option<Uuid>, String)>,
}

impl RoomSearchModalState {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.last_edit = None;
        self.last_fired_key = None;
    }

    /// Open pre-filled (the `/search` command path). The debounce timestamp
    /// is set so the search fires on its own shortly after the modal opens.
    pub(crate) fn open_with_query(&mut self, query: String) {
        self.open();
        self.query = query;
        self.last_edit = Some(
            Instant::now()
                .checked_sub(SEARCH_DEBOUNCE)
                .unwrap_or_else(Instant::now),
        );
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
        self.last_edit = None;
        self.last_fired_key = None;
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
            self.last_edit = Some(Instant::now());
        }
    }

    pub(crate) fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.last_edit = Some(Instant::now());
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
        self.last_edit = Some(Instant::now());
    }

    fn debounce_elapsed(&self) -> bool {
        self.last_edit
            .is_some_and(|at| at.elapsed() >= SEARCH_DEBOUNCE)
    }

    /// Whether the query was edited within the debounce window, meaning a
    /// search is about to fire. Render uses this to show "Searching..."
    /// instead of a premature "No matching messages".
    pub(crate) fn query_recently_edited(&self) -> bool {
        self.last_edit
            .is_some_and(|at| at.elapsed() < SEARCH_DEBOUNCE)
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
                    unread_count: chat.unread_counts.get(&room.id).copied().unwrap_or(0),
                    last_message_at: room_activity_at(room.id, &chat.room_last_message_at),
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
    sort_picker_items(&mut items);
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

    sort_picker_items(&mut items);

    items
}

fn sort_picker_items(items: &mut [RoomSearchItem]) {
    items.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then_with(|| (b.unread_count > 0).cmp(&(a.unread_count > 0)))
            .then_with(|| b.last_message_at.cmp(&a.last_message_at))
            .then_with(|| normalize_text(&a.label).cmp(&normalize_text(&b.label)))
    });
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
        RoomSlot::Feeds => ("rss", "rss inbox", chat.feeds.unread_count()),
        RoomSlot::News => ("news", "shared links", chat.news.unread_count()),
        RoomSlot::Notifications => (
            "mentions",
            "notifications",
            chat.notifications.unread_count(),
        ),
        RoomSlot::Discover => ("browse rooms", "custom rooms", 0),
        RoomSlot::Showcase => ("showcases", "projects", chat.showcase.unread_count()),
        RoomSlot::Work => ("work", "profiles", chat.work.unread_count()),
        RoomSlot::Room(_) => {
            unreachable!("real rooms are built from ChatRoom")
        }
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

/// What the modal's query line currently means. A leading `?` flips from
/// room jumping to message search; inside message search, the familiar `#`
/// and `@` prefixes scope to one room or one DM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModalQuery {
    Rooms,
    Messages(MessageQuery),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MessageQuery {
    pub scope: Option<MessageScope>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MessageScope {
    Room(String),
    Dm(String),
}

pub(crate) fn parse_modal_query(input: &str) -> ModalQuery {
    let Some(rest) = input.trim_start().strip_prefix('?') else {
        return ModalQuery::Rooms;
    };
    let rest = rest.trim_start();
    let (scope, text) = if let Some(rest) = rest.strip_prefix('#') {
        let (token, text) = split_first_token(rest);
        (Some(MessageScope::Room(token.to_lowercase())), text)
    } else if let Some(rest) = rest.strip_prefix('@') {
        let (token, text) = split_first_token(rest);
        (Some(MessageScope::Dm(token.to_lowercase())), text)
    } else {
        (None, rest)
    };
    ModalQuery::Messages(MessageQuery {
        scope,
        text: text.trim().to_string(),
    })
}

fn split_first_token(input: &str) -> (&str, &str) {
    match input.find(char::is_whitespace) {
        Some(at) => (&input[..at], input[at..].trim_start()),
        None => (input, ""),
    }
}

/// Resolve a `#slug` / `@user` search scope against the user's joined rooms.
/// `None` means the token does not name a joined room/DM (yet), so the
/// search must not fire.
pub(crate) fn resolve_message_scope(
    chat: &ChatState,
    current_user_id: Uuid,
    scope: &MessageScope,
) -> Option<Uuid> {
    match scope {
        MessageScope::Room(slug) => {
            if slug.is_empty() {
                return None;
            }
            chat.rooms.iter().find_map(|(room, _)| {
                (room.kind != "dm"
                    && is_chat_list_room(room)
                    && room_label(room, current_user_id, &chat.usernames)
                        .trim_start_matches('#')
                        .eq_ignore_ascii_case(slug))
                .then_some(room.id)
            })
        }
        MessageScope::Dm(name) => {
            if name.is_empty() {
                return None;
            }
            chat.rooms.iter().find_map(|(room, _)| {
                (room.kind == "dm"
                    && dm_peer_label(room, current_user_id, &chat.usernames)
                        .eq_ignore_ascii_case(name))
                .then_some(room.id)
            })
        }
    }
}

/// Display label (`#slug` / `@peer`) for a search hit's room, resolved from
/// the user's joined-room list.
pub(crate) fn hit_room_label(chat: &ChatState, current_user_id: Uuid, room_id: Uuid) -> String {
    chat.rooms
        .iter()
        .find(|(room, _)| room.id == room_id)
        .map(|(room, _)| room_label(room, current_user_id, &chat.usernames))
        .unwrap_or_else(|| "#?".to_string())
}

/// Per-frame driver for the modal's message-search mode: once the query has
/// sat unchanged for `SEARCH_DEBOUNCE`, is long enough, and any scope token
/// resolves, fire one search through `ChatState` (latest wins; an unchanged
/// query never refires). Called from `App::tick`.
pub(crate) fn tick_message_search(app: &mut crate::app::state::App) {
    if !app.room_search_modal_state.is_open() {
        return;
    }
    let ModalQuery::Messages(query) = parse_modal_query(app.room_search_modal_state.query()) else {
        return;
    };
    // Context window for the selected hit, independent of the query gates
    // below so it also covers the Mentions single-message preview (empty
    // query text).
    let hits = &app.chat.message_search.hits;
    if !hits.is_empty() {
        let selected = app
            .room_search_modal_state
            .selected()
            .min(hits.len().saturating_sub(1));
        let message_id = hits[selected].message.id;
        app.chat.ensure_search_hit_context(message_id);
    }
    if query.text.chars().count() < SEARCH_MIN_CHARS {
        return;
    }
    let scope_room_id = match &query.scope {
        Some(scope) => match resolve_message_scope(&app.chat, app.user_id, scope) {
            Some(room_id) => Some(room_id),
            None => return,
        },
        None => None,
    };
    if !app.room_search_modal_state.debounce_elapsed() {
        return;
    }
    let key = (scope_room_id, query.text.clone());
    if app.room_search_modal_state.last_fired_key.as_ref() == Some(&key) {
        return;
    }
    app.chat.start_message_search(scope_room_id, query.text);
    app.room_search_modal_state.last_fired_key = Some(key);
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
