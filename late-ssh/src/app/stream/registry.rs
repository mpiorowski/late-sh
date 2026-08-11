//! Process-global live-stream registry: one "watch me" stream per user,
//! in-memory only (scratchpad-registry tier: single replica, dies with the
//! process). late-ssh never touches a media byte; this registry only tracks
//! who is live, the capability ids behind the publisher/watch URLs, and the
//! watcher heartbeats behind the "12 watching" count. Media flows
//! browser/CLI -> LiveKit -> browser/CLI.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use late_core::MutexRecover;
use tokio::sync::watch;
use uuid::Uuid;

/// A registered stream whose publisher page never reported media is swept
/// after this long (the streamer closed the modal and walked away).
pub const PENDING_TTL: Duration = Duration::from_secs(15 * 60);
/// Publisher heartbeat TTL: a go-live page that stops reporting for this
/// long is treated as disconnected and the stream enters grace.
pub const PUBLISHER_TTL: Duration = Duration::from_secs(30);
/// Publisher-gone grace: a disconnect or refresh keeps the stream registered
/// this long before teardown, so a page reload does not kill the stream.
pub const PUBLISHER_GRACE: Duration = Duration::from_secs(30);
/// Watch pages heartbeat while the tab is open; a watcher silent for this
/// long stops counting.
pub const WATCHER_TTL: Duration = Duration::from_secs(45);

/// Lifecycle of one registered stream. `Pending` is the window between
/// `/golive` and the page's first media report; the announcement fires only
/// on the `Pending -> Live` transition (no "live" lines pointing at black
/// screens). `Grace` is a publisher disconnect waiting out
/// [`PUBLISHER_GRACE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamPhase {
    Pending,
    Live,
    Grace,
}

struct StreamEntry {
    user_id: Uuid,
    username: String,
    title: String,
    room_id: Uuid,
    voice_channel_id: Uuid,
    stream_id: String,
    publish_token: String,
    phase: StreamPhase,
    created_at: Instant,
    last_publisher_report: Instant,
    grace_since: Option<Instant>,
    announced: bool,
    /// The go-live page's self-reported browser-mic state. Feeds the "on
    /// air" line in the room's voice display so a browser-mic streamer is
    /// never an invisible speaker. Nothing detects anything: the page
    /// reports its own state.
    mic_on_air: bool,
    watchers: HashMap<String, Instant>,
}

impl StreamEntry {
    fn view(&self) -> LiveStreamView {
        LiveStreamView {
            user_id: self.user_id,
            username: self.username.clone(),
            title: self.title.clone(),
            room_id: self.room_id,
            voice_channel_id: self.voice_channel_id,
            stream_id: self.stream_id.clone(),
            live: self.phase != StreamPhase::Pending,
            mic_on_air: self.mic_on_air,
            watching: self.watchers.len(),
            watch_url: String::new(),
        }
    }
}

/// One stream as the TUI and the watch/publisher pages see it. `live` is
/// false only while pending (grace counts as live: the room row should not
/// flicker out on a page refresh).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveStreamView {
    pub user_id: Uuid,
    pub username: String,
    pub title: String,
    pub room_id: Uuid,
    pub voice_channel_id: Uuid,
    pub stream_id: String,
    pub live: bool,
    pub mic_on_air: bool,
    pub watching: usize,
    /// Public watch-page URL. The registry does not know the web base URL,
    /// so it leaves this empty; `App::tick_stream` fills it from
    /// `StreamService::watch_url` before the copy reaches any UI.
    pub watch_url: String,
}

/// Point-in-time view of every registered stream (pending ones included:
/// the rail's "stream" section lists a stream from `/golive` on, while the
/// #lounge announcement and the LIVE tag wait for `live`), delivered via
/// `watch` so `App::tick` reads local memory only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamSnapshot {
    pub streams: Vec<LiveStreamView>,
}

impl StreamSnapshot {
    pub fn for_user(&self, user_id: Uuid) -> Option<&LiveStreamView> {
        self.streams.iter().find(|view| view.user_id == user_id)
    }

    pub fn for_room(&self, room_id: Uuid) -> Option<&LiveStreamView> {
        self.streams.iter().find(|view| view.room_id == room_id)
    }
}

/// What `/golive` handed back: the capability ids behind the one-time
/// publisher URL and the watch URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamHandles {
    pub stream_id: String,
    pub publish_token: String,
    pub room_id: Uuid,
    pub voice_channel_id: Uuid,
}

/// Everything the publisher grant endpoint needs to mint a publish ticket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherInfo {
    pub user_id: Uuid,
    pub username: String,
    pub title: String,
    pub voice_channel_id: Uuid,
    pub stream_id: String,
}

/// Outcome of a publisher page state report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublisherReport {
    /// Unknown publish token: the stream ended or never existed.
    Gone,
    /// Accepted; media is (still) flowing.
    Live {
        /// True exactly once per registered stream: the `Pending -> Live`
        /// transition the "went live" feed line hangs off.
        went_live: bool,
    },
    /// Accepted; the page reported it stopped publishing (grace starts).
    Stopped,
}

#[derive(Clone)]
pub struct StreamRegistry {
    inner: Arc<Mutex<HashMap<Uuid, StreamEntry>>>,
    tx: watch::Sender<StreamSnapshot>,
}

impl Default for StreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamRegistry {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(StreamSnapshot::default());
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            tx,
        }
    }

    pub fn snapshot(&self) -> StreamSnapshot {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<StreamSnapshot> {
        self.tx.subscribe()
    }

    /// Register (or re-surface) the user's stream. One stream per user: a
    /// second `/golive` while one is registered updates the title and hands
    /// back the same capability ids, so re-running the command re-shows the
    /// modal instead of minting a parallel stream.
    pub fn begin(
        &self,
        user_id: Uuid,
        username: &str,
        title: &str,
        room_id: Uuid,
        voice_channel_id: Uuid,
    ) -> StreamHandles {
        let handles = {
            let mut inner = self.inner.lock_recover();
            let now = Instant::now();
            let entry = inner.entry(user_id).or_insert_with(|| StreamEntry {
                user_id,
                username: username.to_string(),
                title: title.to_string(),
                room_id,
                voice_channel_id,
                stream_id: capability_id(),
                publish_token: capability_id(),
                phase: StreamPhase::Pending,
                created_at: now,
                last_publisher_report: now,
                grace_since: None,
                announced: false,
                watchers: HashMap::new(),
                mic_on_air: false,
            });
            entry.title = title.to_string();
            entry.username = username.to_string();
            StreamHandles {
                stream_id: entry.stream_id.clone(),
                publish_token: entry.publish_token.clone(),
                room_id: entry.room_id,
                voice_channel_id: entry.voice_channel_id,
            }
        };
        self.publish();
        handles
    }

    /// Tear the user's stream down now (`/golive stop`). Returns whether a
    /// stream existed.
    pub fn end_for_user(&self, user_id: Uuid) -> bool {
        let removed = self.inner.lock_recover().remove(&user_id).is_some();
        if removed {
            self.publish();
        }
        removed
    }

    /// Resolve a publish token to what the grant endpoint needs. Valid for
    /// the whole stream lifetime (a page refresh re-fetches it), dead as
    /// soon as the stream is gone.
    pub fn publisher_info(&self, publish_token: &str) -> Option<PublisherInfo> {
        let inner = self.inner.lock_recover();
        inner
            .values()
            .find(|entry| entry.publish_token == publish_token)
            .map(|entry| PublisherInfo {
                user_id: entry.user_id,
                username: entry.username.clone(),
                title: entry.title.clone(),
                voice_channel_id: entry.voice_channel_id,
                stream_id: entry.stream_id.clone(),
            })
    }

    /// Apply a go-live page state report (media flowing or stopped, plus the
    /// page's own browser-mic state).
    pub fn report_publisher(
        &self,
        publish_token: &str,
        publishing: bool,
        mic_live: bool,
    ) -> PublisherReport {
        let outcome = {
            let mut inner = self.inner.lock_recover();
            let Some(entry) = inner
                .values_mut()
                .find(|entry| entry.publish_token == publish_token)
            else {
                return PublisherReport::Gone;
            };
            entry.last_publisher_report = Instant::now();
            entry.mic_on_air = mic_live;
            if publishing {
                let went_live = !entry.announced;
                entry.announced = true;
                entry.phase = StreamPhase::Live;
                entry.grace_since = None;
                PublisherReport::Live { went_live }
            } else {
                if entry.phase == StreamPhase::Live {
                    entry.phase = StreamPhase::Grace;
                    entry.grace_since = Some(Instant::now());
                }
                PublisherReport::Stopped
            }
        };
        self.publish();
        outcome
    }

    /// Resolve a watch-URL stream id. `None` once the stream is gone: watch
    /// URLs die with the stream.
    pub fn watch_view(&self, stream_id: &str) -> Option<LiveStreamView> {
        let inner = self.inner.lock_recover();
        inner
            .values()
            .find(|entry| entry.stream_id == stream_id)
            .map(StreamEntry::view)
    }

    /// Record a watch-page heartbeat. Returns false when the stream is gone.
    pub fn watch_heartbeat(&self, stream_id: &str, watcher_id: &str) -> bool {
        let known = {
            let mut inner = self.inner.lock_recover();
            match inner
                .values_mut()
                .find(|entry| entry.stream_id == stream_id)
            {
                Some(entry) => {
                    entry
                        .watchers
                        .insert(watcher_id.to_string(), Instant::now());
                    true
                }
                None => false,
            }
        };
        if known {
            self.publish();
        }
        known
    }

    /// The live (or grace) stream of a user, by username, for `/watch @user`.
    pub fn stream_for_username(&self, username: &str) -> Option<LiveStreamView> {
        let inner = self.inner.lock_recover();
        inner
            .values()
            .find(|entry| entry.username.eq_ignore_ascii_case(username))
            .map(StreamEntry::view)
    }

    /// Expire pending streams nobody ever published to, move stale-publisher
    /// streams into grace, tear down streams whose grace ran out, and prune
    /// stale watchers. Run periodically from `main.rs`.
    pub fn sweep(&self) {
        let changed = {
            let mut inner = self.inner.lock_recover();
            let now = Instant::now();
            let before = inner.len();
            let mut changed = false;
            for entry in inner.values_mut() {
                let stale_watchers = entry
                    .watchers
                    .values()
                    .any(|at| now.duration_since(*at) >= WATCHER_TTL);
                if stale_watchers {
                    entry
                        .watchers
                        .retain(|_, at| now.duration_since(*at) < WATCHER_TTL);
                    changed = true;
                }
                if entry.phase == StreamPhase::Live
                    && now.duration_since(entry.last_publisher_report) >= PUBLISHER_TTL
                {
                    entry.phase = StreamPhase::Grace;
                    entry.grace_since = Some(now);
                    changed = true;
                }
            }
            inner.retain(|_, entry| match entry.phase {
                StreamPhase::Pending => now.duration_since(entry.created_at) < PENDING_TTL,
                StreamPhase::Live => true,
                StreamPhase::Grace => entry
                    .grace_since
                    .is_none_or(|since| now.duration_since(since) < PUBLISHER_GRACE),
            });
            changed || inner.len() != before
        };
        if changed {
            self.publish();
        }
    }

    fn publish(&self) {
        let snapshot = {
            let inner = self.inner.lock_recover();
            let mut streams: Vec<LiveStreamView> = inner.values().map(StreamEntry::view).collect();
            streams.sort_by(|a, b| {
                a.username
                    .to_ascii_lowercase()
                    .cmp(&b.username.to_ascii_lowercase())
                    .then_with(|| a.user_id.cmp(&b.user_id))
            });
            StreamSnapshot { streams }
        };
        // `send` refuses to store when no receiver exists yet; the snapshot
        // must stay readable through `snapshot()` regardless.
        self.tx.send_replace(snapshot);
    }
}

/// Random 128-bit capability id for publisher/watch URLs. Unguessable is the
/// whole access model (unlisted, not public, not authed), so this must stay
/// a full-entropy random id, never derived from user or room ids.
fn capability_id() -> String {
    Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
#[path = "registry_test.rs"]
mod registry_test;
