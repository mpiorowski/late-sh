//! Process-global live-stream registry: one "watch me" stream per user,
//! in-memory only (scratchpad-registry tier: single replica, dies with the
//! process). late-ssh never touches a media byte; this registry only tracks
//! who is live, the capability ids behind the publisher/watch URLs, and the
//! watcher heartbeats behind the "12 watching" count. Media flows
//! browser/CLI -> LiveKit -> browser/CLI.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use late_core::MutexRecover;
use tokio::sync::watch;
use uuid::Uuid;

/// A registered stream whose publisher page never reported media is swept
/// after this long (the streamer closed the modal and walked away). Short
/// on purpose: a pending stream occupies a rail row, and `/golive` again is
/// cheap (idempotent, same room).
pub const PENDING_TTL: Duration = Duration::from_secs(5 * 60);
/// Publisher heartbeat TTL: a go-live page that stops reporting for this
/// long is treated as disconnected and the stream enters grace.
pub const PUBLISHER_TTL: Duration = Duration::from_secs(30);
/// Publisher-gone grace: a disconnect or refresh keeps the stream registered
/// this long before teardown, so a page reload does not kill the stream.
pub const PUBLISHER_GRACE: Duration = Duration::from_secs(30);
/// Watch pages heartbeat while the tab is open; a watcher silent for this
/// long stops counting.
pub const WATCHER_TTL: Duration = Duration::from_secs(45);
/// Hard cap on distinct watcher ids tracked per stream. The heartbeat
/// endpoint is unauthenticated (the watch URL is the auth), so both the
/// "N watching" count and registry memory must stay bounded no matter what
/// a link-holder posts; heartbeats for new ids beyond the cap are dropped.
pub const WATCHERS_MAX: usize = 500;
/// Watcher ids are client-generated (a browser UUID, 36 chars); anything
/// longer is rejected at the API boundary before it reaches the registry.
pub const WATCHER_ID_MAX_LEN: usize = 64;

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

/// How the stream's media reaches LiveKit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamPublisher {
    /// Browser go-live console: claim-once publish token, page state reports.
    Console,
    /// OBS pushing to a WHIP ingress; liveness comes from the server-side
    /// ingress status poll, not from any page.
    Obs(ObsIngress),
}

/// The WHIP ingress behind an OBS stream. Kept in the registry so a re-run
/// `/golive obs` re-shows the same connection details instead of minting a
/// second ingress, and so teardown knows what to delete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsIngress {
    pub ingress_id: String,
    pub whip_url: String,
    pub stream_key: String,
}

struct StreamEntry {
    user_id: Uuid,
    username: String,
    title: String,
    room_id: Uuid,
    voice_channel_id: Uuid,
    stream_id: String,
    publish_token: String,
    publisher: StreamPublisher,
    phase: StreamPhase,
    created_at: Instant,
    last_publisher_report: Instant,
    grace_since: Option<Instant>,
    announced: bool,
    /// Claim-once lock on the publish token. `None` until the first grant
    /// fetch; that fetch mints a secret the console keeps as a cookie, and
    /// every later grant fetch or state report must present it. A leaked
    /// publish URL is useless once the streamer's own console has claimed
    /// it; leaked *before* claiming, the intruder claims first and the real
    /// console fails loudly (403), so the hijack is visible, never silent.
    publisher_claim: Option<String>,
    watchers: HashMap<String, Instant>,
    /// Named late.sh users already announced for this stream. A different
    /// thing from `watchers`: those are anonymous browser ids behind the "N
    /// watching" count, with no user to name. Per stream, so the same
    /// regular is announced again at tomorrow's broadcast.
    viewers: HashSet<Uuid>,
}

/// A fresh `Pending` entry with newly minted capability ids.
fn new_entry(
    user_id: Uuid,
    username: &str,
    title: &str,
    room_id: Uuid,
    voice_channel_id: Uuid,
    publisher: StreamPublisher,
) -> StreamEntry {
    let now = Instant::now();
    StreamEntry {
        user_id,
        username: username.to_string(),
        title: title.to_string(),
        room_id,
        voice_channel_id,
        stream_id: capability_id(),
        publish_token: capability_id(),
        publisher,
        phase: StreamPhase::Pending,
        created_at: now,
        last_publisher_report: now,
        grace_since: None,
        announced: false,
        publisher_claim: None,
        watchers: HashMap::new(),
        viewers: HashSet::new(),
    }
}

impl StreamEntry {
    fn handles(&self) -> StreamHandles {
        StreamHandles {
            stream_id: self.stream_id.clone(),
            publish_token: self.publish_token.clone(),
            room_id: self.room_id,
            voice_channel_id: self.voice_channel_id,
        }
    }

    fn view(&self) -> LiveStreamView {
        LiveStreamView {
            user_id: self.user_id,
            username: self.username.clone(),
            title: self.title.clone(),
            room_id: self.room_id,
            voice_channel_id: self.voice_channel_id,
            stream_id: self.stream_id.clone(),
            live: self.phase != StreamPhase::Pending,
            watching: self.watchers.len(),
            watch_url: String::new(),
        }
    }

    fn ended(&self, reason: EndReason, now: Instant) -> EndedStream {
        EndedStream {
            user_id: self.user_id,
            username: self.username.clone(),
            voice_channel_id: self.voice_channel_id,
            ingress_id: match &self.publisher {
                StreamPublisher::Console => None,
                StreamPublisher::Obs(ingress) => Some(ingress.ingress_id.clone()),
            },
            reason,
            phase: self.phase,
            announced: self.announced,
            watching: self.watchers.len(),
            since_publisher_report: now.duration_since(self.last_publisher_report),
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

/// Outcome of registering a console stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeginOutcome {
    Ready(StreamHandles),
    /// The user's registered stream publishes through OBS; `/golive stop`
    /// first.
    PublisherConflict,
}

/// Outcome of registering an OBS stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeginObsOutcome {
    Ready {
        handles: StreamHandles,
        /// The ingress actually stored on the stream. On a re-run this is
        /// the existing one; a caller that just minted a duplicate compares
        /// ingress ids and deletes its own.
        ingress: ObsIngress,
    },
    /// The user's registered stream publishes through the browser console;
    /// `/golive stop` first.
    PublisherConflict,
}

/// One OBS stream as the ingress status poll sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsPublisherPoll {
    pub user_id: Uuid,
    pub title: String,
    pub ingress_id: String,
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

/// Why a stream left the registry. Carried on [`EndedStream`] so the
/// orchestration layer can log one line per teardown: the sweeper's two
/// reasons are invisible from the TUI, and "my stream ended and I don't know
/// why" is otherwise undiagnosable after the fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// `/golive stop` in the composer.
    Command,
    /// A moderation action (stream kick/ban, voice kick, server kick/ban).
    Moderation,
    /// Registered, but the go-live page never reported media
    /// ([`PENDING_TTL`]).
    PendingExpired,
    /// The console stopped reporting media and [`PUBLISHER_GRACE`] ran out.
    GraceExpired,
}

impl EndReason {
    /// Log label. Exhaustive: a new variant has to name itself here.
    pub fn as_str(self) -> &'static str {
        match self {
            EndReason::Command => "command",
            EndReason::Moderation => "moderation",
            EndReason::PendingExpired => "pending_expired",
            EndReason::GraceExpired => "grace_expired",
        }
    }
}

/// A stream the registry just tore down. The caller (StreamService) uses
/// this to force-disconnect the publisher's LiveKit session (registry
/// removal alone only kills the capability URLs, not an already-connected
/// go-live page) and to log the teardown. For an OBS stream, `ingress_id`
/// is the WHIP ingress to delete: without that, the stream key stays valid
/// and OBS reconnects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndedStream {
    pub user_id: Uuid,
    pub username: String,
    pub voice_channel_id: Uuid,
    pub ingress_id: Option<String>,
    pub reason: EndReason,
    /// Phase the stream was in when it left the registry.
    pub phase: StreamPhase,
    /// Whether the stream ever went live (the `Pending -> Live` edge fired).
    pub announced: bool,
    /// Watchers still counted at teardown.
    pub watching: usize,
    /// Age of the go-live page's last state report. The field that separates
    /// "the console reported a stop" from "the console went silent".
    pub since_publisher_report: Duration,
}

/// Outcome of resolving a publish token for the grant endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublisherAccess {
    /// Unknown publish token: the stream ended or never existed.
    Gone,
    /// The token is claimed by another console and the presented secret
    /// does not match.
    Denied,
    Granted {
        info: PublisherInfo,
        /// `Some` exactly once per stream: the first grant fetch mints the
        /// claim secret the console must present from then on.
        new_claim: Option<String>,
    },
}

/// Outcome of a publisher page state report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublisherReport {
    /// Unknown publish token: the stream ended or never existed.
    Gone,
    /// The token is claimed and the presented secret does not match.
    Denied,
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

    /// Register (or re-surface) the user's console stream. One stream per
    /// user: a second `/golive` while one is registered updates the title and
    /// hands back the same capability ids, so re-running the command re-shows
    /// the modal instead of minting a parallel stream. A registered OBS
    /// stream is a conflict: a live broadcast is never silently rewired to a
    /// different publisher.
    pub fn begin(
        &self,
        user_id: Uuid,
        username: &str,
        title: &str,
        room_id: Uuid,
        voice_channel_id: Uuid,
    ) -> BeginOutcome {
        let outcome = {
            let mut inner = self.inner.lock_recover();
            match inner.get_mut(&user_id) {
                Some(entry) => match &entry.publisher {
                    StreamPublisher::Obs(_) => return BeginOutcome::PublisherConflict,
                    StreamPublisher::Console => {
                        entry.title = title.to_string();
                        entry.username = username.to_string();
                        BeginOutcome::Ready(entry.handles())
                    }
                },
                None => {
                    let entry = new_entry(
                        user_id,
                        username,
                        title,
                        room_id,
                        voice_channel_id,
                        StreamPublisher::Console,
                    );
                    let handles = entry.handles();
                    inner.insert(user_id, entry);
                    BeginOutcome::Ready(handles)
                }
            }
        };
        self.publish();
        outcome
    }

    /// Register (or re-surface) the user's OBS stream. On a re-run the
    /// stored ingress wins and is handed back so the same connection details
    /// re-show; the caller compares ingress ids and deletes a freshly minted
    /// duplicate. A registered console stream is a conflict, mirroring
    /// [`begin`](Self::begin).
    pub fn begin_obs(
        &self,
        user_id: Uuid,
        username: &str,
        title: &str,
        room_id: Uuid,
        voice_channel_id: Uuid,
        ingress: ObsIngress,
    ) -> BeginObsOutcome {
        let outcome = {
            let mut inner = self.inner.lock_recover();
            match inner.get_mut(&user_id) {
                Some(entry) => match &entry.publisher {
                    StreamPublisher::Console => return BeginObsOutcome::PublisherConflict,
                    StreamPublisher::Obs(existing) => {
                        let existing = existing.clone();
                        entry.title = title.to_string();
                        entry.username = username.to_string();
                        BeginObsOutcome::Ready {
                            handles: entry.handles(),
                            ingress: existing,
                        }
                    }
                },
                None => {
                    let entry = new_entry(
                        user_id,
                        username,
                        title,
                        room_id,
                        voice_channel_id,
                        StreamPublisher::Obs(ingress.clone()),
                    );
                    let handles = entry.handles();
                    inner.insert(user_id, entry);
                    BeginObsOutcome::Ready { handles, ingress }
                }
            }
        };
        self.publish();
        outcome
    }

    /// The stored ingress of the user's registered OBS stream, if any. Lets
    /// `/golive obs` re-runs skip minting a duplicate ingress.
    pub fn obs_ingress(&self, user_id: Uuid) -> Option<ObsIngress> {
        let inner = self.inner.lock_recover();
        inner
            .get(&user_id)
            .and_then(|entry| match &entry.publisher {
                StreamPublisher::Console => None,
                StreamPublisher::Obs(ingress) => Some(ingress.clone()),
            })
    }

    /// Every registered OBS stream, for the ingress status poll.
    pub fn obs_streams(&self) -> Vec<ObsPublisherPoll> {
        let inner = self.inner.lock_recover();
        inner
            .values()
            .filter_map(|entry| match &entry.publisher {
                StreamPublisher::Console => None,
                StreamPublisher::Obs(ingress) => Some(ObsPublisherPoll {
                    user_id: entry.user_id,
                    title: entry.title.clone(),
                    ingress_id: ingress.ingress_id.clone(),
                }),
            })
            .collect()
    }

    /// Apply an ingress status poll result to the user's OBS stream. Mirrors
    /// [`report_publisher`](Self::report_publisher) without the claim
    /// machinery: the server itself is the reporter, there is no page.
    /// `Gone` covers a stream that ended (or switched publisher) since the
    /// poll snapshot was taken.
    pub fn report_obs(&self, user_id: Uuid, publishing: bool) -> PublisherReport {
        let outcome = {
            let mut inner = self.inner.lock_recover();
            let Some(entry) = inner
                .get_mut(&user_id)
                .filter(|entry| matches!(entry.publisher, StreamPublisher::Obs(_)))
            else {
                return PublisherReport::Gone;
            };
            entry.last_publisher_report = Instant::now();
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

    /// Tear the user's stream down now (`/golive stop`, moderation kick).
    /// Returns the ended stream so the caller can disconnect the publisher's
    /// LiveKit session and log the teardown, `None` when no stream existed.
    pub fn end_for_user(&self, user_id: Uuid, reason: EndReason) -> Option<EndedStream> {
        let removed = self.inner.lock_recover().remove(&user_id);
        let ended = removed.map(|entry| entry.ended(reason, Instant::now()));
        if ended.is_some() {
            self.publish();
        }
        ended
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

    /// Claim-once resolution of a publish token for the grant endpoint. The
    /// first caller claims the token and gets the minted secret back; from
    /// then on only callers presenting that secret are granted. The claim
    /// dies with the stream (a fresh `/golive` after a stop mints new
    /// capability ids and starts unclaimed).
    pub fn access_publisher(
        &self,
        publish_token: &str,
        presented_claim: Option<&str>,
    ) -> PublisherAccess {
        let mut inner = self.inner.lock_recover();
        let Some(entry) = inner
            .values_mut()
            .find(|entry| entry.publish_token == publish_token)
        else {
            return PublisherAccess::Gone;
        };
        let new_claim = match (&entry.publisher_claim, presented_claim) {
            (None, _) => {
                let secret = capability_id();
                entry.publisher_claim = Some(secret.clone());
                Some(secret)
            }
            (Some(claim), Some(presented)) if claim == presented => None,
            (Some(_), _) => return PublisherAccess::Denied,
        };
        PublisherAccess::Granted {
            info: PublisherInfo {
                user_id: entry.user_id,
                username: entry.username.clone(),
                title: entry.title.clone(),
                voice_channel_id: entry.voice_channel_id,
                stream_id: entry.stream_id.clone(),
            },
            new_claim,
        }
    }

    /// Apply a go-live page state report (media flowing or stopped). Once
    /// the token is claimed, reports must present the claim secret: a bare
    /// URL-holder must not be able to shove a live stream into grace with a
    /// forged `publishing: false`.
    pub fn report_publisher(
        &self,
        publish_token: &str,
        publishing: bool,
        presented_claim: Option<&str>,
    ) -> PublisherReport {
        let outcome = {
            let mut inner = self.inner.lock_recover();
            let Some(entry) = inner
                .values_mut()
                .find(|entry| entry.publish_token == publish_token)
            else {
                return PublisherReport::Gone;
            };
            if let Some(claim) = &entry.publisher_claim
                && presented_claim != Some(claim.as_str())
            {
                return PublisherReport::Denied;
            }
            entry.last_publisher_report = Instant::now();
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
    /// New watcher ids past [`WATCHERS_MAX`] are dropped (known ids still
    /// refresh), and only a count change republishes the snapshot, so a
    /// heartbeat flood can neither grow memory nor spam every session.
    pub fn watch_heartbeat(&self, stream_id: &str, watcher_id: &str) -> bool {
        let (known, count_changed) = {
            let mut inner = self.inner.lock_recover();
            match inner
                .values_mut()
                .find(|entry| entry.stream_id == stream_id)
            {
                Some(entry) => {
                    let seen_before = entry.watchers.contains_key(watcher_id);
                    let admitted = seen_before || entry.watchers.len() < WATCHERS_MAX;
                    if admitted {
                        entry
                            .watchers
                            .insert(watcher_id.to_string(), Instant::now());
                    }
                    (true, admitted && !seen_before)
                }
                None => (false, false),
            }
        };
        if count_changed {
            self.publish();
        }
        known
    }

    /// Record a named late.sh user arriving at a stream. Returns the
    /// streamer's username the first time this viewer shows up at this
    /// stream, `None` on a repeat visit, on the streamer opening their own
    /// room, and while the stream is still pending (no announcement ever
    /// points at a black screen). No `publish`: viewers are not part of the
    /// snapshot, so a room reopen costs nothing on the wire.
    pub fn note_viewer(&self, streamer_id: Uuid, viewer_id: Uuid) -> Option<String> {
        if streamer_id == viewer_id {
            return None;
        }
        let mut inner = self.inner.lock_recover();
        let entry = inner.get_mut(&streamer_id)?;
        if entry.phase == StreamPhase::Pending {
            return None;
        }
        entry
            .viewers
            .insert(viewer_id)
            .then(|| entry.username.clone())
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
    /// stale watchers. Run periodically from `main.rs`. Returns the streams
    /// torn down this pass so the caller can disconnect their publishers.
    pub fn sweep(&self) -> Vec<EndedStream> {
        self.sweep_at(Instant::now())
    }

    /// [`sweep`](Self::sweep) against an explicit clock, so the TTL
    /// transitions are testable without sleeping.
    pub fn sweep_at(&self, now: Instant) -> Vec<EndedStream> {
        let (changed, ended) = {
            let mut inner = self.inner.lock_recover();
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
            let expired: Vec<(Uuid, EndReason)> = inner
                .values()
                .filter_map(|entry| match entry.phase {
                    StreamPhase::Pending => (now.duration_since(entry.created_at) >= PENDING_TTL)
                        .then_some((entry.user_id, EndReason::PendingExpired)),
                    StreamPhase::Live => None,
                    StreamPhase::Grace => entry
                        .grace_since
                        .is_some_and(|since| now.duration_since(since) >= PUBLISHER_GRACE)
                        .then_some((entry.user_id, EndReason::GraceExpired)),
                })
                .collect();
            let ended: Vec<EndedStream> = expired
                .into_iter()
                .filter_map(|(user_id, reason)| {
                    inner.remove(&user_id).map(|entry| entry.ended(reason, now))
                })
                .collect();
            (changed || !ended.is_empty(), ended)
        };
        if changed {
            self.publish();
        }
        ended
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
/// a full-entropy random id, never derived from user or room ids. v4, not
/// the house-standard v7, on purpose: these are secrets, not row ids, and a
/// v7 would leak the stream's start time while carrying only ~74 random
/// bits against v4's 122.
fn capability_id() -> String {
    Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
#[path = "registry_test.rs"]
mod registry_test;
