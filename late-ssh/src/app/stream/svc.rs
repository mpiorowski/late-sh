//! Stream orchestration: the I/O around [`super::registry::StreamRegistry`].
//! Owns the lazy `#<user>-live` room creation, LiveKit ticket minting via
//! `VoiceService`, the go-live announcement, and the event channel back to
//! sessions. late-ssh moves tokens, registry state, and one activity line;
//! media never touches this crate.

use anyhow::Context;
use late_core::{
    db::Db,
    models::{
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        stream_ban::StreamBan,
        voice_channel::{TARGET_CHAT_ROOM, VoiceChannel},
    },
};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    stream::registry::{
        BeginObsOutcome, BeginOutcome, EndReason, EndedStream, LiveStreamView, ObsIngress,
        PublisherAccess, PublisherReport, StreamRegistry, StreamSnapshot,
    },
    voice::svc::{StreamMediaTicket, VoiceService},
};

/// Result of a `/golive`, delivered back to the session via the event
/// channel (room creation is DB work, so the command cannot answer inline).
#[derive(Clone, Debug)]
pub enum StreamEvent {
    GoLiveReady {
        user_id: Uuid,
        title: String,
        publish_url: String,
        watch_url: String,
        room_id: Uuid,
    },
    /// `/golive obs`: the stream is registered and OBS connection details
    /// are ready to show. OBS pushes to `whip_url` with `stream_key` as the
    /// bearer token; going live waits for the ingress status poll to see
    /// media, same contract as the console's first media report.
    GoLiveObsReady {
        user_id: Uuid,
        title: String,
        whip_url: String,
        stream_key: String,
        watch_url: String,
        room_id: Uuid,
    },
    GoLiveFailed {
        user_id: Uuid,
        message: String,
    },
    /// A named late.sh user arrived at `streamer_id`'s stream. Broadcast to
    /// every session; only the streamer's own acts on it (banner + desktop
    /// notification). Fired once per viewer per stream, so it cannot become
    /// a notification drip.
    ViewerJoined {
        streamer_id: Uuid,
        viewer_username: String,
    },
}

/// Outcome of the shared `/golive` groundwork (ban gates, room + voice
/// channel ensured, streamer joined), before any publisher-specific step.
enum GoLivePrep {
    Ready {
        room_id: Uuid,
        voice_channel_id: Uuid,
    },
    Refused {
        message: String,
    },
}

/// Everything the go-live page needs from its grant fetch.
#[derive(Clone, Debug)]
pub struct PublishGrant {
    pub ticket: StreamMediaTicket,
    pub title: String,
    pub username: String,
    pub stream_id: String,
    pub watch_url: String,
}

/// Outcome of a claim-checked publish grant request.
#[derive(Clone, Debug)]
pub enum PublishGrantAccess {
    /// Dead URL: the stream ended or never existed.
    Gone,
    /// The token is claimed by another console; no grant.
    Denied,
    Granted {
        grant: PublishGrant,
        /// `Some` on the claiming (first) fetch; the API layer hands it to
        /// the console as a cookie via the late-web proxy.
        new_claim: Option<String>,
    },
}

const STREAM_EVENT_CAP: usize = 64;

#[derive(Clone)]
pub struct StreamService {
    registry: StreamRegistry,
    db: Db,
    voice: VoiceService,
    activity: ActivityPublisher,
    /// Public late-web base URL (`Config::web_url`); publisher and watch
    /// URLs hang off it.
    web_url: String,
    evt_tx: broadcast::Sender<StreamEvent>,
}

impl StreamService {
    pub fn new(db: Db, voice: VoiceService, activity: ActivityPublisher, web_url: String) -> Self {
        let (evt_tx, _) = broadcast::channel(STREAM_EVENT_CAP);
        Self {
            registry: StreamRegistry::new(),
            db,
            voice,
            activity,
            web_url: web_url.trim_end_matches('/').to_string(),
            evt_tx,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<StreamEvent> {
        self.evt_tx.subscribe()
    }

    pub fn snapshot(&self) -> StreamSnapshot {
        self.registry.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<StreamSnapshot> {
        self.registry.subscribe()
    }

    pub fn watch_url(&self, stream_id: &str) -> String {
        format!("{}/live/{stream_id}", self.web_url)
    }

    fn publish_url(&self, publish_token: &str) -> String {
        format!("{}/golive/{publish_token}", self.web_url)
    }

    /// `/golive <title>`: lazily create/reuse the streamer's permanent
    /// stream room (chat history persists between streams), make sure the
    /// streamer is a member, register the stream, and hand the session its
    /// publisher URL. Errors come back as `GoLiveFailed` banners; this is
    /// the one place the whole flow's failure modes are logged.
    pub fn go_live_task(&self, user_id: Uuid, username: String, title: Option<String>) {
        let service = self.clone();
        let span = info_span!("stream.go_live_task", user_id = %user_id, username = %username);
        tokio::spawn(
            async move {
                let title = title.unwrap_or_default();
                let event = service.go_live(user_id, &username, &title).await;
                service.send_go_live_event(user_id, event);
            }
            .instrument(span),
        );
    }

    /// `/golive obs [title]`: same registration flow, but the media comes
    /// from OBS through a WHIP ingress instead of a browser console.
    pub fn go_live_obs_task(&self, user_id: Uuid, username: String, title: Option<String>) {
        let service = self.clone();
        let span = info_span!("stream.go_live_obs_task", user_id = %user_id, username = %username);
        tokio::spawn(
            async move {
                let title = title.unwrap_or_default();
                let event = service.go_live_obs(user_id, &username, &title).await;
                service.send_go_live_event(user_id, event);
            }
            .instrument(span),
        );
    }

    /// The one place a `/golive` outcome is logged and delivered, whatever
    /// the publisher kind.
    fn send_go_live_event(&self, user_id: Uuid, event: anyhow::Result<StreamEvent>) {
        match event {
            Ok(event) => {
                match &event {
                    StreamEvent::GoLiveReady { room_id, .. } => {
                        tracing::info!(room_id = %room_id, "stream registered");
                    }
                    StreamEvent::GoLiveObsReady { room_id, .. } => {
                        tracing::info!(room_id = %room_id, "obs stream registered");
                    }
                    StreamEvent::GoLiveFailed { message, .. } => {
                        tracing::info!(user_id = %user_id, reason = %message, "go live refused");
                    }
                    // Never produced by a `/golive`; `note_viewer` owns it
                    // and logs its own line.
                    StreamEvent::ViewerJoined { .. } => {}
                }
                let _ = self.evt_tx.send(event);
            }
            Err(error) => {
                tracing::error!(error = ?error, user_id = %user_id, "go live failed");
                let _ = self.evt_tx.send(StreamEvent::GoLiveFailed {
                    user_id,
                    message: "Could not start the stream. Try again.".to_string(),
                });
            }
        }
    }

    /// Shared `/golive` groundwork for both publisher kinds: ban gates, the
    /// permanent stream room, its voice channel, streamer membership.
    async fn prepare_go_live(&self, user_id: Uuid, username: &str) -> anyhow::Result<GoLivePrep> {
        if !self.voice.config().enabled {
            anyhow::bail!("voice/LiveKit is not configured");
        }
        if self.voice.is_blocked(user_id) {
            // Surface the block at command time: without this the stream
            // half-registers (room, rail row) and only the page's grant
            // fetch fails, as a generic error.
            return Ok(GoLivePrep::Refused {
                message: "You have been removed from voice by a moderator.".to_string(),
            });
        }
        let client = self.db.get().await.context("getting db client")?;
        if StreamBan::is_active_for_user(&client, user_id)
            .await
            .context("checking stream ban")?
        {
            return Ok(GoLivePrep::Refused {
                message: "A moderator has blocked you from streaming.".to_string(),
            });
        }
        let room = ChatRoom::get_or_create_stream_room(&client, username, user_id)
            .await
            .context("ensuring stream room")?;
        let voice_channel = VoiceChannel::upsert_for_target(
            &client,
            TARGET_CHAT_ROOM,
            room.id,
            &format!("{username} live"),
            true,
        )
        .await
        .context("ensuring stream voice channel")?;
        ChatRoomMember::join(&client, room.id, user_id)
            .await
            .context("joining streamer to stream room")?;
        Ok(GoLivePrep::Ready {
            room_id: room.id,
            voice_channel_id: voice_channel.id,
        })
    }

    async fn go_live(
        &self,
        user_id: Uuid,
        username: &str,
        title: &str,
    ) -> anyhow::Result<StreamEvent> {
        let (room_id, voice_channel_id) = match self.prepare_go_live(user_id, username).await? {
            GoLivePrep::Ready {
                room_id,
                voice_channel_id,
            } => (room_id, voice_channel_id),
            GoLivePrep::Refused { message } => {
                return Ok(StreamEvent::GoLiveFailed { user_id, message });
            }
        };
        match self
            .registry
            .begin(user_id, username, title, room_id, voice_channel_id)
        {
            BeginOutcome::Ready(handles) => Ok(StreamEvent::GoLiveReady {
                user_id,
                title: title.to_string(),
                publish_url: self.publish_url(&handles.publish_token),
                watch_url: self.watch_url(&handles.stream_id),
                room_id,
            }),
            BeginOutcome::PublisherConflict => Ok(StreamEvent::GoLiveFailed {
                user_id,
                message: "You are set up to stream from OBS. Run /golive stop first.".to_string(),
            }),
        }
    }

    async fn go_live_obs(
        &self,
        user_id: Uuid,
        username: &str,
        title: &str,
    ) -> anyhow::Result<StreamEvent> {
        let (room_id, voice_channel_id) = match self.prepare_go_live(user_id, username).await? {
            GoLivePrep::Ready {
                room_id,
                voice_channel_id,
            } => (room_id, voice_channel_id),
            GoLivePrep::Refused { message } => {
                return Ok(StreamEvent::GoLiveFailed { user_id, message });
            }
        };
        // Reuse the stored ingress on a re-run so the same connection
        // details re-show; mint one only when none is registered.
        let (ingress, minted) = match self.registry.obs_ingress(user_id) {
            Some(existing) => (existing, false),
            None => {
                let created = self
                    .voice
                    .create_whip_ingress(voice_channel_id, user_id, username)
                    .await
                    .context("creating WHIP ingress")?;
                (
                    ObsIngress {
                        ingress_id: created.ingress_id,
                        whip_url: created.url,
                        stream_key: created.stream_key,
                    },
                    true,
                )
            }
        };
        match self.registry.begin_obs(
            user_id,
            username,
            title,
            room_id,
            voice_channel_id,
            ingress.clone(),
        ) {
            BeginObsOutcome::Ready {
                handles,
                ingress: stored,
            } => {
                if minted && stored.ingress_id != ingress.ingress_id {
                    // Lost a same-user race; the stored ingress wins and the
                    // duplicate must not stay a valid stream key.
                    self.delete_ingress_task(ingress.ingress_id);
                }
                Ok(StreamEvent::GoLiveObsReady {
                    user_id,
                    title: title.to_string(),
                    whip_url: stored.whip_url,
                    stream_key: stored.stream_key,
                    watch_url: self.watch_url(&handles.stream_id),
                    room_id,
                })
            }
            BeginObsOutcome::PublisherConflict => {
                if minted {
                    // The refused begin must not leave a live stream key
                    // pointed at the conflicting stream's room.
                    self.delete_ingress_task(ingress.ingress_id);
                }
                Ok(StreamEvent::GoLiveFailed {
                    user_id,
                    message: "You are streaming from the browser console. Run /golive stop first."
                        .to_string(),
                })
            }
        }
    }

    /// `/golive stop` (also fired by moderation): tear the stream down now
    /// and force-disconnect the go-live console from LiveKit. Returns
    /// whether one existed.
    pub fn stop(&self, user_id: Uuid, reason: EndReason) -> bool {
        match self.registry.end_for_user(user_id, reason) {
            Some(ended) => {
                self.end_stream_task(ended);
                true
            }
            None => false,
        }
    }

    /// The one funnel every teardown passes through, so the whole story of a
    /// stream's death is auditable from one log line: why it ended, whether
    /// it ever went live, and how stale the console's last report was (the
    /// field that separates "the page reported a stop" from "the page went
    /// silent"). Then the media dies: registry removal kills the capability
    /// URLs only, not an already-connected go-live page.
    ///
    /// The disconnect is fire and forget, so it logs its own failure: the
    /// page also stops itself when its next state report 404s, so a failed
    /// removal degrades to a short delay instead of an endless broadcast.
    /// Debug level because a pending stream's console may never have
    /// connected at all.
    fn end_stream_task(&self, ended: EndedStream) {
        tracing::info!(
            user_id = %ended.user_id,
            username = %ended.username,
            reason = ended.reason.as_str(),
            phase = ?ended.phase,
            went_live = ended.announced,
            watching = ended.watching,
            since_publisher_report_ms = ended.since_publisher_report.as_millis() as u64,
            "stream ended"
        );
        // An OBS stream's ingress dies with the stream: the participant
        // removal below only cuts the current session, while the stream key
        // stays valid and OBS auto-reconnects through it.
        if let Some(ingress_id) = &ended.ingress_id {
            self.delete_ingress_task(ingress_id.clone());
        }
        let voice = self.voice.clone();
        tokio::spawn(async move {
            if let Err(error) = voice
                .remove_stream_publisher(ended.voice_channel_id, ended.user_id)
                .await
            {
                tracing::debug!(
                    error = ?error,
                    user_id = %ended.user_id,
                    "stream publisher disconnect failed; page stops itself on the next report"
                );
            }
        });
    }

    /// Fire-and-forget ingress deletion, so it logs its own failures. A
    /// leaked ingress is a stream key OBS keeps publishing through, and OBS
    /// has no page that stops itself the way the go-live console does, so a
    /// transient LiveKit failure gets a few retries before giving up. The
    /// boot reconciliation pass ([`Self::reconcile_ingresses`]) is the
    /// backstop for an outage that outlasts them.
    fn delete_ingress_task(&self, ingress_id: String) {
        let voice = self.voice.clone();
        tokio::spawn(async move {
            let backoff = [
                std::time::Duration::from_secs(5),
                std::time::Duration::from_secs(30),
            ];
            let mut attempt = 1;
            loop {
                let error = match voice.delete_ingress(&ingress_id).await {
                    Ok(()) => return,
                    Err(error) => error,
                };
                match backoff.get(attempt - 1) {
                    Some(delay) => {
                        tracing::debug!(error = ?error, ingress_id = %ingress_id, attempt, "stream ingress delete failed; retrying");
                        tokio::time::sleep(*delay).await;
                        attempt += 1;
                    }
                    None => {
                        tracing::warn!(error = ?error, ingress_id = %ingress_id, attempts = attempt, "stream ingress delete failed; orphan until the next boot reconciliation");
                        return;
                    }
                }
            }
        });
    }

    /// Boot-time reconciliation. A WHIP ingress is a LiveKit-side resource
    /// whose stream key outlives this process, while the registry that owns
    /// it is process memory: after a restart, every ingress LiveKit still
    /// holds is an orphan (a key OBS can publish through with all app-side
    /// trace of the stream gone, which breaks the no-invisible-speaker
    /// invariant). Delete every ingress the registry does not know.
    pub async fn reconcile_ingresses(&self) {
        if !self.voice.is_enabled() {
            return;
        }
        let ids = match self.voice.list_ingress_ids().await {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = ?error, "ingress reconciliation list failed");
                return;
            }
        };
        let known: std::collections::HashSet<String> = self
            .registry
            .obs_streams()
            .into_iter()
            .map(|poll| poll.ingress_id)
            .collect();
        for ingress_id in ids {
            if known.contains(&ingress_id) {
                continue;
            }
            tracing::info!(ingress_id = %ingress_id, "deleting orphaned stream ingress");
            self.delete_ingress_task(ingress_id);
        }
    }

    /// Resolve a publisher URL token into LiveKit connection details for the
    /// go-live page. Claim-once: the first fetch locks the token to that
    /// console (the returned secret rides back as a cookie), so a leaked
    /// publish URL cannot mint LiveKit tokens once the real console is up.
    pub fn publisher_grant(
        &self,
        publish_token: &str,
        presented_claim: Option<&str>,
    ) -> anyhow::Result<PublishGrantAccess> {
        let (info, new_claim) = match self
            .registry
            .access_publisher(publish_token, presented_claim)
        {
            PublisherAccess::Gone => return Ok(PublishGrantAccess::Gone),
            PublisherAccess::Denied => return Ok(PublishGrantAccess::Denied),
            PublisherAccess::Granted { info, new_claim } => (info, new_claim),
        };
        let ticket = self.voice.stream_publish_ticket(
            info.voice_channel_id,
            info.user_id,
            &info.username,
        )?;
        Ok(PublishGrantAccess::Granted {
            grant: PublishGrant {
                ticket,
                title: info.title,
                username: info.username,
                stream_id: info.stream_id.clone(),
                watch_url: self.watch_url(&info.stream_id),
            },
            new_claim,
        })
    }

    /// Go-live page state report. The first "media flowing" report fires the
    /// one #lounge announcement; a stop starts the grace timer. The API
    /// layer maps `Gone` to 404 and `Denied` (claimed token, wrong or
    /// missing secret) to 403.
    pub fn report_publisher(
        &self,
        publish_token: &str,
        publishing: bool,
        presented_claim: Option<&str>,
    ) -> PublisherReport {
        let info = self.registry.publisher_info(publish_token);
        let outcome = self
            .registry
            .report_publisher(publish_token, publishing, presented_claim);
        if let PublisherReport::Live { went_live: true } = outcome
            && let Some(info) = info
        {
            tracing::info!(user_id = %info.user_id, "stream went live");
            let title = Some(info.title).filter(|title| !title.trim().is_empty());
            self.activity.went_live_task(info.user_id, title);
        }
        outcome
    }

    /// Watch-page state by stream id. `None` once the stream ends: watch
    /// URLs are per-stream capabilities and die with it.
    pub fn watch_view(&self, stream_id: &str) -> Option<LiveStreamView> {
        self.registry.watch_view(stream_id)
    }

    /// Subscribe-only LiveKit ticket for a watch page. Minted with a random
    /// anonymous identity per call; the grant physically cannot publish.
    /// Withheld while the stream is pending: nobody subscribes to the room's
    /// voice channel before media has actually flowed (grace still counts as
    /// live, so a page refresh mid-stream reconnects).
    /// The viewer's identity is derived from its (stable, client-generated)
    /// watcher id rather than minted fresh per call. A viewer whose media
    /// path keeps failing re-fetches a grant on every retry, and a random
    /// identity per fetch left a new ghost participant in the LiveKit room
    /// each time, with nothing tying the attempts to one person. Reusing
    /// the id also means a reconnect *replaces* the stale participant
    /// instead of racing it.
    pub fn watch_grant(
        &self,
        stream_id: &str,
        watcher_id: &str,
    ) -> anyhow::Result<Option<StreamMediaTicket>> {
        let Some(view) = self.registry.watch_view(stream_id) else {
            return Ok(None);
        };
        if !view.live {
            return Ok(None);
        }
        let identity = format!("viewer-{watcher_id}");
        let ticket = self
            .voice
            .stream_watch_ticket(view.voice_channel_id, &identity)?;
        Ok(Some(ticket))
    }

    pub fn watch_heartbeat(&self, stream_id: &str, watcher_id: &str) -> bool {
        self.registry.watch_heartbeat(stream_id, watcher_id)
    }

    /// `/watch @user`: the viewer is about to open the watch page.
    pub fn note_viewer_of_username(
        &self,
        streamer_username: &str,
        viewer_id: Uuid,
        viewer_username: &str,
    ) {
        let Some(view) = self.registry.stream_for_username(streamer_username) else {
            return;
        };
        self.note_viewer(view.user_id, viewer_id, viewer_username);
    }

    /// The other identified way in: the viewer opened the streamer's stream
    /// room from the rail (where the #lounge went-live line sends people).
    pub fn note_viewer_in_room(&self, room_id: Uuid, viewer_id: Uuid, viewer_username: &str) {
        let Some(view) = self.registry.snapshot().for_room(room_id).cloned() else {
            return;
        };
        self.note_viewer(view.user_id, viewer_id, viewer_username);
    }

    /// The one funnel both arrival paths share: the registry decides whether
    /// this is a first arrival, then the #lounge line and the streamer's
    /// notification fire together.
    fn note_viewer(&self, streamer_id: Uuid, viewer_id: Uuid, viewer_username: &str) {
        let Some(streamer_username) = self.registry.note_viewer(streamer_id, viewer_id) else {
            return;
        };
        tracing::info!(
            streamer_id = %streamer_id,
            viewer_id = %viewer_id,
            viewer = %viewer_username,
            "stream viewer joined"
        );
        self.activity
            .watching_stream_task(viewer_id, streamer_username);
        let _ = self.evt_tx.send(StreamEvent::ViewerJoined {
            streamer_id,
            viewer_username: viewer_username.to_string(),
        });
    }

    /// `/watch @user`: the live stream's watch URL, if that user is live.
    pub fn watch_url_for_username(&self, username: &str) -> Option<String> {
        let view = self.registry.stream_for_username(username)?;
        view.live.then(|| self.watch_url(&view.stream_id))
    }

    /// Ingress status poll for OBS streams; run right before [`sweep`] from
    /// the same periodic task. The poll is the OBS analogue of the go-live
    /// page's state reports: `ENDPOINT_PUBLISHING` keeps the stream live
    /// (and fires the one #lounge announcement on the first hit), any other
    /// ingress state reports stopped and starts grace.
    pub async fn poll_obs_publishers(&self) {
        // The polls run concurrently and the HTTP client carries a request
        // timeout, so one slow LiveKit call delays the sweep behind this by
        // at most one timeout, not one per stream.
        let statuses = futures_util::future::join_all(self.registry.obs_streams().into_iter().map(
            |poll| async move {
                let publishing = self.voice.ingress_publishing(&poll.ingress_id).await;
                (poll, publishing)
            },
        ))
        .await;
        for (poll, publishing) in statuses {
            let publishing = match publishing {
                Ok(publishing) => publishing,
                Err(error) => {
                    // Skip, not stop: a flaky LiveKit API call must not
                    // shove a live stream into grace. A truly dead ingress
                    // is caught by the publisher TTL once reports stop
                    // landing.
                    tracing::debug!(
                        error = ?error,
                        user_id = %poll.user_id,
                        "ingress status poll failed"
                    );
                    continue;
                }
            };
            if let PublisherReport::Live { went_live: true } =
                self.registry.report_obs(poll.user_id, publishing)
            {
                tracing::info!(user_id = %poll.user_id, "stream went live via obs");
                let title = Some(poll.title).filter(|title| !title.trim().is_empty());
                self.activity.went_live_task(poll.user_id, title);
            }
        }
    }

    /// Registry hygiene pass; run periodically from `main.rs`. Streams the
    /// sweep tears down (pending TTL, grace ran out) get their publisher
    /// disconnected, same as an explicit stop.
    pub fn sweep(&self) {
        for ended in self.registry.sweep() {
            self.end_stream_task(ended);
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
