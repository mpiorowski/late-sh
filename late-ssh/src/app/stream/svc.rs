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
        voice_channel::{TARGET_CHAT_ROOM, VoiceChannel},
    },
};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    stream::registry::{LiveStreamView, PublisherReport, StreamRegistry, StreamSnapshot},
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
    GoLiveFailed {
        user_id: Uuid,
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

const STREAM_EVENT_CAP: usize = 64;

#[derive(Clone)]
pub struct StreamService {
    registry: StreamRegistry,
    db: Db,
    voice: VoiceService,
    activity: ActivityPublisher,
    /// Public late-web base URL (`LATE_WEB_URL`); publisher and watch URLs
    /// hang off it.
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
                match service.go_live(user_id, &username, &title).await {
                    Ok(event) => {
                        tracing::info!(room_id = %match &event {
                            StreamEvent::GoLiveReady { room_id, .. } => *room_id,
                            _ => Uuid::nil(),
                        }, "stream registered");
                        let _ = service.evt_tx.send(event);
                    }
                    Err(error) => {
                        tracing::error!(error = ?error, user_id = %user_id, "go live failed");
                        let _ = service.evt_tx.send(StreamEvent::GoLiveFailed {
                            user_id,
                            message: "Could not start the stream. Try again.".to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn go_live(
        &self,
        user_id: Uuid,
        username: &str,
        title: &str,
    ) -> anyhow::Result<StreamEvent> {
        if !self.voice.config().enabled {
            anyhow::bail!("voice/LiveKit is not configured");
        }
        let client = self.db.get().await.context("getting db client")?;
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

        let handles = self
            .registry
            .begin(user_id, username, title, room.id, voice_channel.id);
        Ok(StreamEvent::GoLiveReady {
            user_id,
            title: title.to_string(),
            publish_url: self.publish_url(&handles.publish_token),
            watch_url: self.watch_url(&handles.stream_id),
            room_id: room.id,
        })
    }

    /// `/golive stop`: tear the stream down now. Returns whether one existed.
    pub fn stop(&self, user_id: Uuid) -> bool {
        self.registry.end_for_user(user_id)
    }

    /// Resolve a publisher URL token into LiveKit connection details for the
    /// go-live page. `None` when the stream is gone (dead URL).
    pub fn publisher_grant(&self, publish_token: &str) -> anyhow::Result<Option<PublishGrant>> {
        let Some(info) = self.registry.publisher_info(publish_token) else {
            return Ok(None);
        };
        let ticket = self.voice.stream_publish_ticket(
            info.voice_channel_id,
            info.user_id,
            &info.username,
        )?;
        Ok(Some(PublishGrant {
            ticket,
            title: info.title,
            username: info.username,
            stream_id: info.stream_id.clone(),
            watch_url: self.watch_url(&info.stream_id),
        }))
    }

    /// Go-live page state report. The first "media flowing" report fires the
    /// one #lounge announcement; a stop starts the grace timer. Returns
    /// false when the stream is gone.
    pub fn report_publisher(&self, publish_token: &str, publishing: bool, mic_live: bool) -> bool {
        let info = self.registry.publisher_info(publish_token);
        match self
            .registry
            .report_publisher(publish_token, publishing, mic_live)
        {
            PublisherReport::Gone => false,
            PublisherReport::Live { went_live } => {
                if went_live && let Some(info) = info {
                    tracing::info!(user_id = %info.user_id, "stream went live");
                    let title = Some(info.title).filter(|title| !title.trim().is_empty());
                    self.activity.went_live_task(info.user_id, title);
                }
                true
            }
            PublisherReport::Stopped => true,
        }
    }

    /// Watch-page state by stream id. `None` once the stream ends: watch
    /// URLs are per-stream capabilities and die with it.
    pub fn watch_view(&self, stream_id: &str) -> Option<LiveStreamView> {
        self.registry.watch_view(stream_id)
    }

    /// Subscribe-only LiveKit ticket for a watch page. Minted with a random
    /// anonymous identity per call; the grant physically cannot publish.
    pub fn watch_grant(&self, stream_id: &str) -> anyhow::Result<Option<StreamMediaTicket>> {
        let Some(view) = self.registry.watch_view(stream_id) else {
            return Ok(None);
        };
        let identity = format!("viewer-{}", Uuid::new_v4().simple());
        let ticket = self
            .voice
            .stream_watch_ticket(view.voice_channel_id, &identity)?;
        Ok(Some(ticket))
    }

    pub fn watch_heartbeat(&self, stream_id: &str, watcher_id: &str) -> bool {
        self.registry.watch_heartbeat(stream_id, watcher_id)
    }

    /// `/watch @user`: the live stream's watch URL, if that user is live.
    pub fn watch_url_for_username(&self, username: &str) -> Option<String> {
        let view = self.registry.stream_for_username(username)?;
        view.live.then(|| self.watch_url(&view.stream_id))
    }

    /// Registry hygiene pass; run periodically from `main.rs`.
    pub fn sweep(&self) {
        self.registry.sweep();
    }
}
