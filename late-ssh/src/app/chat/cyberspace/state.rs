//! Per-session UI state for the cyberspace pane and its modals.
//!
//! Everything shown here was fetched by this user's own linked account and
//! lives only in this session's memory: cyberspace content is never cached
//! server-side or shown to anyone but the user who fetched it.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};
use crate::app::common::primitives::Banner;

use super::api::{
    CircMessage, CircRoom, CircStreamEvent, CsNotification, CsPost, NewPost, UNREAD_PROBE_LIMIT,
};
use super::svc::{CircRoomSession, CsEvent, CsThread, CyberspaceService};

pub(crate) const TITLE_MAX_CHARS: usize = 100;
pub(crate) const TOPICS_MAX_CHARS: usize = 80;
pub(crate) const BODY_MAX_CHARS: usize = 32_768;
const MAX_TOPICS: usize = 3;
/// How often a linked session re-fetches the unread badge. Everything else in
/// the pane is user-driven, but the rail badge has to notice a reply that
/// landed while the user was reading something else. RSS polls every 30
/// minutes from one global task; this one rides the session tick instead,
/// because the count is per user and needs that user's own token.
const UNREAD_POLL_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// How stale the feed has to be before *entering* the pane refetches it.
/// Entering happens more often than it looks: cycling the room rail lands on
/// the slot, and every landing would otherwise be an authenticated call to a
/// third party under the user's own token, which is the traffic shape their
/// anti-bot terms are about. `r` is the explicit refresh and ignores this.
const FEED_RELOAD_INTERVAL: Duration = Duration::from_secs(30);
/// Their cap on one chat message.
pub(crate) const CIRC_MESSAGE_MAX_CHARS: usize = 2_048;
/// How much of a room's conversation one session keeps. Their live window is
/// 50 and history pages 50 at a time; this bounds a long sitting without
/// truncating the scrollback anyone actually reads.
const CIRC_MESSAGE_CAP: usize = 300;

/// Outcome of one pane tick, mirroring `FeedsTick`.
pub struct CsTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LinkStatus {
    Unknown,
    Unlinked,
    Linked { username: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    Feed,
    Thread,
    Notifications,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LinkField {
    Email,
    Password,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ComposeField {
    Title,
    Topics,
    Body,
}

pub(crate) struct LinkModal {
    pub email: TextArea<'static>,
    pub password: TextArea<'static>,
    pub focus: LinkField,
    pub error: Option<String>,
    pub busy: bool,
}

pub(crate) struct ComposeModal {
    pub title: TextArea<'static>,
    pub topics: TextArea<'static>,
    pub body: TextArea<'static>,
    pub focus: ComposeField,
    pub error: Option<String>,
    pub busy: bool,
}

/// The room picker: their whole roster, with the rooms already on the rail
/// marked. Adding one is what creates its rail entry; nothing here opens a
/// room, since the rail entry is how a room is entered afterwards.
pub(crate) struct RoomsModal {
    pub roster: Vec<CircRoom>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
}

pub(crate) struct ReplyModal {
    pub post: CsPost,
    pub body: TextArea<'static>,
    pub error: Option<String>,
    pub busy: bool,
}

/// Boxed variants: each modal carries its own `TextArea`s, so the inline enum
/// would be ~2KB living in every session's pane state whether a modal is open
/// or not.
pub(crate) enum Modal {
    Link(Box<LinkModal>),
    Compose(Box<ComposeModal>),
    Reply(Box<ReplyModal>),
    Rooms(Box<RoomsModal>),
}

/// A room the user is currently inside. Its `session` is what makes it fetch:
/// history, the live stream, and the presence heartbeat all hang off it and
/// all stop when it drops, which is why leaving a room is simply dropping this.
pub(crate) struct OpenRoom {
    pub slug: String,
    pub messages: Vec<CircMessage>,
    pub loading: bool,
    /// `None` while reading; `Some` once the user starts writing.
    pub composer: Option<TextArea<'static>>,
    /// Rendered rows scrolled back from the newest. 0 is the live bottom.
    pub scroll: usize,
    /// How far back the conversation can scroll, written by the renderer once
    /// it knows how many rows the messages wrapped to. Counting messages
    /// instead would stop `k` short of the top of a room full of long lines.
    pub max_scroll: Cell<usize>,
    /// Their stream gave up. Reading still works, the room is just no longer
    /// live, and the user is told rather than left staring at a frozen room.
    pub stream_down: bool,
    session: CircRoomSession,
}

pub struct State {
    service: CyberspaceService,
    user_id: Uuid,
    event_rx: broadcast::Receiver<CsEvent>,
    /// Rooms pinned into the rail, in the user's order. Their API has no join
    /// or leave, so this list is ours: a bookmark, not their state.
    pub(crate) pinned: Vec<String>,
    /// Per-room read cursors: slug -> newest message timestamp seen while the
    /// user was inside (their clock, epoch ms). Seeded from the account row
    /// at session init, advanced locally as rooms are read, persisted
    /// fire-and-forget through `mark_circ_room_read_task`.
    room_reads: HashMap<String, i64>,
    /// The roster's `last_message_at` per room, refreshed by the 10-minute
    /// badge poll. Compared against `room_reads` for the rail's unread dots;
    /// same clock on both sides, so skew never enters into it.
    room_last_message: HashMap<String, i64>,
    pub(crate) open_room: Option<OpenRoom>,
    pub(crate) link: LinkStatus,
    pub(crate) view: View,
    pub(crate) posts: Vec<CsPost>,
    pub(crate) selected: usize,
    pub(crate) thread: Option<CsThread>,
    /// The post the thread view is currently for. A load that arrives for
    /// anything else is stale and gets dropped, which is what stops a slow
    /// fetch from yanking a thread the user has already left. Set before the
    /// post itself exists, since a notification opens a thread by id alone.
    pub(crate) thread_target: Option<String>,
    pub(crate) thread_scroll: usize,
    /// Last row the thread view can scroll to, written by the renderer after
    /// it lays the entry out: only it knows the viewport and how the text
    /// wrapped, and an estimate here is either short (the tail of a long entry
    /// becomes unreachable) or long (`j` runs off into blank space).
    pub(crate) thread_max_scroll: Cell<usize>,
    pub(crate) notifications: Vec<CsNotification>,
    pub(crate) notif_selected: usize,
    unread_notifications: i64,
    unread_entries: i64,
    /// Entries published after this are unread. Advances when a feed the user
    /// asked to read arrives, to the newest entry on that page. `None` (never
    /// visited) reads as nothing unread, not as a whole page of it.
    feed_read_at: Option<DateTime<Utc>>,
    /// Set by the two reads the user asks for (entering the pane, `r`) and
    /// consumed by the `FeedLoaded` that answers them, which is the moment the
    /// entries are actually on screen and the cursor may move. Loads fired for
    /// other reasons (publishing an entry) leave it unset and mark nothing.
    mark_read_on_load: bool,
    /// What the `●` row markers compare against: the cursor as it was when
    /// this visit started. Frozen for the visit, so entering the pane does not
    /// wipe the marks off the very entries the user came to read.
    feed_marker_at: Option<DateTime<Utc>>,
    last_unread_poll: Instant,
    /// `None` until the first feed load of this session.
    last_feed_load: Option<Instant>,
    pub(crate) loading: bool,
    pub(crate) modal: Option<Modal>,
}

impl State {
    pub fn new(service: CyberspaceService, user_id: Uuid) -> Self {
        let event_rx = service.subscribe_events();
        service.session_init_task(user_id);
        Self {
            service,
            user_id,
            event_rx,
            pinned: Vec::new(),
            room_reads: HashMap::new(),
            room_last_message: HashMap::new(),
            open_room: None,
            link: LinkStatus::Unknown,
            view: View::Feed,
            posts: Vec::new(),
            selected: 0,
            thread: None,
            thread_target: None,
            thread_scroll: 0,
            thread_max_scroll: Cell::new(0),
            notifications: Vec::new(),
            notif_selected: 0,
            unread_notifications: 0,
            unread_entries: 0,
            feed_read_at: None,
            mark_read_on_load: false,
            feed_marker_at: None,
            // `session_init_task` above fetches the count for a linked user,
            // so the interval starts running from session start.
            last_unread_poll: Instant::now(),
            last_feed_load: None,
            loading: false,
            modal: None,
        }
    }

    /// The rail badge: notifications plus new entries, one number for
    /// "cyberspace has this much waiting for you". The pane header splits it
    /// back apart, since the two open with different keys.
    pub fn unread_count(&self) -> i64 {
        self.unread_notifications + self.unread_entries
    }

    pub(crate) fn unread_notifications(&self) -> i64 {
        self.unread_notifications
    }

    pub(crate) fn unread_entries(&self) -> i64 {
        self.unread_entries
    }

    /// Whether the unread count is a floor rather than a number. The probe
    /// page is `UNREAD_PROBE_LIMIT` entries, so a full one means "at least
    /// this many": the badge has to say so instead of naming a count it
    /// cannot stand behind.
    pub fn unread_saturated(&self) -> bool {
        self.unread_entries >= i64::from(UNREAD_PROBE_LIMIT)
    }

    /// Whether a feed row gets the new-entry mark, against the cursor as it
    /// stood when this visit started.
    pub(crate) fn is_unread_entry(&self, post: &CsPost) -> bool {
        is_newer_than(post, self.feed_marker_at)
    }

    pub(crate) fn is_linked(&self) -> bool {
        matches!(self.link, LinkStatus::Linked { .. })
    }

    /// Known to be unlinked, as opposed to [`LinkStatus::Unknown`] where the
    /// session-init answer has not landed yet. The shell uses this to drop a
    /// pane the rail has stopped listing, so it must not fire on "not sure".
    pub(crate) fn is_unlinked(&self) -> bool {
        matches!(self.link, LinkStatus::Unlinked)
    }

    pub fn modal_active(&self) -> bool {
        self.modal.is_some()
    }

    /// Entering the pane (rail selection or `/cs`): a fresh feed for linked
    /// users, rate-limited by [`FEED_RELOAD_INTERVAL`], and nothing at all for
    /// unlinked ones (they never reach the pane).
    ///
    /// The pane always opens on the newest entry. A selection kept from the
    /// last visit points into a feed that has since been refetched, so it
    /// lands on whatever entry happens to sit at that row now.
    pub fn opened(&mut self) {
        self.back_to_feed();
        self.selected = 0;
        self.notif_selected = 0;
        // Freeze the marks for this visit before the cursor moves past them.
        self.feed_marker_at = self.feed_read_at;
        if feed_reload_due(
            self.is_linked(),
            self.loading,
            self.last_feed_load.map(|at| at.elapsed()),
        ) {
            self.mark_read_on_load = true;
            self.load_feed();
        }
    }

    /// `r`: the user asking for the feed, so no interval applies.
    pub(crate) fn refresh(&mut self) {
        if self.is_linked() {
            self.mark_read_on_load = true;
            self.load_feed();
        }
    }

    /// Everything the arrived feed shows counts as read. The cursor lands on
    /// the newest entry actually on screen, not on the clock: a stamp of "now"
    /// would swallow entries published since the last fetch that the reload
    /// interval kept off this page, and entries left unfetched by a failed
    /// load. The cursor never moves backwards, so a quiet feed whose newest
    /// entry the cursor already covers writes nothing.
    ///
    /// Only a load the user asked for lands here (entering the pane, `r`),
    /// never a `FeedLoaded` on its own: publishing an entry from another room
    /// also loads the feed, and that is not the user reading it.
    fn mark_feed_read(&mut self) {
        let Some(newest) = self.posts.iter().filter_map(|post| post.created_at).max() else {
            return;
        };
        if self.feed_read_at.is_some_and(|cursor| cursor >= newest) {
            return;
        }
        self.feed_read_at = Some(newest);
        self.unread_entries = 0;
        self.service.mark_feed_read_task(self.user_id, newest);
    }

    fn load_feed(&mut self) {
        self.last_feed_load = Some(Instant::now());
        self.loading = true;
        self.service.load_feed_task(self.user_id);
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        match self.view {
            View::Feed => {
                self.selected = step_index(self.selected, delta, self.posts.len());
            }
            View::Thread => {
                self.thread_scroll = self
                    .thread_scroll
                    .saturating_add_signed(delta)
                    .min(self.thread_max_scroll.get());
            }
            View::Notifications => {
                self.notif_selected =
                    step_index(self.notif_selected, delta, self.notifications.len());
            }
        }
    }

    /// `g`: back to the top of whatever the current view is a list of.
    pub(crate) fn go_to_top(&mut self) {
        match self.view {
            View::Feed => self.selected = 0,
            View::Thread => self.thread_scroll = 0,
            View::Notifications => self.notif_selected = 0,
        }
    }

    pub(crate) fn open_selected_thread(&mut self) {
        let Some(post) = self.posts.get(self.selected).cloned() else {
            return;
        };
        self.thread_target = Some(post.post_id.clone());
        self.thread = Some(CsThread {
            post: post.clone(),
            replies: Vec::new(),
        });
        self.reset_thread_scroll();
        self.view = View::Thread;
        self.loading = true;
        self.service.load_thread_task(self.user_id, post);
    }

    /// Enter on a notification opens the entry it is about. The post is
    /// fetched by id rather than looked up locally: the entry someone replied
    /// to is usually older than the feed page in memory.
    pub(crate) fn open_selected_notification(&mut self) -> Option<Banner> {
        let notification = self.notifications.get(self.notif_selected)?;
        let Some(post_id) = notification.post_id() else {
            return Some(Banner::error("That notification has no entry to open."));
        };
        let post_id = post_id.to_string();
        self.thread_target = Some(post_id.clone());
        // No placeholder to show: unlike the feed path, nothing here knows
        // the post yet, so the thread view renders its loading state.
        self.thread = None;
        self.reset_thread_scroll();
        self.view = View::Thread;
        self.loading = true;
        self.service.load_thread_by_id_task(self.user_id, post_id);
        None
    }

    pub(crate) fn open_notifications(&mut self) {
        if !self.is_linked() {
            return;
        }
        self.view = View::Notifications;
        self.notif_selected = 0;
        self.loading = true;
        self.service.load_notifications_task(self.user_id);
    }

    /// Esc from a sub-view goes back to the feed, matching the `b back` the
    /// footer advertises. Reports whether it acted, so the shell's escape
    /// chain keeps looking when the pane has nothing to close.
    pub(crate) fn escape_to_feed(&mut self) -> bool {
        if self.view == View::Feed {
            return false;
        }
        self.back_to_feed();
        true
    }

    pub(crate) fn back_to_feed(&mut self) {
        self.view = View::Feed;
        self.thread = None;
        self.thread_target = None;
        self.reset_thread_scroll();
    }

    /// The ceiling belongs to the entry that was on screen, so it goes with it.
    /// The renderer refills it on the first frame of the next entry, which
    /// always precedes the keystroke that could scroll one.
    fn reset_thread_scroll(&mut self) {
        self.thread_scroll = 0;
        self.thread_max_scroll.set(0);
    }

    pub fn open_link_modal(&mut self) {
        let mut email = new_themed_textarea("you@example.com", WrapMode::None, true);
        set_themed_textarea_cursor_visible(&mut email, true);
        let mut password = new_themed_textarea("password", WrapMode::None, false);
        password.set_mask_char('•');
        self.modal = Some(Modal::Link(Box::new(LinkModal {
            email,
            password,
            focus: LinkField::Email,
            error: None,
            busy: false,
        })));
    }

    pub fn open_compose_modal(&mut self) -> Option<Banner> {
        if !self.is_linked() {
            return Some(Banner::error(
                "Link your cyberspace account first: /cs link",
            ));
        }
        self.modal = Some(Modal::Compose(Box::new(ComposeModal {
            title: new_themed_textarea("Title (optional)", WrapMode::None, true),
            topics: new_themed_textarea("Topics, up to 3 (optional)", WrapMode::None, false),
            body: new_themed_textarea("Write your entry (markdown)...", WrapMode::Word, false),
            focus: ComposeField::Title,
            error: None,
            busy: false,
        })));
        None
    }

    pub(crate) fn open_reply_modal(&mut self) {
        let Some(post) = self.current_thread_post() else {
            return;
        };
        self.modal = Some(Modal::Reply(Box::new(ReplyModal {
            post,
            body: new_themed_textarea("Write your reply (markdown)...", WrapMode::Word, true),
            error: None,
            busy: false,
        })));
    }

    fn current_thread_post(&self) -> Option<CsPost> {
        match self.view {
            View::Thread => self.thread.as_ref().map(|thread| thread.post.clone()),
            View::Feed => self.posts.get(self.selected).cloned(),
            View::Notifications => None,
        }
    }

    pub(crate) fn close_modal(&mut self) {
        self.modal = None;
    }

    // --- their chat rooms ---------------------------------------------------

    pub fn pinned_rooms(&self) -> &[String] {
        &self.pinned
    }

    pub(crate) fn open_room_slug(&self) -> Option<&str> {
        self.open_room.as_ref().map(|room| room.slug.as_str())
    }

    /// `/cs chat`: the room picker. Their roster is fetched here and nowhere
    /// else, on demand and never on a timer, because a human asked for it.
    pub(crate) fn open_rooms_modal(&mut self) -> Option<Banner> {
        if !self.is_linked() {
            return Some(Banner::error(
                "Link your cyberspace account first: /cs link",
            ));
        }
        self.modal = Some(Modal::Rooms(Box::new(RoomsModal {
            roster: Vec::new(),
            selected: 0,
            loading: true,
            error: None,
        })));
        self.service.load_circ_rooms_task(self.user_id);
        None
    }

    /// Move the picker's selection. Its own list, so it does not share the
    /// pane's view-based movement.
    pub(crate) fn move_rooms_modal_selection(&mut self, delta: isize) {
        if let Some(Modal::Rooms(rooms)) = &mut self.modal {
            rooms.selected = step_index(rooms.selected, delta, rooms.roster.len());
        }
    }

    /// Add the highlighted room to the rail, or take it off again. This is
    /// the only way a chat room becomes a rail entry.
    pub(crate) fn toggle_selected_room(&mut self) -> Option<Banner> {
        let Some(Modal::Rooms(rooms)) = &self.modal else {
            return None;
        };
        let slug = rooms.roster.get(rooms.selected)?.key().to_string();
        let banner = match self.pinned.iter().position(|pinned| *pinned == slug) {
            Some(index) => {
                self.pinned.remove(index);
                Banner::success(&format!("Removed #{slug} from your rail."))
            }
            None => {
                self.pinned.push(slug.clone());
                Banner::success(&format!("Added #{slug} to your rail."))
            }
        };
        self.service
            .set_circ_pinned_task(self.user_id, self.pinned.clone());
        Some(banner)
    }

    /// Enter a room: everything it fetches hangs off the session held here, so
    /// a room nobody is looking at fetches nothing. Re-entering the room
    /// already open is a no-op rather than a reconnect.
    pub fn enter_room(&mut self, slug: String) {
        if self.open_room_slug() == Some(slug.as_str()) {
            return;
        }
        // Leaving the previous room closes its stream, announces the user out
        // of it, and stamps its read cursor before the new one opens.
        self.leave_room();
        let session = self.service.open_circ_room(self.user_id, slug.clone());
        self.open_room = Some(OpenRoom {
            slug,
            messages: Vec::new(),
            loading: true,
            composer: None,
            scroll: 0,
            max_scroll: Cell::new(0),
            stream_down: false,
            session,
        });
        // Walking in is what clears the dot, before a single message has
        // loaded. Waiting for history would leave the mark up on exactly the
        // rooms whose history did not arrive.
        self.stamp_open_room_read();
    }

    /// Leaving the room surface for anything else. Dropping the session is
    /// what stops the stream, the heartbeat, and any further fetching; the
    /// read cursor stamps on the way out, so what was on screen stays read.
    pub fn leave_room(&mut self) {
        self.stamp_open_room_read();
        self.open_room = None;
    }

    /// Move the open room's read cursor forward. Runs on entering, when
    /// history lands, and on leaving: entering a room always clears its dot,
    /// which is the whole contract of the mark. Only a cursor that actually
    /// advances is persisted, so re-visiting a quiet room writes nothing.
    fn stamp_open_room_read(&mut self) {
        let Some(room) = &self.open_room else {
            return;
        };
        let slug = room.slug.clone();
        let newest_message = room.messages.iter().map(|message| message.timestamp).max();
        // The roster's own stamp is the floor. Reading the messages is not
        // always possible (their history call can fail, the room can be
        // empty, the page can carry nothing stampable), but being in the room
        // is having seen it either way, and a dot the user cannot clear by
        // walking in is worse than no dot at all. It also keeps the
        // comparison like for like: the dot comes from `last_message_at`, so
        // acknowledging that same value can never drift against it.
        let Some(newest) = newest_message
            .into_iter()
            .chain(self.room_last_message.get(&slug).copied())
            .max()
        else {
            return;
        };
        let known = self.room_reads.get(&slug).copied().unwrap_or(i64::MIN);
        if newest <= known {
            return;
        }
        self.room_reads.insert(slug.clone(), newest);
        self.service
            .mark_circ_room_read_task(self.user_id, slug, newest);
    }

    /// One flag per pinned room, aligned with `pinned_rooms`: does the rail
    /// row get an unread dot? The open room never does (being in it is
    /// reading it), and a room never visited shows nothing rather than
    /// claiming unread history the user was never behind on.
    pub(crate) fn room_unread_flags(&self) -> Vec<bool> {
        self.pinned
            .iter()
            .map(|slug| {
                if self.open_room_slug() == Some(slug.as_str()) {
                    return false;
                }
                match (self.room_last_message.get(slug), self.room_reads.get(slug)) {
                    (Some(last_message), Some(read)) => last_message > read,
                    _ => false,
                }
            })
            .collect()
    }

    /// The user did something in the open room, which is what keeps them from
    /// showing as idle in the room's user list on their side.
    fn note_room_activity(&self) {
        if let Some(room) = &self.open_room {
            room.session.note_activity();
        }
    }

    pub(crate) fn room_scroll(&mut self, delta: isize) {
        let Some(room) = &mut self.open_room else {
            return;
        };
        // Scroll counts rendered rows back from the newest, so up means older.
        // The ceiling comes from the renderer, which is the only thing that
        // knows how many rows the conversation wrapped to.
        let ceiling = room.max_scroll.get();
        room.scroll = room.scroll.saturating_add_signed(-delta).min(ceiling);
    }

    /// `g` in a room jumps back to the live bottom.
    pub(crate) fn room_to_bottom(&mut self) {
        if let Some(room) = &mut self.open_room {
            room.scroll = 0;
        }
    }

    pub(crate) fn start_room_composer(&mut self) {
        let Some(room) = &mut self.open_room else {
            return;
        };
        if room.composer.is_none() {
            // One line: it draws in the chat composer slot, which is a single
            // row, and their cap is one message rather than a document. No
            // placeholder here: `chat::ui` draws the empty state itself so the
            // cursor sits on the hint's first character instead of before it.
            room.composer = Some(new_themed_textarea("", WrapMode::None, true));
        }
        self.note_room_activity();
    }

    pub(crate) fn room_composer_mut(&mut self) -> Option<&mut TextArea<'static>> {
        self.open_room.as_mut()?.composer.as_mut()
    }

    /// The open room's composer for rendering. It draws in the chat composer
    /// slot at the bottom of the screen, not inside the pane, so a room has
    /// one input in the place every other room's input lives.
    pub(crate) fn room_composer(&self) -> Option<&TextArea<'static>> {
        self.open_room.as_ref()?.composer.as_ref()
    }

    /// Typing counts as activity, which is what keeps the user from showing
    /// as idle to everyone else in the room.
    pub(crate) fn note_composer_activity(&self) {
        self.note_room_activity();
    }

    pub(crate) fn cancel_room_composer(&mut self) -> bool {
        match &mut self.open_room {
            Some(room) if room.composer.is_some() => {
                room.composer = None;
                true
            }
            _ => false,
        }
    }

    /// Send what is in the room composer. Nothing is echoed locally: the
    /// message arrives through the room's own stream like everyone else's, so
    /// there is no provisional row to reconcile or leave behind on failure.
    pub(crate) fn submit_room_composer(&mut self) -> Option<Banner> {
        let Some(room) = &mut self.open_room else {
            return None;
        };
        let composer = room.composer.as_ref()?;
        let content = single_line(composer);
        if content.is_empty() {
            room.composer = None;
            return None;
        }
        if content.chars().count() > CIRC_MESSAGE_MAX_CHARS {
            return Some(Banner::error(&format!(
                "Cyberspace messages are capped at {CIRC_MESSAGE_MAX_CHARS} characters."
            )));
        }
        let slug = room.slug.clone();
        room.composer = None;
        room.scroll = 0;
        self.note_room_activity();
        self.service
            .send_circ_message_task(self.user_id, slug, content);
        None
    }

    pub(crate) fn is_pinned(&self, slug: &str) -> bool {
        self.pinned.iter().any(|pinned| pinned == slug)
    }

    /// Submit whichever modal is open. Validation happens here (the boundary);
    /// the modal stays open and busy until the service answers, so a failed
    /// publish never eats the draft.
    pub(crate) fn submit_modal(&mut self) {
        match &mut self.modal {
            Some(Modal::Link(link)) => {
                let email = single_line(&link.email);
                let password = link.password.lines().join("");
                if email.is_empty() || password.is_empty() {
                    link.error = Some("email and password are both required".to_string());
                    return;
                }
                link.error = None;
                link.busy = true;
                self.service.link_task(self.user_id, email, password);
            }
            Some(Modal::Compose(compose)) => {
                let title = single_line(&compose.title);
                let body = compose.body.lines().join("\n").trim().to_string();
                let topics = match parse_topics(&single_line(&compose.topics)) {
                    Ok(topics) => topics,
                    Err(error) => {
                        compose.error = Some(error);
                        return;
                    }
                };
                if body.is_empty() {
                    compose.error = Some("the entry needs a body".to_string());
                    return;
                }
                compose.error = None;
                compose.busy = true;
                self.service.post_task(
                    self.user_id,
                    NewPost {
                        content: body,
                        title: (!title.is_empty()).then_some(title),
                        topics,
                    },
                );
            }
            Some(Modal::Reply(reply)) => {
                let body = reply.body.lines().join("\n").trim().to_string();
                if body.is_empty() {
                    reply.error = Some("the reply needs a body".to_string());
                    return;
                }
                reply.error = None;
                reply.busy = true;
                self.service
                    .reply_task(self.user_id, reply.post.clone(), body);
            }
            // The picker has nothing to submit: toggling a room is the whole
            // interaction, and it takes effect as it is pressed.
            Some(Modal::Rooms(_)) | None => {}
        }
    }

    pub(crate) fn unlink(&mut self) {
        self.service.unlink_task(self.user_id);
    }

    pub fn tick(&mut self) -> CsTick {
        let changed = !self.event_rx.is_empty();
        let banner = self.drain_events();
        self.poll_unread_if_due();
        CsTick { banner, changed }
    }

    /// Slow badge refresh. The clock is stamped when the request goes out,
    /// not when the count lands, so a hung or failing fetch cannot queue a
    /// fresh request on every tick.
    fn poll_unread_if_due(&mut self) {
        if !unread_poll_due(self.is_linked(), self.last_unread_poll.elapsed()) {
            return;
        }
        self.last_unread_poll = Instant::now();
        self.service
            .refresh_unread_task(self.user_id, !self.pinned.is_empty());
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => {
                    if let Some(next) = self.apply_event(event) {
                        banner = Some(next);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive cyberspace event");
                    break;
                }
            }
        }
        banner
    }

    fn apply_event(&mut self, event: CsEvent) -> Option<Banner> {
        match event {
            CsEvent::LinkStatus {
                user_id,
                username,
                feed_read_at,
                circ_rooms,
                circ_room_reads,
            } if user_id == self.user_id => {
                self.link = match username {
                    Some(username) => LinkStatus::Linked { username },
                    None => LinkStatus::Unlinked,
                };
                self.feed_read_at = feed_read_at;
                self.feed_marker_at = feed_read_at;
                self.pinned = circ_rooms;
                self.room_reads = circ_room_reads;
                None
            }
            CsEvent::LinkSucceeded { user_id, username } if user_id == self.user_id => {
                self.link = LinkStatus::Linked {
                    username: username.clone(),
                };
                if matches!(self.modal, Some(Modal::Link(_))) {
                    self.modal = None;
                }
                Some(Banner::success(&format!(
                    "Linked to cyberspace as {username}"
                )))
            }
            CsEvent::LinkFailed { user_id, error } if user_id == self.user_id => {
                if let Some(Modal::Link(link)) = &mut self.modal {
                    link.error = Some(error);
                    link.busy = false;
                    None
                } else {
                    Some(Banner::error(&format!("Cyberspace link failed: {error}")))
                }
            }
            CsEvent::Unlinked { user_id } if user_id == self.user_id => {
                self.link = LinkStatus::Unlinked;
                self.posts.clear();
                self.notifications.clear();
                self.thread = None;
                // Dropping the open room closes its stream and heartbeat:
                // an unlinked account must not still be present in a room.
                // Directly, not via `leave_room`: the account row is gone,
                // so there is no cursor left to stamp.
                self.open_room = None;
                self.pinned.clear();
                self.room_reads.clear();
                self.room_last_message.clear();
                self.unread_notifications = 0;
                self.unread_entries = 0;
                self.feed_read_at = None;
                self.mark_read_on_load = false;
                self.feed_marker_at = None;
                self.view = View::Feed;
                Some(Banner::success("Cyberspace account unlinked."))
            }
            CsEvent::FeedLoaded { user_id, posts } if user_id == self.user_id => {
                self.posts = posts;
                self.selected = clamp_index(self.selected, self.posts.len());
                self.loading = false;
                if self.mark_read_on_load {
                    self.mark_read_on_load = false;
                    self.mark_feed_read();
                }
                None
            }
            CsEvent::ThreadLoaded { user_id, thread } if user_id == self.user_id => {
                // Only adopt the thread the user is still looking at; a stale
                // load for a thread they already left would yank the view.
                if self.view == View::Thread
                    && self.thread_target.as_deref() == Some(thread.post.post_id.as_str())
                {
                    self.thread = Some(thread);
                    self.loading = false;
                }
                None
            }
            CsEvent::NotificationsLoaded {
                user_id,
                notifications,
            } if user_id == self.user_id => {
                self.notifications = dedupe_notifications(notifications);
                self.notif_selected = clamp_index(self.notif_selected, self.notifications.len());
                self.loading = false;
                self.unread_notifications = 0;
                None
            }
            CsEvent::UnreadCount { user_id, count } if user_id == self.user_id => {
                self.unread_notifications = count;
                None
            }
            CsEvent::RecentEntries { user_id, posts } if user_id == self.user_id => {
                self.unread_entries = count_unread_entries(&posts, self.feed_read_at);
                None
            }
            CsEvent::PostCreated { user_id, title, .. } if user_id == self.user_id => {
                if matches!(self.modal, Some(Modal::Compose(_))) {
                    self.modal = None;
                }
                let label = title.as_deref().unwrap_or("your entry");
                Some(Banner::success(&format!("Published {label} on cyberspace")))
            }
            CsEvent::ReplyPosted { user_id, .. } if user_id == self.user_id => {
                if matches!(self.modal, Some(Modal::Reply(_))) {
                    self.modal = None;
                }
                Some(Banner::success("Reply posted on cyberspace."))
            }
            CsEvent::CircRooms { user_id, rooms } if user_id == self.user_id => {
                // The roster answers two consumers: the picker (when open)
                // and the rail's unread dots, which compare each room's
                // last_message_at against this session's read cursor.
                self.room_last_message = rooms
                    .iter()
                    .filter_map(|room| Some((room.key().to_string(), room.last_message_at?)))
                    .collect();
                // A roster landing while the user sits in a room is theirs to
                // acknowledge: they are looking at it. Without this, a roster
                // that first arrives mid-visit would dot the room the moment
                // they step out of it.
                self.stamp_open_room_read();
                if let Some(Modal::Rooms(modal)) = &mut self.modal {
                    modal.roster = rooms;
                    modal.selected = clamp_index(modal.selected, modal.roster.len());
                    modal.loading = false;
                }
                None
            }
            CsEvent::CircPinned { user_id, rooms } if user_id == self.user_id => {
                // Another session of the same user pinned something; adopt
                // their list rather than keeping a divergent rail.
                self.pinned = rooms;
                None
            }
            CsEvent::CircHistoryLoaded {
                user_id,
                room,
                messages,
            } if user_id == self.user_id => {
                let loaded = if let Some(open) = &mut self.open_room
                    && open.slug == room
                {
                    open.loading = false;
                    for message in messages {
                        merge_message(&mut open.messages, message);
                    }
                    trim_messages(&mut open.messages);
                    true
                } else {
                    false
                };
                // History landing is the room being read: the cursor moves
                // now, not only on the way out, so a session that ends
                // abruptly still remembers this visit.
                if loaded {
                    self.stamp_open_room_read();
                }
                None
            }
            CsEvent::CircStreamed {
                user_id,
                room,
                event,
            } if user_id == self.user_id => {
                // A frame for a room this session is not in belongs to another
                // session of the same user; theirs to apply, not ours.
                if let Some(open) = &mut self.open_room
                    && open.slug == room
                {
                    apply_stream_event(open, event);
                }
                None
            }
            CsEvent::CircStreamEnded { user_id, room } if user_id == self.user_id => {
                if let Some(open) = &mut self.open_room
                    && open.slug == room
                {
                    open.stream_down = true;
                }
                None
            }
            CsEvent::ActionFailed { user_id, error } if user_id == self.user_id => {
                self.loading = false;
                // A failed action ends any pending read: a later feed load the
                // user did not ask for must not inherit the mark.
                self.mark_read_on_load = false;
                match &mut self.modal {
                    Some(Modal::Compose(compose)) if compose.busy => {
                        compose.error = Some(error);
                        compose.busy = false;
                        None
                    }
                    Some(Modal::Reply(reply)) if reply.busy => {
                        reply.error = Some(error);
                        reply.busy = false;
                        None
                    }
                    Some(Modal::Link(link)) if link.busy => {
                        link.error = Some(error);
                        link.busy = false;
                        None
                    }
                    // The picker shows its own failure: a roster that never
                    // arrived is the modal's problem, not a page-level banner.
                    Some(Modal::Rooms(rooms)) => {
                        rooms.error = Some(error);
                        rooms.loading = false;
                        None
                    }
                    _ => Some(Banner::error(&error)),
                }
            }
            _ => None,
        }
    }
}

/// Apply one live frame to the open room. Their stream carries edits as well
/// as arrivals, so a client that only appends never shows a deletion.
pub(crate) fn apply_stream_event(room: &mut OpenRoom, event: CircStreamEvent) {
    match event {
        CircStreamEvent::Window(messages) => {
            for message in messages {
                merge_message(&mut room.messages, message);
            }
        }
        CircStreamEvent::Upsert(message) => merge_message(&mut room.messages, *message),
        CircStreamEvent::Patch {
            id,
            content,
            deleted,
        } => {
            if let Some(message) = room.messages.iter_mut().find(|message| message.id == id) {
                if let Some(content) = content {
                    message.content = content;
                }
                if deleted {
                    // A tombstone keeps its author and time, and loses
                    // everything that hung off the message.
                    message.deleted = true;
                    message.image_url = None;
                    message.gif_url = None;
                    message.styles.clear();
                }
            }
        }
        CircStreamEvent::Removed(id) => room.messages.retain(|message| message.id != id),
    }
    trim_messages(&mut room.messages);
}

/// Insert or replace by id, keeping the list oldest-first. History and the
/// stream's opening window overlap by design, so the same message arriving
/// twice must land as one row.
fn merge_message(messages: &mut Vec<CircMessage>, message: CircMessage) {
    match messages
        .iter()
        .position(|existing| existing.id == message.id)
    {
        Some(index) => messages[index] = message,
        None => {
            let at = messages
                .iter()
                .position(|existing| existing.timestamp > message.timestamp);
            match at {
                Some(index) => messages.insert(index, message),
                None => messages.push(message),
            }
        }
    }
}

/// Drop the oldest rows past the cap: a long sitting in a busy room should not
/// grow a session's memory without bound.
fn trim_messages(messages: &mut Vec<CircMessage>) {
    let excess = messages.len().saturating_sub(CIRC_MESSAGE_CAP);
    if excess > 0 {
        messages.drain(..excess);
    }
}

pub(crate) fn single_line(input: &TextArea<'static>) -> String {
    input.lines().join(" ").trim().to_string()
}

/// Topics: comma or whitespace separated, lowercased, deduped, max 3.
pub(crate) fn parse_topics(raw: &str) -> Result<Vec<String>, String> {
    let mut topics: Vec<String> = Vec::new();
    for topic in raw
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
    {
        let topic = topic.trim_start_matches('#').to_lowercase();
        if topic.is_empty() || topics.contains(&topic) {
            continue;
        }
        topics.push(topic);
    }
    if topics.len() > MAX_TOPICS {
        return Err(format!("up to {MAX_TOPICS} topics"));
    }
    Ok(topics)
}

/// An unlinked session has no token to poll with, and a linked one polls no
/// faster than [`UNREAD_POLL_INTERVAL`].
pub(crate) fn unread_poll_due(linked: bool, since_last_poll: Duration) -> bool {
    linked && since_last_poll >= UNREAD_POLL_INTERVAL
}

/// Never fetch for an unlinked user or on top of a fetch already in flight.
/// `None` means this session has not loaded the feed yet, which always fetches.
pub(crate) fn feed_reload_due(
    linked: bool,
    loading: bool,
    since_last_load: Option<Duration>,
) -> bool {
    if !linked || loading {
        return false;
    }
    match since_last_load {
        None => true,
        Some(elapsed) => elapsed >= FEED_RELOAD_INTERVAL,
    }
}

/// How many of the probed entries the user has not seen. The probe page is
/// deliberately small (`UNREAD_PROBE_LIMIT`), so a user who has been away long
/// enough sees the page size rather than the true number: the badge is a nudge,
/// not an inventory, and the alternative is pulling their whole feed on a timer.
///
/// A `None` cursor is a user who has never opened the pane. That reads as
/// nothing unread, because opening for the first time to "10 new" would be
/// counting entries that were never theirs to miss.
pub(crate) fn count_unread_entries(posts: &[CsPost], cursor: Option<DateTime<Utc>>) -> i64 {
    match cursor {
        None => 0,
        Some(_) => posts
            .iter()
            .filter(|post| is_newer_than(post, cursor))
            .count() as i64,
    }
}

/// An entry with no timestamp cannot be placed against the cursor, so it is
/// never new: a badge that counts entries the user cannot find is worse than
/// one that misses them.
fn is_newer_than(post: &CsPost, cursor: Option<DateTime<Utc>>) -> bool {
    match (post.created_at, cursor) {
        (Some(created), Some(cursor)) => created > cursor,
        _ => false,
    }
}

/// What makes two notifications the same event: kind, who did it, which entry
/// it points at, and which reply it came from.
type NotificationKey = (String, Option<String>, Option<String>, Option<String>);

/// One row per thing that happened, rather than one per notification their
/// API hands out: a single reply shows up as several `reply` notifications
/// with distinct ids, so the view fills with rows that read identically and
/// all open the same entry. Keeping the first occurrence keeps the newest
/// stamp, since the page is newest-first.
///
/// The key is the finest grain the payload offers. `reply_id` is what makes it
/// safe: two real replies from one person on one entry are different replies
/// and both survive, while the same reply notified again (an edit, say) folds
/// away. Kinds that carry no reply id fall back to the entry, which is as
/// precise as they get. No `×N` count on the row, because a count would have
/// to be trusted to mean something and duplicates are not repeats.
pub(crate) fn dedupe_notifications(notifications: Vec<CsNotification>) -> Vec<CsNotification> {
    let mut seen: HashSet<NotificationKey> = HashSet::new();
    notifications
        .into_iter()
        .filter(|notification| {
            seen.insert((
                notification.kind.clone(),
                notification.actor_username.clone(),
                notification.target_id.clone(),
                notification.reply_id().map(str::to_string),
            ))
        })
        .collect()
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn step_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, len as isize - 1) as usize
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
