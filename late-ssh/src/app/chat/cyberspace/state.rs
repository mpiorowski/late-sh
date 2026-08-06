//! Per-session UI state for the cyberspace pane and its modals.
//!
//! Everything shown here was fetched by this user's own linked account and
//! lives only in this session's memory: cyberspace content is never cached
//! server-side or shown to anyone but the user who fetched it.

use std::time::{Duration, Instant};

use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::common::composer::{new_themed_textarea, set_themed_textarea_cursor_visible};
use crate::app::common::primitives::Banner;

use super::api::{CsNotification, CsPost, NewPost};
use super::svc::{CsEvent, CsThread, CyberspaceService};

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

pub(crate) struct ReplyModal {
    pub post: CsPost,
    pub body: TextArea<'static>,
    pub error: Option<String>,
    pub busy: bool,
}

pub(crate) enum Modal {
    Link(LinkModal),
    Compose(ComposeModal),
    Reply(ReplyModal),
}

pub struct State {
    service: CyberspaceService,
    user_id: Uuid,
    event_rx: broadcast::Receiver<CsEvent>,
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
    pub(crate) notifications: Vec<CsNotification>,
    pub(crate) notif_selected: usize,
    unread_count: i64,
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
            link: LinkStatus::Unknown,
            view: View::Feed,
            posts: Vec::new(),
            selected: 0,
            thread: None,
            thread_target: None,
            thread_scroll: 0,
            notifications: Vec::new(),
            notif_selected: 0,
            unread_count: 0,
            // `session_init_task` above fetches the count for a linked user,
            // so the interval starts running from session start.
            last_unread_poll: Instant::now(),
            last_feed_load: None,
            loading: false,
            modal: None,
        }
    }

    pub fn unread_count(&self) -> i64 {
        self.unread_count
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
    pub fn opened(&mut self) {
        self.view = View::Feed;
        if feed_reload_due(
            self.is_linked(),
            self.loading,
            self.last_feed_load.map(|at| at.elapsed()),
        ) {
            self.load_feed();
        }
    }

    /// `r`: the user asking for the feed, so no interval applies.
    pub(crate) fn refresh(&mut self) {
        if self.is_linked() {
            self.load_feed();
        }
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
                let ceiling = self.thread.as_ref().map_or(0, thread_scroll_ceiling);
                self.thread_scroll = self
                    .thread_scroll
                    .saturating_add_signed(delta)
                    .min(ceiling);
            }
            View::Notifications => {
                self.notif_selected =
                    step_index(self.notif_selected, delta, self.notifications.len());
            }
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
        self.thread_scroll = 0;
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
        self.thread_scroll = 0;
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
        self.thread_scroll = 0;
    }

    pub fn open_link_modal(&mut self) {
        let mut email = new_themed_textarea("you@example.com", WrapMode::None, true);
        set_themed_textarea_cursor_visible(&mut email, true);
        let mut password = new_themed_textarea("password", WrapMode::None, false);
        password.set_mask_char('•');
        self.modal = Some(Modal::Link(LinkModal {
            email,
            password,
            focus: LinkField::Email,
            error: None,
            busy: false,
        }));
    }

    pub fn open_compose_modal(&mut self) -> Option<Banner> {
        if !self.is_linked() {
            return Some(Banner::error(
                "Link your cyberspace account first: /cs link",
            ));
        }
        self.modal = Some(Modal::Compose(ComposeModal {
            title: new_themed_textarea("Title (optional)", WrapMode::None, true),
            topics: new_themed_textarea("Topics, up to 3 (optional)", WrapMode::None, false),
            body: new_themed_textarea("Write your entry (markdown)...", WrapMode::Word, false),
            focus: ComposeField::Title,
            error: None,
            busy: false,
        }));
        None
    }

    pub(crate) fn open_reply_modal(&mut self) {
        let Some(post) = self.current_thread_post() else {
            return;
        };
        self.modal = Some(Modal::Reply(ReplyModal {
            post,
            body: new_themed_textarea("Write your reply (markdown)...", WrapMode::Word, true),
            error: None,
            busy: false,
        }));
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
            None => {}
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
        self.service.refresh_unread_task(self.user_id);
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
            CsEvent::LinkStatus { user_id, username } if user_id == self.user_id => {
                self.link = match username {
                    Some(username) => LinkStatus::Linked { username },
                    None => LinkStatus::Unlinked,
                };
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
                self.unread_count = 0;
                self.view = View::Feed;
                Some(Banner::success("Cyberspace account unlinked."))
            }
            CsEvent::FeedLoaded { user_id, posts } if user_id == self.user_id => {
                self.posts = posts;
                self.selected = clamp_index(self.selected, self.posts.len());
                self.loading = false;
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
                self.notifications = notifications;
                self.notif_selected = clamp_index(self.notif_selected, self.notifications.len());
                self.loading = false;
                self.unread_count = 0;
                None
            }
            CsEvent::UnreadCount { user_id, count } if user_id == self.user_id => {
                self.unread_count = count;
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
            CsEvent::ActionFailed { user_id, error } if user_id == self.user_id => {
                self.loading = false;
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
                    _ => Some(Banner::error(&error)),
                }
            }
            _ => None,
        }
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

/// An upper bound on thread scrolling. The renderer does the exact clamp,
/// since only it knows the viewport height; this stops `j` past the end from
/// running away, which would otherwise need one `k` per press before the view
/// moved again.
fn thread_scroll_ceiling(thread: &CsThread) -> usize {
    let post_lines = thread.post.content.lines().count();
    let reply_lines: usize = thread
        .replies
        .iter()
        .map(|reply| reply.content.lines().count() + 2)
        .sum();
    post_lines + reply_lines + 8
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
