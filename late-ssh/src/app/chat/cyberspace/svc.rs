//! Orchestration for the cyberspace pane: fire-and-forget tasks that call the
//! API as the linked user and publish results as broadcast events. The service
//! keeps no per-user content (their terms ban caching for redistribution);
//! everything fetched lives only in the requesting session's UI state. The
//! one thing held here is the in-memory id-token cache, so a browsing session
//! doesn't re-refresh on every keypress.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use late_core::{db::Db, models::cyberspace_account::CyberspaceAccount};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::activity::publisher::ActivityPublisher;

use super::api::{
    CircMessage, CircRoom, CircStreamBuffer, CircStreamEvent, CsApi, CsApiError, CsNotification,
    CsPost, CsReply, NewPost, parse_circ_stream_frame,
};

/// Firebase id tokens live ~60 minutes; refresh with slack so a token handed
/// out by the cache is never about to expire mid-request.
const TOKEN_CACHE_TTL: Duration = Duration::from_secs(50 * 60);
/// How many consecutive failures end a room's stream. Their docs ask for one
/// stream per room and no reconnect loops, so a room whose stream will not
/// stay up tells the session and stops rather than retrying forever.
const CIRC_STREAM_MAX_FAILURES: u32 = 3;
const CIRC_STREAM_RETRY_DELAY: Duration = Duration::from_secs(5);
/// How long a stream has to have carried the room before its ending counts as
/// the ordinary token expiry rather than as a failure.
const CIRC_STREAM_MIN_RUN: Duration = Duration::from_secs(30);
/// A frame that never terminates must not grow a session's memory without
/// bound; their frames are a message each, so this is far above any real one.
const CIRC_STREAM_BUFFER_CAP: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct CsThread {
    pub post: CsPost,
    pub replies: Vec<CsReply>,
}

/// Events carry their data instead of going through a shared snapshot:
/// cyberspace content is per-user by their API terms, so there is nothing
/// shared to snapshot. Sessions filter on `user_id`.
#[derive(Clone, Debug)]
pub enum CsEvent {
    /// Session-init answer: whether this user has a linked account, and where
    /// their feed reading left off.
    LinkStatus {
        user_id: Uuid,
        username: Option<String>,
        feed_read_at: Option<DateTime<Utc>>,
        circ_rooms: Vec<String>,
        /// Per-room read cursors (slug -> newest message ts seen, their
        /// clock), landing with the pins so the rail's unread dots have
        /// something to compare against from the first roster fetch.
        circ_room_reads: HashMap<String, i64>,
    },
    LinkSucceeded {
        user_id: Uuid,
        username: String,
    },
    LinkFailed {
        user_id: Uuid,
        error: String,
    },
    Unlinked {
        user_id: Uuid,
    },
    FeedLoaded {
        user_id: Uuid,
        posts: Vec<CsPost>,
    },
    ThreadLoaded {
        user_id: Uuid,
        thread: CsThread,
    },
    NotificationsLoaded {
        user_id: Uuid,
        notifications: Vec<CsNotification>,
    },
    UnreadCount {
        user_id: Uuid,
        count: i64,
    },
    /// The newest few entries, for the session to count against its own read
    /// cursor. The posts travel rather than the count, so the cursor never has
    /// to leave the session that owns it.
    RecentEntries {
        user_id: Uuid,
        posts: Vec<CsPost>,
    },
    PostCreated {
        user_id: Uuid,
        title: Option<String>,
    },
    ReplyPosted {
        user_id: Uuid,
        post_id: String,
    },
    /// Their chat roster, for the room list the user picks from.
    CircRooms {
        user_id: Uuid,
        rooms: Vec<CircRoom>,
    },
    /// The pinned rail rooms, after a load or an add/remove.
    CircPinned {
        user_id: Uuid,
        rooms: Vec<String>,
    },
    /// A page of room history, answering the room being opened.
    CircHistoryLoaded {
        user_id: Uuid,
        room: String,
        messages: Vec<CircMessage>,
    },
    /// A live frame from the open room's stream. Deletions ride this too, as
    /// a patch against a message already on screen.
    CircStreamed {
        user_id: Uuid,
        room: String,
        event: CircStreamEvent,
    },
    /// The open room's stream ended (their token expired, or the connection
    /// dropped). The session decides whether to reopen: only the room still
    /// on screen deserves one.
    CircStreamEnded {
        user_id: Uuid,
        room: String,
    },
    /// Any task failure the user should see, rendered as a banner.
    ActionFailed {
        user_id: Uuid,
        error: String,
    },
}

struct CachedToken {
    id_token: String,
    /// Where their realtime database lives, which the chat stream needs and
    /// only the login/refresh answer carries.
    rtdb_url: Option<String>,
    fetched_at: Instant,
}

/// A room the user currently has open. Holding one is what makes the room
/// fetch anything: its history load, its live stream, and its presence
/// heartbeat all die when it drops, and dropping it also announces the user
/// out of the room. Nothing in the pane may keep one for a room that is not
/// on screen.
pub struct CircRoomSession {
    service: CyberspaceService,
    user_id: Uuid,
    room: String,
    activity: Arc<AtomicI64>,
    tasks: Vec<JoinHandle<()>>,
}

impl CircRoomSession {
    pub fn room(&self) -> &str {
        &self.room
    }

    /// The user did something here. Their presence list shows anyone quiet for
    /// too long as idle, and only this session knows when its user last acted.
    pub fn note_activity(&self) {
        self.activity.store(now_ms(), Ordering::Relaxed);
    }
}

impl Drop for CircRoomSession {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        self.service
            .leave_circ_room_task(self.user_id, self.room.clone());
    }
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Why a usable id token could not be produced. `NotLinked` renders as the
/// login form; `Broken` means the stored refresh token was rejected (password
/// change, revocation) and the user must re-link.
#[derive(Debug)]
enum TokenError {
    NotLinked,
    Broken(String),
    Transport(String),
}

impl TokenError {
    fn user_message(&self) -> String {
        match self {
            Self::NotLinked => "cyberspace account not linked. /cs link".to_string(),
            Self::Broken(error) => format!("cyberspace link broken ({error}). /cs link again"),
            Self::Transport(error) => format!("cyberspace unreachable: {error}"),
        }
    }
}

#[derive(Clone)]
pub struct CyberspaceService {
    db: Db,
    api: CsApi,
    tokens: Arc<Mutex<HashMap<Uuid, CachedToken>>>,
    /// One refresh per user at a time: opening a room fires three tasks at
    /// once, and each hitting a cold or expired cache would otherwise POST
    /// its own refresh with the same stored refresh token.
    refresh_guards: Arc<Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>>,
    evt_tx: broadcast::Sender<CsEvent>,
    activity: Option<ActivityPublisher>,
}

impl CyberspaceService {
    pub fn new(db: Db, base_url: String) -> Self {
        let (evt_tx, _) = broadcast::channel(256);
        Self {
            db,
            api: CsApi::new(base_url),
            tokens: Arc::new(Mutex::new(HashMap::new())),
            refresh_guards: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
            activity: None,
        }
    }

    pub fn with_activity(mut self, activity: ActivityPublisher) -> Self {
        self.activity = Some(activity);
        self
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<CsEvent> {
        self.evt_tx.subscribe()
    }

    fn publish(&self, event: CsEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::debug!(%e, "no cyberspace event subscribers");
        }
    }

    fn fail(&self, user_id: Uuid, error: String) {
        self.publish(CsEvent::ActionFailed { user_id, error });
    }

    /// Session init: answer whether the user is linked and where their feed
    /// reading left off, and if linked fetch the unread badge. Later refreshes
    /// come from the session tick (`State::poll_unread_if_due`) or a user
    /// action in the pane.
    pub fn session_init_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let account = match service.linked_account(user_id).await {
                    Ok(account) => account,
                    Err(e) => {
                        late_core::error_span!(
                            "cyberspace_link_status_failed",
                            error = ?e,
                            user_id = %user_id,
                            "failed to load cyberspace link status"
                        );
                        return;
                    }
                };
                let linked = account.is_some();
                let has_rooms = account
                    .as_ref()
                    .is_some_and(|account| !account.circ_rooms.is_empty());
                // The cursor lands before any entries do, so the session has
                // something to count them against, and the pinned rooms land
                // before the rail is drawn.
                service.publish(CsEvent::LinkStatus {
                    user_id,
                    username: account.as_ref().map(|a| a.cs_username.clone()),
                    feed_read_at: account.as_ref().and_then(|a| a.feed_read_at),
                    circ_room_reads: account
                        .as_ref()
                        .map(|a| a.room_read_cursors())
                        .unwrap_or_default(),
                    circ_rooms: account.map(|a| a.circ_rooms).unwrap_or_default(),
                });
                if linked {
                    service.refresh_unread(user_id, has_rooms).await;
                }
            }
            .instrument(info_span!("cyberspace.session_init", user_id = %user_id)),
        );
    }

    /// Fire-and-forget cursor write. Nobody upstream waits on it: the session
    /// already moved its own copy, and a lost write only re-counts entries the
    /// user has seen.
    pub fn mark_feed_read_task(&self, user_id: Uuid, at: DateTime<Utc>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    CyberspaceAccount::mark_feed_read(&client, user_id, at).await
                }
                .await;
                if let Err(e) = result {
                    late_core::error_span!(
                        "cyberspace_mark_feed_read_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to move the cyberspace feed read cursor"
                    );
                }
            }
            .instrument(info_span!("cyberspace.mark_feed_read", user_id = %user_id)),
        );
    }

    pub fn link_task(&self, user_id: Uuid, email: String, password: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service.do_link(user_id, email, password).await {
                    Ok(account) => {
                        service.publish(CsEvent::LinkSucceeded {
                            user_id,
                            username: account.cs_username.clone(),
                        });
                        service.load_feed(user_id).await;
                        service
                            .refresh_unread(user_id, !account.circ_rooms.is_empty())
                            .await;
                    }
                    Err(error) => service.publish(CsEvent::LinkFailed { user_id, error }),
                }
            }
            .instrument(info_span!("cyberspace.link", user_id = %user_id)),
        );
    }

    pub fn unlink_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    CyberspaceAccount::delete_for_user(&client, user_id).await
                }
                .await;
                match result {
                    Ok(_) => {
                        service
                            .tokens
                            .lock()
                            .expect("token cache lock")
                            .remove(&user_id);
                        service.publish(CsEvent::Unlinked { user_id });
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "cyberspace_unlink_failed",
                            error = ?e,
                            user_id = %user_id,
                            "failed to unlink cyberspace account"
                        );
                        service.fail(user_id, "unlink failed, try again".to_string());
                    }
                }
            }
            .instrument(info_span!("cyberspace.unlink", user_id = %user_id)),
        );
    }

    pub fn load_feed_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move { service.load_feed(user_id).await }
                .instrument(info_span!("cyberspace.feed", user_id = %user_id)),
        );
    }

    pub fn load_thread_task(&self, user_id: Uuid, post: CsPost) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.list_replies(&token, &post.post_id).await {
                    Ok(replies) => service.publish(CsEvent::ThreadLoaded {
                        user_id,
                        thread: CsThread { post, replies },
                    }),
                    Err(e) => service.fail(user_id, format!("loading replies failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.thread", user_id = %user_id)),
        );
    }

    /// Open a thread from a post id alone, which is all a notification gives
    /// us. The post itself is fetched first: the entry a notification is
    /// about is often older than the feed page in memory, so it cannot be
    /// looked up locally.
    pub fn load_thread_by_id_task(&self, user_id: Uuid, post_id: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                let post = match service.api.get_post(&token, &post_id).await {
                    Ok(post) => post,
                    Err(e) => {
                        return service.fail(user_id, format!("opening the entry failed: {e}"));
                    }
                };
                match service.api.list_replies(&token, &post.post_id).await {
                    Ok(replies) => service.publish(CsEvent::ThreadLoaded {
                        user_id,
                        thread: CsThread { post, replies },
                    }),
                    Err(e) => service.fail(user_id, format!("loading replies failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.thread_by_id", user_id = %user_id)),
        );
    }

    pub fn post_task(&self, user_id: Uuid, post: NewPost) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.create_post(&token, &post).await {
                    Ok(created) => {
                        if let Some(activity) = &service.activity {
                            activity.cyberspace_posted_task(user_id, created.title.clone());
                        }
                        service.publish(CsEvent::PostCreated {
                            user_id,
                            title: created.title,
                        });
                        service.load_feed(user_id).await;
                    }
                    Err(e) => service.fail(user_id, format!("post failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.post", user_id = %user_id)),
        );
    }

    pub fn reply_task(&self, user_id: Uuid, post: CsPost, content: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                let post_id = post.post_id.clone();
                match service.api.create_reply(&token, &post_id, &content).await {
                    Ok(()) => {
                        service.publish(CsEvent::ReplyPosted {
                            user_id,
                            post_id: post_id.clone(),
                        });
                        // Reload the thread so the fresh reply shows up in place.
                        match service.api.list_replies(&token, &post_id).await {
                            Ok(replies) => service.publish(CsEvent::ThreadLoaded {
                                user_id,
                                thread: CsThread { post, replies },
                            }),
                            Err(e) => {
                                tracing::debug!(%e, "reply landed but thread reload failed");
                            }
                        }
                    }
                    Err(e) => service.fail(user_id, format!("reply failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.reply", user_id = %user_id)),
        );
    }

    /// Load notifications, then mark them read server-side (opening the view
    /// is reading them, same contract as the RSS inbox).
    pub fn load_notifications_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.list_notifications(&token).await {
                    Ok(notifications) => {
                        service.publish(CsEvent::NotificationsLoaded {
                            user_id,
                            notifications,
                        });
                        if let Err(e) = service.api.mark_all_notifications_read(&token).await {
                            tracing::debug!(%e, "marking cyberspace notifications read failed");
                        }
                        service.publish(CsEvent::UnreadCount { user_id, count: 0 });
                    }
                    Err(e) => service.fail(user_id, format!("loading notifications failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.notifications", user_id = %user_id)),
        );
    }

    /// Their chat roster, for the room list the user pins from.
    pub fn load_circ_rooms_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.list_circ_rooms(&token).await {
                    Ok(rooms) => service.publish(CsEvent::CircRooms { user_id, rooms }),
                    Err(e) => service.fail(user_id, format!("loading chat rooms failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.circ_rooms", user_id = %user_id)),
        );
    }

    /// Write the pinned rail rooms. The session already moved its own copy, so
    /// this only has to make it survive a reconnect.
    pub fn set_circ_pinned_task(&self, user_id: Uuid, rooms: Vec<String>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    CyberspaceAccount::set_circ_rooms(&client, user_id, &rooms).await
                }
                .await;
                match result {
                    Ok(()) => service.publish(CsEvent::CircPinned { user_id, rooms }),
                    Err(e) => {
                        late_core::error_span!(
                            "cyberspace_pin_rooms_failed",
                            error = ?e,
                            user_id = %user_id,
                            "failed to store pinned cyberspace chat rooms"
                        );
                        service.fail(user_id, "pinning the room failed, try again".to_string());
                    }
                }
            }
            .instrument(info_span!("cyberspace.circ_pin", user_id = %user_id)),
        );
    }

    pub fn send_circ_message_task(&self, user_id: Uuid, room: String, content: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                // Nothing is published on success: the message comes back
                // through the room's own stream, the same way everyone else's
                // does, so there is no local echo to reconcile.
                if let Err(e) = service.api.send_circ_message(&token, &room, &content).await {
                    service.fail(user_id, format!("sending failed: {e}"));
                }
            }
            .instrument(info_span!("cyberspace.circ_send", user_id = %user_id)),
        );
    }

    /// Open a room: its history, its live stream, and the presence heartbeat
    /// that puts the user in its user list. Everything here dies with the
    /// returned handle, which is what keeps a closed room from fetching.
    pub fn open_circ_room(&self, user_id: Uuid, room: String) -> CircRoomSession {
        let activity = Arc::new(AtomicI64::new(now_ms()));
        let history = {
            let service = self.clone();
            let room = room.clone();
            tokio::spawn(
                async move { service.load_circ_history(user_id, room).await }
                    .instrument(info_span!("cyberspace.circ_history", user_id = %user_id)),
            )
        };
        let stream = {
            let service = self.clone();
            let room = room.clone();
            tokio::spawn(
                async move { service.run_circ_stream(user_id, room).await }
                    .instrument(info_span!("cyberspace.circ_stream", user_id = %user_id)),
            )
        };
        let presence = {
            let service = self.clone();
            let room = room.clone();
            let activity = Arc::clone(&activity);
            tokio::spawn(
                async move { service.run_circ_presence(user_id, room, activity).await }
                    .instrument(info_span!("cyberspace.circ_presence", user_id = %user_id)),
            )
        };
        CircRoomSession {
            service: self.clone(),
            user_id,
            room,
            activity,
            tasks: vec![history, stream, presence],
        }
    }

    async fn load_circ_history(&self, user_id: Uuid, room: String) {
        let token = match self.id_token(user_id).await {
            Ok(token) => token,
            Err(e) => return self.fail(user_id, e.user_message()),
        };
        match self.api.read_circ_room(&token, &room, None).await {
            Ok(history) => {
                self.publish(CsEvent::CircHistoryLoaded {
                    user_id,
                    room: room.clone(),
                    messages: history.messages,
                });
                // Opening a room is reading it, same contract as the
                // notifications view. Their side owns this state, so this is
                // also what clears the rail mark on the next roster poll.
                if let Err(e) = self.api.mark_circ_room_read(&token, &room).await {
                    tracing::debug!(%e, "marking cyberspace chat room read failed");
                }
            }
            Err(e) => self.fail(user_id, format!("loading {room} failed: {e}")),
        }
    }

    /// The live stream, reopened when their id token expires (~60 minutes)
    /// and after a dropped connection. Bounded on purpose: their docs ask for
    /// one stream per room and no reconnect loops, so a room that cannot hold
    /// a stream gives up and tells the session rather than hammering them.
    async fn run_circ_stream(&self, user_id: Uuid, room: String) {
        let mut failures = 0;
        while failures < CIRC_STREAM_MAX_FAILURES {
            let started = Instant::now();
            let outcome = self.stream_once(user_id, &room).await;
            // A stream that carried the room for a while and then ended is the
            // expected token expiry, and earns a clean slate. One that ends
            // immediately counts against the budget however it ended: without
            // that, an instantly-closing stream reopens forever, which is the
            // reconnect loop their docs ask clients not to run.
            match (outcome, started.elapsed() >= CIRC_STREAM_MIN_RUN) {
                (Ok(()), true) => failures = 0,
                (Ok(()), false) => failures += 1,
                (Err(e), long_run) => {
                    failures += 1;
                    tracing::debug!(%e, room = %room, long_run, "cyberspace chat stream dropped");
                }
            }
            tokio::time::sleep(CIRC_STREAM_RETRY_DELAY).await;
        }
        self.publish(CsEvent::CircStreamEnded { user_id, room });
    }

    async fn stream_once(&self, user_id: Uuid, room: &str) -> Result<(), String> {
        let (token, rtdb_url) = self
            .token_and_rtdb(user_id)
            .await
            .map_err(|e| e.user_message())?;
        let response = self
            .api
            .open_circ_stream(&rtdb_url, room, &token)
            .await
            .map_err(|e| e.to_string())?;

        let mut stream = response.bytes_stream();
        // Frames are blank-line separated; a partial tail waits for the next
        // chunk rather than being parsed half-formed or decoded half-charred.
        let mut buffer = CircStreamBuffer::default();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.without_url().to_string())?;
            for frame in buffer.push(&chunk) {
                if let Some(event) = parse_circ_stream_frame(&frame) {
                    self.publish(CsEvent::CircStreamed {
                        user_id,
                        room: room.to_string(),
                        event,
                    });
                }
            }
            if buffer.pending_len() > CIRC_STREAM_BUFFER_CAP {
                return Err("chat stream sent an oversized frame".to_string());
            }
        }
        Ok(())
    }

    /// Announce the user in the room, then keep announcing at the cadence
    /// their own response names. Stopping is how they drop out of the list,
    /// so this task ending is a feature.
    async fn run_circ_presence(&self, user_id: Uuid, room: String, activity: Arc<AtomicI64>) {
        loop {
            let token = match self.id_token(user_id).await {
                Ok(token) => token,
                // No token means no presence; the room still reads and sends
                // once a token comes back, so this is not worth a banner.
                Err(_) => return,
            };
            let last_activity = activity.load(Ordering::Relaxed);
            let heartbeat = match self.api.circ_presence(&token, &room, last_activity).await {
                Ok(presence) => Duration::from_millis(presence.heartbeat_ms),
                Err(e) => {
                    tracing::debug!(%e, room = %room, "cyberspace presence heartbeat failed");
                    return;
                }
            };
            tokio::time::sleep(heartbeat).await;
        }
    }

    /// Politeness on the way out: drop off the room's user list now rather
    /// than when the heartbeat goes stale.
    fn leave_circ_room_task(&self, user_id: Uuid, room: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let Ok(token) = service.id_token(user_id).await else {
                    return;
                };
                if let Err(e) = service.api.leave_circ_room(&token, &room).await {
                    tracing::debug!(%e, room = %room, "leaving cyberspace chat room failed");
                }
            }
            .instrument(info_span!("cyberspace.circ_leave", user_id = %user_id)),
        );
    }

    async fn do_link(
        &self,
        user_id: Uuid,
        email: String,
        password: String,
    ) -> Result<CyberspaceAccount, String> {
        let tokens = match self.api.login(&email, &password).await {
            Ok(tokens) => tokens,
            Err(e) => return Err(format!("login failed: {e}")),
        };
        let Some(refresh_token) = tokens.refresh_token.clone() else {
            return Err("login response missing refresh token".to_string());
        };
        let identity = match self.api.me(&tokens.id_token).await {
            Ok(identity) => identity,
            Err(e) => return Err(format!("loading your cyberspace profile failed: {e}")),
        };
        let result = async {
            let client = self.db.get().await?;
            CyberspaceAccount::upsert_for_user(
                &client,
                user_id,
                &identity.cs_user_id,
                &identity.cs_username,
                &refresh_token,
            )
            .await
        }
        .await;
        match result {
            Ok(account) => {
                self.cache_token(user_id, tokens.id_token, tokens.rtdb_url);
                Ok(account)
            }
            Err(e) => {
                late_core::error_span!(
                    "cyberspace_link_store_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to store cyberspace link"
                );
                Err("storing the link failed, try again".to_string())
            }
        }
    }

    async fn load_feed(&self, user_id: Uuid) {
        let token = match self.id_token(user_id).await {
            Ok(token) => token,
            Err(e) => return self.fail(user_id, e.user_message()),
        };
        match self.api.list_feed(&token).await {
            Ok(posts) => self.publish(CsEvent::FeedLoaded { user_id, posts }),
            Err(e) => self.fail(user_id, format!("loading the feed failed: {e}")),
        }
    }

    /// Fire-and-forget badge refresh, driven by the session tick. Failures
    /// are logged inside `refresh_unread`: nobody upstream is waiting on it,
    /// and a stale badge is not worth a banner. `include_circ_roster` comes
    /// from the caller, who knows whether any rooms are pinned: a roster
    /// fetch that decorates nothing is a call their terms have no answer for.
    pub fn refresh_unread_task(&self, user_id: Uuid, include_circ_roster: bool) {
        let service = self.clone();
        tokio::spawn(
            async move { service.refresh_unread(user_id, include_circ_roster).await }
                .instrument(info_span!("cyberspace.unread", user_id = %user_id)),
        );
    }

    /// Every badge on one token: the notification count from their counter
    /// endpoint, the newest entries for the session to count unread ones out
    /// of, and (when rooms are pinned) the chat roster, whose
    /// `last_message_at` stamps are what the rail's room dots compare
    /// against.
    async fn refresh_unread(&self, user_id: Uuid, include_circ_roster: bool) {
        let token = match self.id_token(user_id).await {
            Ok(token) => token,
            Err(_) => return,
        };
        match self.api.unread_count(&token).await {
            Ok(count) => self.publish(CsEvent::UnreadCount { user_id, count }),
            Err(e) => tracing::debug!(%e, "cyberspace unread count failed"),
        }
        match self.api.list_recent_entries(&token).await {
            Ok(posts) => self.publish(CsEvent::RecentEntries { user_id, posts }),
            Err(e) => tracing::debug!(%e, "cyberspace recent entries failed"),
        }
        if include_circ_roster {
            match self.api.list_circ_rooms(&token).await {
                Ok(rooms) => self.publish(CsEvent::CircRooms { user_id, rooms }),
                Err(e) => tracing::debug!(%e, "cyberspace chat roster refresh failed"),
            }
        }
    }

    /// Fire-and-forget per-room read-cursor write. Nobody upstream waits on
    /// it: the session already moved its own copy, and a lost write only
    /// re-dots a room the user has seen.
    pub fn mark_circ_room_read_task(&self, user_id: Uuid, slug: String, last_message_ts: i64) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    CyberspaceAccount::mark_circ_room_read(&client, user_id, &slug, last_message_ts)
                        .await
                }
                .await;
                if let Err(e) = result {
                    late_core::error_span!(
                        "cyberspace_mark_circ_room_read_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to move a cyberspace chat room read cursor"
                    );
                }
            }
            .instrument(info_span!("cyberspace.mark_circ_room_read", user_id = %user_id)),
        );
    }

    async fn linked_account(&self, user_id: Uuid) -> anyhow::Result<Option<CyberspaceAccount>> {
        let client = self.db.get().await?;
        CyberspaceAccount::find_by_user_id(&client, user_id).await
    }

    /// Caching a token also drops every expired one. These are live bearer
    /// tokens for a third-party account: without the sweep the map keeps one
    /// resident for every user who has ever linked, for the life of the
    /// process, long after their session ended and the token went stale.
    fn cache_token(&self, user_id: Uuid, id_token: String, rtdb_url: Option<String>) {
        let mut tokens = self.tokens.lock().expect("token cache lock");
        tokens.retain(|_, cached| cached.fetched_at.elapsed() < TOKEN_CACHE_TTL);
        tokens.insert(
            user_id,
            CachedToken {
                id_token,
                rtdb_url,
                fetched_at: Instant::now(),
            },
        );
    }

    /// A token plus the realtime-database URL that came with it, for opening a
    /// chat stream. A login that answered without one means chat cannot stream
    /// for this user, which is a plain failure rather than a silent fallback.
    async fn token_and_rtdb(&self, user_id: Uuid) -> Result<(String, String), TokenError> {
        let id_token = self.id_token(user_id).await?;
        let rtdb_url = {
            let tokens = self.tokens.lock().expect("token cache lock");
            tokens
                .get(&user_id)
                .and_then(|cached| cached.rtdb_url.clone())
        };
        match rtdb_url {
            Some(rtdb_url) => Ok((id_token, rtdb_url)),
            None => Err(TokenError::Broken(
                "their login did not name a realtime database".to_string(),
            )),
        }
    }

    fn cached_token(&self, user_id: Uuid) -> Option<String> {
        let tokens = self.tokens.lock().expect("token cache lock");
        match tokens.get(&user_id) {
            Some(cached) if cached.fetched_at.elapsed() < TOKEN_CACHE_TTL => {
                Some(cached.id_token.clone())
            }
            _ => None,
        }
    }

    /// The user's refresh lock. Guards nobody holds are swept on the way,
    /// the same hygiene as the token cache: without it the map keeps an
    /// entry for every user who ever refreshed, for the life of the process.
    fn refresh_guard(&self, user_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        let mut guards = self.refresh_guards.lock().expect("refresh guard lock");
        guards.retain(|_, guard| Arc::strong_count(guard) > 1);
        guards.entry(user_id).or_default().clone()
    }

    /// A usable id token for the user: cache hit inside the TTL, otherwise a
    /// refresh with the stored refresh token. A rejected refresh token means
    /// the link is broken (password change, revocation) and needs a re-link.
    /// Concurrent callers on a cold cache (a room open fires three tasks at
    /// once) serialize on the per-user guard and share the one refresh the
    /// winner cached.
    async fn id_token(&self, user_id: Uuid) -> Result<String, TokenError> {
        if let Some(token) = self.cached_token(user_id) {
            return Ok(token);
        }
        let guard = self.refresh_guard(user_id);
        let _held = guard.lock().await;
        if let Some(token) = self.cached_token(user_id) {
            return Ok(token);
        }
        let refresh_token = {
            let client = self
                .db
                .get()
                .await
                .map_err(|e| TokenError::Transport(e.to_string()))?;
            match CyberspaceAccount::find_by_user_id(&client, user_id).await {
                Ok(Some(account)) => account.refresh_token,
                Ok(None) => return Err(TokenError::NotLinked),
                Err(e) => return Err(TokenError::Transport(e.to_string())),
            }
        };
        match self.api.refresh(&refresh_token).await {
            Ok(tokens) => {
                self.cache_token(user_id, tokens.id_token.clone(), tokens.rtdb_url);
                Ok(tokens.id_token)
            }
            Err(CsApiError::Api { code, .. }) => Err(TokenError::Broken(code)),
            Err(CsApiError::Transport(message)) => Err(TokenError::Transport(message)),
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
