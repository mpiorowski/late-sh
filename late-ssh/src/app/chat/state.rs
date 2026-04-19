use std::collections::{HashMap, HashSet, VecDeque};

use late_core::{
    MutexRecover,
    models::{article::NEWS_MARKER, chat_message::ChatMessage, chat_room::ChatRoom},
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::common::overlay::Overlay;

use crate::app::common::{composer::ComposerState, primitives::Banner};
use crate::app::help_modal::data::HelpTopic;
use crate::state::{ActiveUser, ActiveUsers};

use super::{
    news, notifications,
    notifications::svc::NotificationService,
    svc::{ChatEvent, ChatService, ChatSnapshot},
};

pub(crate) const ROOM_JUMP_KEYS: &[u8] = b"asdfghjklqwertyuiopzxcvbnm1234567890";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionMatch {
    pub name: String,
    pub online: bool,
}

#[derive(Default)]
pub(crate) struct MentionAutocomplete {
    pub active: bool,
    pub query: String,
    pub trigger_offset: usize,
    pub matches: Vec<MentionMatch>,
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplyTarget {
    pub author: String,
    pub preview: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoomSlot {
    Room(Uuid),
    News,
    Notifications,
}

pub struct ChatState {
    pub(crate) service: ChatService,
    user_id: Uuid,
    is_admin: bool,
    active_users: Option<ActiveUsers>,
    snapshot_rx: watch::Receiver<ChatSnapshot>,
    event_rx: tokio::sync::broadcast::Receiver<ChatEvent>,
    pub(crate) rooms: Vec<(ChatRoom, Vec<ChatMessage>)>,
    general_room_id: Option<Uuid>,
    pub(crate) usernames: HashMap<Uuid, String>,
    pub(crate) countries: HashMap<Uuid, String>,
    ignored_user_ids: HashSet<Uuid>,
    overlay: Option<Overlay>,
    pub(crate) unread_counts: HashMap<Uuid, i64>,
    pending_read_rooms: HashSet<Uuid>,
    room_tx: watch::Sender<Option<Uuid>>,
    pub(crate) selected_room_id: Option<Uuid>,
    pub(crate) room_jump_active: bool,
    composer: ComposerState,
    pub(crate) composing: bool,
    composer_room_id: Option<Uuid>,
    pending_send_notices: VecDeque<Uuid>,
    pub(crate) pending_dm_screen_switch: bool,
    pub(crate) mention_ac: MentionAutocomplete,
    pub(crate) all_usernames: Vec<String>,
    pub(crate) bonsai_glyphs: HashMap<Uuid, String>,
    pub(crate) selected_message_id: Option<Uuid>,
    pub(crate) highlighted_message_id: Option<Uuid>,
    pub(crate) edited_message_id: Option<Uuid>,
    pub(crate) reply_target: Option<ReplyTarget>,
    bg_task: tokio::task::AbortHandle,

    /// News (shown as a virtual room in the room list)
    pub(crate) news_selected: bool,
    pub(crate) news: news::state::State,

    /// Notifications / mentions (shown as a virtual room in the room list)
    pub(crate) notifications_selected: bool,
    pub(crate) notifications: notifications::state::State,

    /// Pending desktop notifications drained on render. `kind` matches the
    /// string identifiers stored in `users.settings.notify_kinds` ("dms", "mentions").
    pub(crate) pending_notifications: Vec<PendingNotification>,
    requested_help_topic: Option<HelpTopic>,
}

pub(crate) struct PendingNotification {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
}

impl Drop for ChatState {
    fn drop(&mut self) {
        self.bg_task.abort();
    }
}

impl ChatState {
    pub fn new(
        service: ChatService,
        notification_service: NotificationService,
        user_id: Uuid,
        is_admin: bool,
        active_users: Option<ActiveUsers>,
        article_service: news::svc::ArticleService,
    ) -> Self {
        let snapshot_rx = service.subscribe_state();
        let event_rx = service.subscribe_events();
        let (room_tx, room_rx) = watch::channel(None);
        let bg_task = service.start_user_refresh_task(user_id, room_rx);

        Self {
            service,
            user_id,
            is_admin,
            active_users,
            snapshot_rx,
            event_rx,
            rooms: Vec::new(),
            general_room_id: None,
            usernames: HashMap::new(),
            countries: HashMap::new(),
            ignored_user_ids: HashSet::new(),
            overlay: None,
            unread_counts: HashMap::new(),
            pending_read_rooms: HashSet::new(),
            room_tx,
            selected_room_id: None,
            room_jump_active: false,
            composer: ComposerState::new(80),
            composing: false,
            composer_room_id: None,
            pending_send_notices: VecDeque::new(),
            pending_dm_screen_switch: false,
            mention_ac: MentionAutocomplete::default(),
            all_usernames: Vec::new(),
            bonsai_glyphs: HashMap::new(),
            selected_message_id: None,
            highlighted_message_id: None,
            edited_message_id: None,
            reply_target: None,
            bg_task,
            news_selected: false,
            news: news::state::State::new(article_service, user_id, is_admin),
            notifications_selected: false,
            notifications: notifications::state::State::new(notification_service, user_id),
            pending_notifications: Vec::new(),
            requested_help_topic: None,
        }
    }

    pub fn set_composer_text_width(&mut self, width: usize) {
        self.composer.set_text_width(width);
    }

    pub fn sync_composer_layout(&mut self) {
        self.composer.sync_layout();
    }

    pub(crate) fn composer_rows(&self) -> &[crate::app::common::composer::ComposerRow] {
        self.composer.rows()
    }

    pub(crate) fn composer_text(&self) -> &str {
        self.composer.text()
    }

    pub(crate) fn composer_cursor(&self) -> usize {
        self.composer.cursor()
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn start_composing(&mut self) {
        if let Some(room_id) = self.selected_room_id {
            self.start_composing_in_room(room_id);
        }
    }

    pub fn start_composing_in_room(&mut self, room_id: Uuid) {
        self.room_jump_active = false;
        self.composing = true;
        self.composer_room_id = Some(room_id);
        self.selected_message_id = None;
        self.reply_target = None;
        self.edited_message_id = None;
    }

    pub fn request_list(&self) {
        self.service
            .list_chats_task(self.user_id, self.selected_room_id);
    }

    pub fn sync_selection(&mut self) {
        if self.rooms.is_empty() {
            self.selected_room_id = None;
            self.room_jump_active = false;
            return;
        }

        if let Some(selected_id) = self.selected_room_id
            && self.rooms.iter().any(|(room, _)| room.id == selected_id)
        {
            return;
        }

        self.selected_room_id = Some(self.rooms[0].0.id);
    }

    pub fn mark_selected_room_read(&mut self) {
        let Some(room_id) = self.selected_room_id else {
            return;
        };

        self.pending_read_rooms.insert(room_id);
        self.unread_counts.insert(room_id, 0);
        self.service.mark_room_read_task(self.user_id, room_id);
    }

    /// Returns visible messages for the given room.
    fn visible_messages_for_room(&self, room_id: Uuid) -> Vec<&ChatMessage> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(_, msgs)| msgs.iter().collect())
            .unwrap_or_default()
    }

    pub(crate) fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
    }

    pub fn scroll_overlay(&mut self, delta: i16) {
        if let Some(overlay) = &mut self.overlay {
            overlay.scroll(delta);
        }
    }

    pub fn take_requested_help_topic(&mut self) -> Option<HelpTopic> {
        self.requested_help_topic.take()
    }

    fn select_from_ids(&mut self, ids: &[Uuid], delta: isize) {
        if ids.is_empty() {
            self.selected_message_id = None;
            return;
        }

        let current_idx = self
            .selected_message_id
            .and_then(|id| ids.iter().position(|mid| *mid == id));

        let new_idx = match current_idx {
            Some(idx) => (idx as isize)
                .saturating_add(delta)
                .clamp(0, ids.len() as isize - 1) as usize,
            None => 0,
        };

        self.selected_message_id = Some(ids[new_idx]);
    }

    /// Move message cursor by delta. Positive = toward older, negative = toward newer.
    /// First press activates cursor on the newest message.
    pub fn select_message_in_room(&mut self, room_id: Uuid, delta: isize) {
        self.highlighted_message_id = None;
        let ids: Vec<Uuid> = self
            .visible_messages_for_room(room_id)
            .iter()
            .map(|m| m.id)
            .collect();
        self.select_from_ids(&ids, delta);
    }

    pub fn clear_message_selection(&mut self) {
        self.selected_message_id = None;
    }

    pub fn begin_reply_to_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        let message = self.selected_message_in_room(room_id)?;
        let message_user_id = message.user_id;
        let message_body = message.body.clone();
        let author = self
            .usernames
            .get(&message_user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(message_user_id));
        self.reply_target = Some(ReplyTarget {
            author,
            preview: reply_preview_text(&message_body),
        });
        self.composing = true;
        self.composer_room_id = Some(room_id);
        self.edited_message_id = None;
        None
    }

    pub fn begin_edit_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        let selected_id = self.selected_message_id?;
        let Some(message) = self.find_message_in_room(room_id, selected_id) else {
            return Some(Banner::error("Selected message not found"));
        };
        let message_user_id = message.user_id;
        let room_id = message.room_id;
        let body = message.body.clone();
        self.begin_edit_message(selected_id, message_user_id, room_id, &body)
    }

    fn begin_edit_message(
        &mut self,
        selected_id: Uuid,
        message_user_id: Uuid,
        room_id: Uuid,
        body: &str,
    ) -> Option<Banner> {
        let is_own = message_user_id == self.user_id;
        if !is_own && !self.is_admin {
            return Some(Banner::error("Can only edit your own messages"));
        }
        self.edited_message_id = Some(selected_id);
        self.composer.set_text(body);
        self.composing = true;
        self.composer_room_id = Some(room_id);
        None
    }

    pub(crate) fn reply_target(&self) -> Option<&ReplyTarget> {
        self.reply_target.as_ref()
    }

    /// Delete the selected message if owned by user (or if admin).
    /// Moves selection to the adjacent message (prefer the next/older one,
    /// fall back to the previous/newer one) so pressing `d` repeatedly
    /// cleanly reaps a run of own messages without the cursor jumping
    /// back to the newest every time.
    pub fn delete_selected_message_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        let selected_id = self.selected_message_id?;
        let msg_user_id = self
            .find_message_in_room(room_id, selected_id)
            .map(|m| m.user_id)?;
        let is_own = msg_user_id == self.user_id;
        if !is_own && !self.is_admin {
            return Some(Banner::error("Can only delete your own messages"));
        }
        self.service
            .delete_message_task(self.user_id, selected_id, self.is_admin);
        self.selected_message_id = self
            .rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, msgs)| adjacent_message_id(msgs, selected_id));
        Some(Banner::success("Deleting message..."))
    }

    fn selected_message_in_room(&self, room_id: Uuid) -> Option<&ChatMessage> {
        let selected_id = self.selected_message_id?;
        self.find_message_in_room(room_id, selected_id)
    }

    pub fn selected_message_author_in_room(&self, room_id: Uuid) -> Option<(Uuid, String)> {
        let message = self.selected_message_in_room(room_id)?;
        let user_id = message.user_id;
        let display_name = self
            .usernames
            .get(&user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(user_id));
        Some((user_id, display_name))
    }

    fn find_message_in_room(&self, room_id: Uuid, message_id: Uuid) -> Option<&ChatMessage> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, msgs)| msgs.iter().find(|m| m.id == message_id))
    }

    fn selected_room_slug(&self) -> Option<String> {
        self.selected_room().and_then(|room| room.slug.clone())
    }

    fn selected_room(&self) -> Option<&ChatRoom> {
        let room_id = self.selected_room_id?;
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(room, _)| room)
    }

    pub fn general_room_id(&self) -> Option<Uuid> {
        self.general_room_id.or_else(|| {
            self.rooms
                .iter()
                .find(|(room, _)| room.kind == "general" && room.slug.as_deref() == Some("general"))
                .map(|(room, _)| room.id)
        })
    }

    fn dm_display_name(&self, room: &ChatRoom) -> String {
        dm_sort_key(room, self.user_id, &self.usernames)
    }

    /// Build the flat visual navigation order.
    /// Order: core (general, announcements) → news → mentions → public rooms (alpha) → private (alpha) → DMs
    pub(crate) fn visual_order(&self) -> Vec<RoomSlot> {
        let mut order = Vec::new();

        // Core: permanent rooms, hardcoded order
        let core_order = ["general", "announcements", "suggestions", "bugs"];
        for slug in &core_order {
            if let Some((room, _)) = self
                .rooms
                .iter()
                .find(|(r, _)| r.permanent && r.slug.as_deref() == Some(slug))
            {
                order.push(RoomSlot::Room(room.id));
            }
        }
        // Any other permanent rooms not in the hardcoded list
        for (room, _) in &self.rooms {
            if room.kind != "dm"
                && room.permanent
                && !core_order.contains(&room.slug.as_deref().unwrap_or(""))
            {
                order.push(RoomSlot::Room(room.id));
            }
        }

        // News
        order.push(RoomSlot::News);

        // Mentions / notifications
        order.push(RoomSlot::Notifications);

        // Public rooms (auto_join, alpha by slug)
        let mut public: Vec<_> = self
            .rooms
            .iter()
            .filter(|(r, _)| r.kind != "dm" && !r.permanent && r.auto_join)
            .collect();
        public.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
        order.extend(public.iter().map(|(r, _)| RoomSlot::Room(r.id)));

        // Private rooms (!auto_join, alpha by slug)
        let mut private: Vec<_> = self
            .rooms
            .iter()
            .filter(|(r, _)| r.kind != "dm" && !r.permanent && !r.auto_join)
            .collect();
        private.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
        order.extend(private.iter().map(|(r, _)| RoomSlot::Room(r.id)));

        // DMs (sorted by display name to match nav rendering)
        let mut dms: Vec<_> = self.rooms.iter().filter(|(r, _)| r.kind == "dm").collect();
        dms.sort_by(|(a, _), (b, _)| {
            let name_a = self.dm_display_name(a);
            let name_b = self.dm_display_name(b);
            name_a.cmp(&name_b)
        });
        order.extend(dms.iter().map(|(r, _)| RoomSlot::Room(r.id)));

        order
    }

    pub(crate) fn room_jump_targets(&self) -> Vec<(u8, RoomSlot)> {
        self.visual_order()
            .into_iter()
            .zip(ROOM_JUMP_KEYS.iter().copied())
            .map(|(slot, key)| (key, slot))
            .collect()
    }

    fn select_room_slot(&mut self, slot: RoomSlot) -> bool {
        self.selected_message_id = None;
        self.highlighted_message_id = None;

        match slot {
            RoomSlot::News => {
                let changed = !self.news_selected;
                if changed {
                    self.select_news();
                }
                changed
            }
            RoomSlot::Notifications => {
                let changed = !self.notifications_selected;
                if changed {
                    self.select_notifications();
                }
                changed
            }
            RoomSlot::Room(next_id) => {
                let changed = self.news_selected
                    || self.notifications_selected
                    || self.selected_room_id != Some(next_id);
                self.news_selected = false;
                self.notifications_selected = false;
                self.selected_room_id = Some(next_id);
                changed
            }
        }
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        let order = self.visual_order();
        if order.is_empty() {
            return false;
        }

        let current_item = if self.notifications_selected {
            RoomSlot::Notifications
        } else if self.news_selected {
            RoomSlot::News
        } else {
            self.selected_room_id
                .map(RoomSlot::Room)
                .unwrap_or(RoomSlot::News)
        };
        let current = order
            .iter()
            .position(|item| *item == current_item)
            .unwrap_or(0) as isize;
        let next = wrapped_index(current, delta, order.len());
        self.select_room_slot(order[next])
    }

    pub fn activate_room_jump(&mut self) {
        self.room_jump_active = !self.composing && !self.rooms.is_empty();
    }

    pub fn cancel_room_jump(&mut self) {
        self.room_jump_active = false;
    }

    pub fn handle_room_jump_key(&mut self, byte: u8) -> bool {
        let targets = self.room_jump_targets();
        let Some(slot) = resolve_room_jump_target(&targets, byte) else {
            self.room_jump_active = false;
            return false;
        };

        self.room_jump_active = false;
        self.select_room_slot(slot)
    }

    pub fn stop_composing(&mut self) {
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reply_target = None;
    }

    pub fn reset_composer(&mut self) {
        self.composer.clear();
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reply_target = None;
        self.edited_message_id = None;
        self.mention_ac = MentionAutocomplete::default();
    }

    fn clear_composer_after_submit(&mut self) {
        self.composer.clear();
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reply_target = None;
        self.edited_message_id = None;
    }

    fn clear_composer_after_send(&mut self) {
        self.composer.clear();
        self.room_jump_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
    }

    fn open_overlay(&mut self, title: &str, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        self.overlay = Some(Overlay::new(title, lines));
    }

    fn ignore_list_lines(&self) -> Vec<String> {
        if self.ignored_user_ids.is_empty() {
            return vec!["Ignore list is empty".to_string()];
        }

        let mut labels: Vec<String> = self
            .ignored_user_ids
            .iter()
            .map(|id| {
                self.usernames
                    .get(id)
                    .map(|name| format!("@{name}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(*id)))
            })
            .collect();
        labels.sort();
        labels
    }

    fn active_user_lines(&self) -> Vec<String> {
        format_active_user_lines(self.active_users.as_ref())
    }

    pub fn submit_composer(&mut self, keep_open: bool) -> Option<Banner> {
        let body = self.composer.text().trim_end().to_string();

        if body.trim() == "/help" {
            self.clear_composer_after_submit();
            self.requested_help_topic = Some(HelpTopic::Chat);
            return None;
        }

        if body.trim() == "/music" {
            self.clear_composer_after_submit();
            self.requested_help_topic = Some(HelpTopic::Music);
            return None;
        }

        if body.trim() == "/active" {
            self.clear_composer_after_submit();
            self.open_overlay("Active Users", self.active_user_lines());
            return None;
        }

        if body.trim() == "/list" {
            self.clear_composer_after_submit();
            let Some(room) = self.selected_room() else {
                return Some(Banner::error("No room selected"));
            };
            if !room_supports_member_list(room) {
                return Some(Banner::error("/list only works in private rooms"));
            }
            self.service.list_room_members_task(self.user_id, room.id);
            return None;
        }

        if let Some(target) = parse_user_command(&body, "/ignore") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Ignored Users", self.ignore_list_lines()),
                Some(name) => self
                    .service
                    .ignore_user_task(self.user_id, name.to_string()),
            }
            return None;
        }
        if let Some(target) = parse_user_command(&body, "/unignore") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Ignored Users", self.ignore_list_lines()),
                Some(name) => self
                    .service
                    .unignore_user_task(self.user_id, name.to_string()),
            }
            return None;
        }

        if let Some(target) = parse_dm_command(&body) {
            self.service.start_dm_task(self.user_id, target.to_string());
            self.clear_composer_after_submit();
            return Some(Banner::success(&format!("Opening DM with {target}...")));
        }

        if let Some(room) = parse_join_command(&body) {
            self.service.join_room_task(self.user_id, room.to_string());
            self.clear_composer_after_submit();
            return Some(Banner::success(&format!("Joining #{room}...")));
        }

        if parse_leave_command(&body) {
            self.clear_composer_after_submit();
            if let Some(room_id) = self.selected_room_id {
                let slug = self.selected_room_slug().unwrap_or_default();
                self.service
                    .leave_room_task(self.user_id, room_id, slug.clone());
                return Some(Banner::success(&format!("Leaving #{slug}...")));
            } else {
                return Some(Banner::error("No room selected"));
            }
        }

        if let Some(slug) = parse_create_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.is_admin {
                return Some(Banner::error("Admin only: /create-room"));
            }
            self.service
                .create_permanent_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Creating #{slug}...")));
        }

        if let Some(slug) = parse_create_command(&body) {
            self.clear_composer_after_submit();
            self.service
                .create_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Creating #{slug}...")));
        }

        if let Some(slug) = parse_delete_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.is_admin {
                return Some(Banner::error("Admin only: /delete-room"));
            }
            self.service
                .delete_permanent_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Deleting #{slug}...")));
        }

        if let Some(room_id) = self.composer_room_id
            && !body.is_empty()
        {
            let request_id = Uuid::now_v7();
            let body = if let Some(reply) = &self.reply_target {
                format!("> @{}: {}\n{}", reply.author, reply.preview, body)
            } else {
                body
            };
            if let Some(message_id) = self.edited_message_id {
                self.service.edit_message_task(
                    self.user_id,
                    message_id,
                    body,
                    request_id,
                    self.is_admin,
                );
            } else {
                self.service.send_message_task(
                    self.user_id,
                    room_id,
                    self.selected_room_slug(),
                    body,
                    request_id,
                    self.is_admin,
                );
            }
            self.pending_send_notices.push_back(request_id);
        }
        if keep_open {
            self.clear_composer_after_send();
        } else {
            self.clear_composer_after_submit();
        }
        None
    }

    pub fn composer_clear(&mut self) {
        self.composer.clear();
    }
    pub fn composer_backspace(&mut self) {
        self.composer.backspace();
    }

    pub fn composer_delete_right(&mut self) {
        self.composer.delete_right();
    }

    pub fn composer_delete_word_right(&mut self) {
        self.composer.delete_word_right();
    }

    pub fn composer_delete_word_left(&mut self) {
        self.composer.delete_word_left();
    }

    pub fn composer_push(&mut self, ch: char) {
        self.composer.push(ch);
    }

    pub fn composer_cursor_left(&mut self) {
        self.composer.cursor_left();
    }

    pub fn composer_cursor_right(&mut self) {
        self.composer.cursor_right();
    }

    pub fn composer_cursor_word_left(&mut self) {
        self.composer.cursor_word_left();
    }

    pub fn composer_cursor_word_right(&mut self) {
        self.composer.cursor_word_right();
    }

    pub fn composer_cursor_up(&mut self) {
        self.composer.cursor_up();
    }

    pub fn composer_cursor_down(&mut self) {
        self.composer.cursor_down();
    }

    pub fn tick(&mut self) -> Option<Banner> {
        let _ = self.room_tx.send(self.selected_room_id);
        self.drain_snapshot();
        let banner = self.drain_events();
        let news_banner = self.news.tick();
        let notif_banner = self.notifications.tick();
        banner.or(news_banner).or(notif_banner)
    }

    pub fn select_news(&mut self) {
        self.room_jump_active = false;
        self.news_selected = true;
        self.notifications_selected = false;
        self.selected_message_id = None;
        self.highlighted_message_id = None;
        self.news.list_articles();
        self.news.mark_read();
    }

    pub fn deselect_news(&mut self) {
        self.news_selected = false;
    }

    pub fn select_notifications(&mut self) {
        self.room_jump_active = false;
        self.notifications_selected = true;
        self.news_selected = false;
        self.selected_message_id = None;
        self.highlighted_message_id = None;
        self.notifications.list();
        self.notifications.mark_read();
    }

    pub fn cursor_visible(&self) -> bool {
        self.composing
    }

    pub fn is_autocomplete_active(&self) -> bool {
        self.mention_ac.active
    }

    pub fn update_autocomplete(&mut self) {
        // Scan backward from end of composer to find a trigger `@`
        let bytes = self.composer.text().as_bytes();
        let mut at_offset = None;
        for i in (0..bytes.len()).rev() {
            if bytes[i] == b'@' {
                // Valid if at start or preceded by whitespace
                if i == 0 || bytes[i - 1] == b' ' {
                    at_offset = Some(i);
                }
                break;
            }
            // Stop scanning if we hit a space (no @ in this word)
            if bytes[i] == b' ' {
                break;
            }
        }

        let Some(offset) = at_offset else {
            self.mention_ac.active = false;
            return;
        };

        let query = &self.composer.text()[offset + 1..];
        let query_lower = query.to_ascii_lowercase();
        let active_users = self.active_users.as_ref();
        let matches = rank_mention_matches(&self.all_usernames, &query_lower, || {
            online_username_set(active_users)
        });

        if matches.is_empty() {
            self.mention_ac.active = false;
            return;
        }

        self.mention_ac.active = true;
        self.mention_ac.query = query.to_string();
        self.mention_ac.trigger_offset = offset;
        self.mention_ac.selected = self
            .mention_ac
            .selected
            .min(matches.len().saturating_sub(1));
        self.mention_ac.matches = matches;
    }

    pub fn ac_move_selection(&mut self, delta: isize) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let len = self.mention_ac.matches.len() as isize;
        let cur = self.mention_ac.selected as isize;
        self.mention_ac.selected = (cur + delta).clamp(0, len - 1) as usize;
    }

    pub fn ac_confirm(&mut self) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let username = self.mention_ac.matches[self.mention_ac.selected]
            .name
            .clone();
        let next = format!(
            "{}@{} ",
            &self.composer.text()[..self.mention_ac.trigger_offset],
            username
        );
        self.composer.set_text(next);
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn ac_dismiss(&mut self) {
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn general_messages(&self) -> &[ChatMessage] {
        let Some(general_id) = self.general_room_id else {
            return &[];
        };
        self.rooms
            .iter()
            .find(|(room, _)| room.id == general_id)
            .map(|(_, msgs)| msgs.as_slice())
            .unwrap_or(&[])
    }

    pub fn usernames(&self) -> &HashMap<Uuid, String> {
        &self.usernames
    }

    pub fn countries(&self) -> &HashMap<Uuid, String> {
        &self.countries
    }

    pub fn bonsai_glyphs(&self) -> &HashMap<Uuid, String> {
        &self.bonsai_glyphs
    }

    fn drain_snapshot(&mut self) {
        if !self.snapshot_rx.has_changed().unwrap_or(false) {
            return;
        }

        let snapshot = self.snapshot_rx.borrow_and_update().clone();
        if snapshot.user_id != Some(self.user_id) {
            return;
        }

        self.usernames = snapshot.usernames;
        self.countries = snapshot.countries;
        self.ignored_user_ids = snapshot.ignored_user_ids.into_iter().collect();
        self.rooms = self.merge_rooms(snapshot.chat_rooms);
        self.general_room_id = snapshot.general_room_id;
        self.unread_counts = self.merge_unread_counts(snapshot.unread_counts);
        self.all_usernames = snapshot.all_usernames;
        self.bonsai_glyphs = snapshot.bonsai_glyphs;
        self.sync_selection();
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                ChatEvent::MessageCreated {
                    message,
                    target_user_ids,
                } => {
                    let is_targeted = target_user_ids.is_some();
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    // Desktop notification queueing. target_user_ids is Some for
                    // DM/private rooms, None for public rooms. Don't notify on
                    // messages we authored ourselves.
                    if message.user_id != self.user_id {
                        let nickname = self
                            .usernames
                            .get(&message.user_id)
                            .cloned()
                            .unwrap_or_else(|| "someone".to_string());
                        let preview: String =
                            message.body.replace('\n', " ").chars().take(80).collect();

                        if is_targeted {
                            self.pending_notifications.push(PendingNotification {
                                kind: "dms",
                                title: format!("New DM from {nickname}"),
                                body: preview,
                            });
                        } else if let Some(me) = self.usernames.get(&self.user_id) {
                            let me_lc = me.to_ascii_lowercase();
                            if super::mentions::extract_mentions(&message.body)
                                .iter()
                                .any(|m| m == &me_lc)
                            {
                                self.pending_notifications.push(PendingNotification {
                                    kind: "mentions",
                                    title: format!("{nickname} mentioned you"),
                                    body: preview,
                                });
                            }
                        }
                    }
                    self.push_message(message);
                }
                ChatEvent::SendSucceeded {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::success("Message sent"));
                }
                ChatEvent::DeltaSynced {
                    user_id,
                    room_id,
                    messages,
                } if self.user_id == user_id => {
                    for message in messages {
                        if message.room_id == room_id {
                            self.push_message(message);
                        }
                    }
                }
                ChatEvent::SendFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::DmOpened { user_id, room_id } if self.user_id == user_id => {
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_dm_screen_switch = true;
                    banner = Some(Banner::success("DM opened"));
                }
                ChatEvent::DmFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomJoined {
                    user_id,
                    room_id,
                    slug,
                } if self.user_id == user_id => {
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_dm_screen_switch = true;
                    banner = Some(Banner::success(&format!("Joined #{slug}")));
                }
                ChatEvent::RoomFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomLeft { user_id, slug } if self.user_id == user_id => {
                    self.selected_room_id = None;
                    self.request_list();
                    banner = Some(Banner::success(&format!("Left #{slug}")));
                }
                ChatEvent::LeaveFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomCreated { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Created #{slug}")));
                }
                ChatEvent::RoomCreateFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::PermanentRoomCreated { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Created permanent #{slug}")));
                }
                ChatEvent::PermanentRoomDeleted { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Deleted permanent #{slug}")));
                }
                ChatEvent::AdminFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::MessageDeleted {
                    user_id,
                    room_id,
                    message_id,
                } => {
                    self.remove_message(room_id, message_id);
                    if self.user_id == user_id {
                        banner = Some(Banner::success("Message deleted"));
                    }
                }
                ChatEvent::MessageEdited {
                    message,
                    target_user_ids,
                } => {
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    self.replace_message(message);
                }
                ChatEvent::EditSucceeded {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::success("Message edited"));
                }
                ChatEvent::EditFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::DeleteFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::IgnoreListUpdated {
                    user_id,
                    ignored_user_ids,
                    message,
                } if self.user_id == user_id => {
                    self.ignored_user_ids = ignored_user_ids.into_iter().collect();
                    self.refilter_local_messages();
                    banner = Some(Banner::success(&message));
                }
                ChatEvent::IgnoreFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomMembersListed {
                    user_id,
                    title,
                    members,
                } if self.user_id == user_id => {
                    self.open_overlay(&title, members);
                }
                ChatEvent::RoomMembersListFailed { user_id, message }
                    if self.user_id == user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }
        banner
    }

    fn push_message(&mut self, message: ChatMessage) {
        let in_dm_room = self
            .rooms
            .iter()
            .any(|(room, _)| room.id == message.room_id && room.kind == "dm");

        if !in_dm_room && self.message_is_ignored(&message) {
            return;
        }

        let is_viewing_room = Some(message.room_id) == self.selected_room_id;

        let Some((_, messages)) = self
            .rooms
            .iter_mut()
            .find(|(room, _)| room.id == message.room_id)
        else {
            return;
        };

        if messages.iter().any(|existing| existing.id == message.id) {
            return;
        }

        // Service snapshots are newest-first; keep same order for cheap appends at the front.
        let room_id = message.room_id;
        messages.insert(0, message);
        if messages.len() > 1000 {
            messages.truncate(1000);
        }

        // Only mark the room as read if the user is actually viewing it.
        // Other warm rooms keep their unread badge until the user opens them.
        if is_viewing_room {
            self.unread_counts.insert(room_id, 0);
        }
    }

    fn remove_message(&mut self, room_id: Uuid, message_id: Uuid) {
        if let Some((_, messages)) = self.rooms.iter_mut().find(|(room, _)| room.id == room_id) {
            messages.retain(|m| m.id != message_id);
        }
    }

    fn replace_message(&mut self, message: ChatMessage) {
        if let Some((_, messages)) = self
            .rooms
            .iter_mut()
            .find(|(room, _)| room.id == message.room_id)
            && let Some(existing) = messages.iter_mut().find(|m| m.id == message.id)
        {
            *existing = message;
        }
    }

    fn merge_rooms(
        &self,
        incoming: Vec<(ChatRoom, Vec<ChatMessage>)>,
    ) -> Vec<(ChatRoom, Vec<ChatMessage>)> {
        let previous_by_room: HashMap<Uuid, &Vec<ChatMessage>> = self
            .rooms
            .iter()
            .map(|(room, msgs)| (room.id, msgs))
            .collect();

        incoming
            .into_iter()
            .map(|(room, messages)| {
                let messages = if messages.is_empty() {
                    previous_by_room
                        .get(&room.id)
                        .map(|previous| (*previous).clone())
                        .unwrap_or_default()
                } else {
                    messages
                };
                // DMs: don't filter. Users leave the DM room if they want it gone.
                let messages = if room.kind == "dm" {
                    messages
                } else {
                    self.filter_messages(messages)
                };
                (room, messages)
            })
            .collect()
    }

    fn merge_unread_counts(&mut self, mut incoming: HashMap<Uuid, i64>) -> HashMap<Uuid, i64> {
        self.pending_read_rooms
            .retain(|room_id| match incoming.get(room_id).copied() {
                Some(0) => false,
                Some(_) => {
                    incoming.insert(*room_id, 0);
                    true
                }
                None => true,
            });
        incoming
    }

    fn filter_messages(&self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .filter(|message| !self.message_is_ignored(message))
            .collect()
    }

    fn message_is_ignored(&self, message: &ChatMessage) -> bool {
        self.ignored_user_ids.contains(&message.user_id)
    }

    /// Strip already-stored messages from any newly-ignored author.
    /// DM rooms are exempt -leaving the DM room is the way to dismiss them.
    fn refilter_local_messages(&mut self) {
        let ignored = &self.ignored_user_ids;
        for (room, messages) in &mut self.rooms {
            if room.kind == "dm" {
                continue;
            }
            messages.retain(|m| !ignored.contains(&m.user_id));
        }
        self.sync_selection();
    }
}

/// Sort key for DMs: resolves the other participant's username.
/// Must match the sort used by the nav UI (`dm_label` in `ui.rs`).
fn dm_sort_key(room: &ChatRoom, user_id: Uuid, usernames: &HashMap<Uuid, String>) -> String {
    let other_id = if room.dm_user_a == Some(user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    };
    other_id
        .and_then(|id| usernames.get(&id))
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| "DM".to_string())
}

/// Parse `/dm @username` or `/dm username` from the composer text.
/// Returns the target username if the input matches.
fn parse_dm_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/dm ")?.trim_start();
    let username = rest.strip_prefix('@').unwrap_or(rest).trim();
    if username.is_empty() {
        return None;
    }
    Some(username)
}

/// Parse `/join #room` or `/join room` from the composer text.
/// Returns the room slug if the input matches.
fn parse_join_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/join ")?.trim_start();
    let room = rest.strip_prefix('#').unwrap_or(rest).trim();
    if room.is_empty() {
        return None;
    }
    Some(room)
}

/// Parse `/leave` from the composer text.
fn parse_leave_command(input: &str) -> bool {
    input.trim() == "/leave"
}

/// Parse `/create <slug>` or `/create #slug` from the composer text.
fn parse_create_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/create ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse `/create-room <slug>` from the composer text (admin only).
fn parse_create_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/create-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse `/delete-room <slug>` from the composer text (admin only).
fn parse_delete_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/delete-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

fn room_supports_member_list(room: &ChatRoom) -> bool {
    room.kind != "dm" && !room.auto_join
}

fn online_username_set(active_users: Option<&ActiveUsers>) -> HashSet<String> {
    let Some(active_users) = active_users else {
        return HashSet::new();
    };
    let guard = active_users.lock_recover();
    guard
        .values()
        .map(|u| u.username.to_ascii_lowercase())
        .collect()
}

pub(crate) fn rank_mention_matches(
    all_usernames: &[String],
    query_lower: &str,
    online_set: impl FnOnce() -> HashSet<String>,
) -> Vec<MentionMatch> {
    // Lowercase each candidate once and keep it paired with the original
    // display name; reused for the prefix filter, the online lookup, and the
    // alphabetical tie-breaker.
    let mut filtered: Vec<(String, String)> = all_usernames
        .iter()
        .filter_map(|name| {
            let lower = name.to_ascii_lowercase();
            lower
                .starts_with(query_lower)
                .then(|| (lower, name.clone()))
        })
        .collect();
    if filtered.is_empty() {
        return Vec::new();
    }

    let online = online_set();
    let mut matches: Vec<(String, MentionMatch)> = filtered
        .drain(..)
        .map(|(lower, name)| {
            let is_online = online.contains(&lower);
            (
                lower,
                MentionMatch {
                    name,
                    online: is_online,
                },
            )
        })
        .collect();
    matches.sort_by(|(a_lower, a), (b_lower, b)| {
        b.online.cmp(&a.online).then_with(|| a_lower.cmp(b_lower))
    });
    matches.into_iter().map(|(_, m)| m).collect()
}

fn format_active_user_lines(active_users: Option<&ActiveUsers>) -> Vec<String> {
    let Some(active_users) = active_users else {
        return vec!["Active user list unavailable".to_string()];
    };

    let guard = active_users.lock_recover();
    if guard.is_empty() {
        return vec!["No active users".to_string()];
    }

    let mut users: Vec<&ActiveUser> = guard.values().collect();
    users.sort_by_key(|user| user.username.to_ascii_lowercase());
    users
        .into_iter()
        .map(|user| {
            if user.connection_count > 1 {
                format!("@{} ({} sessions)", user.username, user.connection_count)
            } else {
                format!("@{}", user.username)
            }
        })
        .collect()
}

fn wrapped_index(current: isize, delta: isize, len: usize) -> usize {
    (current + delta).rem_euclid(len as isize) as usize
}

fn resolve_room_jump_target(targets: &[(u8, RoomSlot)], byte: u8) -> Option<RoomSlot> {
    let byte = byte.to_ascii_lowercase();
    targets
        .iter()
        .find_map(|(key, slot)| (*key == byte).then_some(*slot))
}

/// Parse `/<command>` or `/<command> [@]username`. Returns:
/// - `None` if `input` is not the given command,
/// - `Some(None)` for the bare command (caller treats as "list"),
/// - `Some(Some(username))` for the targeted form.
fn parse_user_command<'a>(input: &'a str, command: &str) -> Option<Option<&'a str>> {
    let rest = input.strip_prefix(command)?;
    let rest = match rest.chars().next() {
        None => return Some(None),
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        return Some(None);
    }
    let username = rest.strip_prefix('@').unwrap_or(rest).trim();
    Some((!username.is_empty()).then_some(username))
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

/// Given a message list containing `current`, return the id of the message
/// that should take over the selection when `current` is deleted: prefer the
/// next index (older message, since the list is ordered newest-first), fall
/// back to the previous index if `current` was the last item, or `None` if
/// `current` is not in the list.
fn adjacent_message_id(msgs: &[ChatMessage], current: Uuid) -> Option<Uuid> {
    let idx = msgs.iter().position(|m| m.id == current)?;
    msgs.get(idx + 1)
        .map(|m| m.id)
        .or_else(|| idx.checked_sub(1).and_then(|i| msgs.get(i).map(|m| m.id)))
}

fn reply_preview_text(body: &str) -> String {
    if let Some(title) = news_reply_preview_text(body) {
        return title;
    }

    let body_without_reply_quote = match body.split_once('\n') {
        Some((first_line, rest))
            if first_line.trim().starts_with("> ") && !rest.trim().is_empty() =>
        {
            rest
        }
        _ => body,
    };

    let first_content_line = body_without_reply_quote
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .unwrap_or("");
    let preview = first_content_line
        .strip_prefix("> ")
        .unwrap_or(first_content_line)
        .trim();
    let preview: String = preview.chars().take(48).collect();
    if preview.chars().count() == 48 {
        format!("{}...", preview.trim_end())
    } else {
        preview
    }
}

fn news_reply_preview_text(body: &str) -> Option<String> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with(NEWS_MARKER) {
        return None;
    }

    let raw = trimmed[NEWS_MARKER.len()..].trim_start();
    let title = raw
        .split(" || ")
        .next()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("news update");

    let preview: String = title.chars().take(48).collect();
    Some(if preview.chars().count() == 48 {
        format!("{}...", preview.trim_end())
    } else {
        preview
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(matches: &[MentionMatch]) -> Vec<&str> {
        matches.iter().map(|m| m.name.as_str()).collect()
    }

    fn online(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn rank_mention_matches_orders_online_before_offline() {
        let all = vec![
            "alice".to_string(),
            "bob".to_string(),
            "carol".to_string(),
            "dave".to_string(),
        ];
        let ranked = rank_mention_matches(&all, "", || online(&["bob", "dave"]));
        assert_eq!(names(&ranked), vec!["bob", "dave", "alice", "carol"]);
        assert!(ranked[0].online && ranked[1].online);
        assert!(!ranked[2].online && !ranked[3].online);
    }

    #[test]
    fn rank_mention_matches_prefix_filter_groups_online_first() {
        // "@a" with two online and one offline 'a'-prefixed users:
        // online 'a' names come first (alphabetically), then offline.
        let all = vec![
            "alice".to_string(),
            "alex".to_string(),
            "albert".to_string(),
            "bob".to_string(),
        ];
        let ranked = rank_mention_matches(&all, "a", || online(&["alice", "alex"]));
        assert_eq!(names(&ranked), vec!["alex", "alice", "albert"]);
        assert!(ranked[0].online && ranked[1].online);
        assert!(!ranked[2].online);
    }

    #[test]
    fn rank_mention_matches_applies_prefix_filter() {
        let all = vec!["alice".to_string(), "albert".to_string(), "bob".to_string()];
        let ranked = rank_mention_matches(&all, "al", || online(&["bob"]));
        assert_eq!(names(&ranked), vec!["albert", "alice"]);
    }

    #[test]
    fn rank_mention_matches_prefix_is_case_insensitive() {
        let all = vec!["Alice".to_string(), "alBert".to_string()];
        let ranked = rank_mention_matches(&all, "al", HashSet::new);
        assert_eq!(names(&ranked), vec!["alBert", "Alice"]);
    }

    #[test]
    fn rank_mention_matches_falls_back_to_alpha_when_no_online_info() {
        let all = vec!["zed".to_string(), "alice".to_string(), "bob".to_string()];
        let ranked = rank_mention_matches(&all, "", HashSet::new);
        assert_eq!(names(&ranked), vec!["alice", "bob", "zed"]);
        assert!(ranked.iter().all(|m| !m.online));
    }

    #[test]
    fn rank_mention_matches_skips_online_set_when_prefix_excludes_all() {
        // When the query filters everyone out, the online-set supplier must
        // not be invoked — it's the expensive path (locks ActiveUsers).
        let all = vec!["alice".to_string(), "bob".to_string()];
        let ranked = rank_mention_matches(&all, "zz", || {
            panic!("online_set should not be built when prefix filter is empty")
        });
        assert!(ranked.is_empty());
    }

    #[test]
    fn online_username_set_returns_empty_for_none() {
        assert!(online_username_set(None).is_empty());
    }

    #[test]
    fn online_username_set_lowercases_active_usernames() {
        use crate::state::ActiveUser;
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        let mut users: HashMap<Uuid, ActiveUser> = HashMap::new();
        users.insert(
            Uuid::now_v7(),
            ActiveUser {
                username: "Alice".to_string(),
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
        users.insert(
            Uuid::now_v7(),
            ActiveUser {
                username: "BOB".to_string(),
                connection_count: 2,
                last_login_at: Instant::now(),
            },
        );
        let active: ActiveUsers = Arc::new(Mutex::new(users));

        let set = online_username_set(Some(&active));
        assert_eq!(set, online(&["alice", "bob"]));
    }

    #[test]
    fn reply_preview_text_uses_message_body_for_nested_replies() {
        let preview = reply_preview_text("> @mat: original message preview\nyou like tetris?");
        assert_eq!(preview, "you like tetris?");
    }

    #[test]
    fn reply_preview_text_uses_news_title_for_news_messages() {
        let preview = reply_preview_text(
            "---NEWS--- Rust 1.95 Released || summary || https://example.com || ascii",
        );
        assert_eq!(preview, "Rust 1.95 Released");
    }

    #[test]
    fn news_marker_detection_matches_announcement_messages() {
        assert!(news_reply_preview_text("---NEWS--- title || summary || url || ascii").is_some());
        assert!(news_reply_preview_text("regular chat message").is_none());
    }

    // --- parse_dm_command ---

    #[test]
    fn parse_dm_with_at() {
        assert_eq!(parse_dm_command("/dm @alice"), Some("alice"));
    }

    #[test]
    fn parse_dm_without_at() {
        assert_eq!(parse_dm_command("/dm bob"), Some("bob"));
    }

    #[test]
    fn parse_dm_empty_username() {
        assert_eq!(parse_dm_command("/dm "), None);
        assert_eq!(parse_dm_command("/dm @"), None);
    }

    #[test]
    fn parse_dm_not_dm_command() {
        assert_eq!(parse_dm_command("hello world"), None);
        assert_eq!(parse_dm_command("/dms alice"), None);
    }

    #[test]
    fn parse_dm_trims_whitespace() {
        assert_eq!(parse_dm_command("/dm  @alice  "), Some("alice"));
    }

    #[test]
    fn wrapped_index_wraps_forward() {
        assert_eq!(wrapped_index(2, 1, 3), 0);
        assert_eq!(wrapped_index(1, 5, 3), 0);
    }

    #[test]
    fn wrapped_index_wraps_backward() {
        assert_eq!(wrapped_index(0, -1, 3), 2);
        assert_eq!(wrapped_index(1, -5, 3), 2);
    }

    #[test]
    fn resolve_room_jump_target_is_case_insensitive() {
        let room_id = Uuid::from_u128(7);
        let targets = [
            (b'a', RoomSlot::Room(room_id)),
            (b's', RoomSlot::News),
            (b'd', RoomSlot::Notifications),
        ];

        assert_eq!(
            resolve_room_jump_target(&targets, b'A'),
            Some(RoomSlot::Room(room_id))
        );
        assert_eq!(
            resolve_room_jump_target(&targets, b's'),
            Some(RoomSlot::News)
        );
        assert_eq!(
            resolve_room_jump_target(&targets, b'D'),
            Some(RoomSlot::Notifications)
        );
        assert_eq!(resolve_room_jump_target(&targets, b'x'), None);
    }

    #[test]
    fn parse_user_command_with_username() {
        assert_eq!(
            parse_user_command("/ignore @alice", "/ignore"),
            Some(Some("alice"))
        );
        assert_eq!(
            parse_user_command("/unignore bob", "/unignore"),
            Some(Some("bob"))
        );
    }

    #[test]
    fn parse_user_command_lists_when_username_missing() {
        assert_eq!(parse_user_command("/ignore", "/ignore"), Some(None));
        assert_eq!(parse_user_command("/ignore   ", "/ignore"), Some(None));
        assert_eq!(parse_user_command("/ignore @", "/ignore"), Some(None));
        assert_eq!(parse_user_command("/unignore", "/unignore"), Some(None));
    }

    #[test]
    fn parse_user_command_rejects_non_matches() {
        assert_eq!(parse_user_command("ignore alice", "/ignore"), None);
        assert_eq!(parse_user_command("/ignored alice", "/ignore"), None);
        assert_eq!(parse_user_command("/unignored alice", "/unignore"), None);
    }

    #[test]
    fn parse_create_room_with_hash() {
        assert_eq!(
            parse_create_room_command("/create-room #announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_create_room_without_hash() {
        assert_eq!(
            parse_create_room_command("/create-room announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_create_room_empty() {
        assert_eq!(parse_create_room_command("/create-room "), None);
        assert_eq!(parse_create_room_command("/create-room #"), None);
    }

    #[test]
    fn parse_create_room_not_command() {
        assert_eq!(parse_create_room_command("hello"), None);
        assert_eq!(parse_create_room_command("/create-rooms foo"), None);
    }

    #[test]
    fn parse_delete_room_with_hash() {
        assert_eq!(
            parse_delete_room_command("/delete-room #announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_delete_room_without_hash() {
        assert_eq!(
            parse_delete_room_command("/delete-room announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_delete_room_empty() {
        assert_eq!(parse_delete_room_command("/delete-room "), None);
    }

    #[test]
    fn parse_delete_room_not_command() {
        assert_eq!(parse_delete_room_command("hello"), None);
    }

    #[test]
    fn room_supports_member_list_only_for_non_auto_join_non_dm_rooms() {
        let private_room = ChatRoom {
            id: Uuid::now_v7(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            kind: "topic".to_string(),
            visibility: "public".to_string(),
            auto_join: false,
            permanent: false,
            slug: Some("side".to_string()),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        assert!(room_supports_member_list(&private_room));

        let public_room = ChatRoom {
            auto_join: true,
            ..private_room.clone()
        };
        assert!(!room_supports_member_list(&public_room));

        let dm_room = ChatRoom {
            kind: "dm".to_string(),
            visibility: "dm".to_string(),
            ..private_room
        };
        assert!(!room_supports_member_list(&dm_room));
    }

    #[test]
    fn format_active_user_lines_sorts_and_shows_session_counts() {
        let active_users = std::sync::Arc::new(std::sync::Mutex::new(HashMap::from([
            (
                Uuid::now_v7(),
                ActiveUser {
                    username: "zoe".to_string(),
                    connection_count: 2,
                    last_login_at: std::time::Instant::now(),
                },
            ),
            (
                Uuid::now_v7(),
                ActiveUser {
                    username: "alice".to_string(),
                    connection_count: 1,
                    last_login_at: std::time::Instant::now(),
                },
            ),
        ])));

        assert_eq!(
            format_active_user_lines(Some(&active_users)),
            vec!["@alice".to_string(), "@zoe (2 sessions)".to_string()]
        );
    }

    #[test]
    fn format_active_user_lines_handles_missing_registry() {
        assert_eq!(
            format_active_user_lines(None),
            vec!["Active user list unavailable".to_string()]
        );
    }

    // --- adjacent_message_id (delete-and-advance) ---

    fn make_msg(id: Uuid) -> ChatMessage {
        ChatMessage {
            id,
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            room_id: Uuid::from_u128(999),
            user_id: Uuid::from_u128(999),
            body: String::new(),
        }
    }

    #[test]
    fn adjacent_message_id_returns_none_for_empty_list() {
        assert_eq!(adjacent_message_id(&[], Uuid::from_u128(1)), None);
    }

    #[test]
    fn adjacent_message_id_returns_none_when_not_in_list() {
        let msgs = vec![make_msg(Uuid::from_u128(1))];
        assert_eq!(adjacent_message_id(&msgs, Uuid::from_u128(99)), None);
    }

    #[test]
    fn adjacent_message_id_prefers_next_index_older_message() {
        // List is newest-first: [0]=newest, [1]=middle, [2]=oldest.
        // Deleting the middle should land on the oldest (idx+1).
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        let msgs = vec![make_msg(a), make_msg(b), make_msg(c)];
        assert_eq!(adjacent_message_id(&msgs, b), Some(c));
    }

    #[test]
    fn adjacent_message_id_falls_back_to_previous_for_last_item() {
        // Deleting the oldest (last index) should land on the previous-older
        // message (idx-1), i.e., the next-oldest remaining.
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        let msgs = vec![make_msg(a), make_msg(b), make_msg(c)];
        assert_eq!(adjacent_message_id(&msgs, c), Some(b));
    }

    #[test]
    fn adjacent_message_id_returns_none_for_sole_item() {
        let a = Uuid::from_u128(1);
        let msgs = vec![make_msg(a)];
        assert_eq!(adjacent_message_id(&msgs, a), None);
    }

    // --- dm_sort_key (regression: nav order must match UI order) ---

    fn make_dm(user_a: Uuid, user_b: Uuid) -> ChatRoom {
        ChatRoom {
            id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            kind: "dm".to_string(),
            visibility: "dm".to_string(),
            auto_join: false,
            permanent: false,
            slug: None,
            language_code: None,
            dm_user_a: Some(user_a),
            dm_user_b: Some(user_b),
        }
    }

    #[test]
    fn dm_sort_key_resolves_other_users_name() {
        let me = Uuid::from_u128(1);
        let alice = Uuid::from_u128(2);
        let bob = Uuid::from_u128(3);

        let mut usernames = HashMap::new();
        usernames.insert(me, "me".to_string());
        usernames.insert(alice, "alice".to_string());
        usernames.insert(bob, "bob".to_string());

        let room = make_dm(me, alice);
        assert_eq!(dm_sort_key(&room, me, &usernames), "@alice");

        // Works regardless of which slot I'm in
        let room = make_dm(bob, me);
        assert_eq!(dm_sort_key(&room, me, &usernames), "@bob");
    }

    #[test]
    fn dm_sort_key_orders_alphabetically_by_display_name() {
        let me = Uuid::from_u128(1);
        let alice = Uuid::from_u128(2);
        let charlie = Uuid::from_u128(3);
        let bob = Uuid::from_u128(4);

        let mut usernames = HashMap::new();
        usernames.insert(alice, "alice".to_string());
        usernames.insert(charlie, "charlie".to_string());
        usernames.insert(bob, "bob".to_string());

        let mut dms = [make_dm(me, charlie), make_dm(me, alice), make_dm(bob, me)];
        dms.sort_by_key(|r| dm_sort_key(r, me, &usernames));

        let names: Vec<_> = dms.iter().map(|r| dm_sort_key(r, me, &usernames)).collect();
        assert_eq!(names, vec!["@alice", "@bob", "@charlie"]);
    }
}
