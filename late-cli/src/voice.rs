#[cfg(any(target_os = "linux", target_os = "windows"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use tracing::{debug, info, warn};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use livekit::{
    PlatformAudio,
    options::TrackPublishOptions,
    prelude::{
        LocalAudioTrack, LocalTrack, LocalTrackPublication, RemoteTrack, Room, RoomEvent,
        RoomOptions, TrackSource,
    },
};

#[derive(Default)]
pub(super) struct VoiceRuntimeState {
    pub(super) joined: bool,
    pub(super) room: Option<String>,
    pub(super) muted: bool,
    pub(super) deafened: bool,
    pub(super) speaking: bool,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    media: Option<VoiceMediaSession>,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
struct VoiceMediaSession {
    room: Room,
    _audio: PlatformAudio,
    publication: LocalTrackPublication,
    disconnected: Arc<AtomicBool>,
    remote_playback_enabled: Arc<AtomicBool>,
    events_task: tokio::task::JoinHandle<()>,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl VoiceMediaSession {
    fn set_remote_playback_enabled(&self, enabled: bool) {
        self.remote_playback_enabled
            .store(enabled, Ordering::Relaxed);

        for participant in self.room.remote_participants().values() {
            for publication in participant.track_publications().values() {
                let Some(RemoteTrack::Audio(track)) = publication.track() else {
                    continue;
                };
                if enabled {
                    track.enable();
                } else {
                    track.disable();
                }
            }
        }
    }
}

impl VoiceRuntimeState {
    pub(super) async fn join(
        &mut self,
        room: String,
        url: String,
        token: String,
        muted: bool,
        deafened: bool,
    ) -> Result<()> {
        self.leave().await;

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let media = connect_voice_media(&room, &url, &token, muted).await?;
            self.media = Some(media);

            self.joined = true;
            self.room = Some(room);
            self.muted = false;
            self.deafened = false;
            self.speaking = false;
            self.set_muted(muted);
            self.set_deafened(deafened);

            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = (&room, &url, &token, muted, deafened);
            anyhow::bail!("voice media is not supported on this platform");
        }
    }

    pub(super) async fn leave(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if let Some(media) = self.media.take() {
            let VoiceMediaSession {
                room,
                _audio,
                publication: _,
                disconnected: _,
                remote_playback_enabled: _,
                events_task,
            } = media;
            if let Err(err) = room.close().await {
                warn!(error = ?err, "failed to close voice room cleanly");
            }
            events_task.abort();
        }

        self.joined = false;
        self.room = None;
        self.speaking = false;
    }

    pub(super) fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.speaking = false;

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if let Some(media) = self.media.as_ref() {
            if muted {
                media.publication.mute();
            } else {
                media.publication.unmute();
            }
        }
    }

    pub(super) fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
        self.speaking = false;

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if let Some(media) = self.media.as_ref() {
            media.set_remote_playback_enabled(!deafened);
        }
    }

    pub(super) fn media_disconnected(&self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.media
                .as_ref()
                .is_some_and(|media| media.disconnected.load(Ordering::Relaxed))
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            false
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn connect_voice_media(
    room_name: &str,
    url: &str,
    token: &str,
    muted: bool,
) -> Result<VoiceMediaSession> {
    let audio = PlatformAudio::new().context("failed to initialize voice audio devices")?;
    let recording_devices: Vec<_> = audio.recording_devices().collect();
    let recording_device = recording_devices
        .first()
        .context("no voice recording devices found")?;
    let recording_device_name = recording_device.name.clone();
    audio
        .set_recording_device(&recording_device.id)
        .with_context(|| format!("failed to select voice microphone {recording_device_name:?}"))?;

    let playout_device_name = audio.playout_devices().next().map(|device| {
        if let Err(err) = audio.set_playout_device(&device.id) {
            warn!(
                device = %device.name,
                error = ?err,
                "failed to select voice playout device; remote voice may be silent"
            );
        }
        device.name
    });

    let room_options = RoomOptions::default();
    let (room, mut events) = Room::connect(url, token, room_options)
        .await
        .with_context(|| format!("failed to connect voice room {room_name:?}"))?;
    let remote_playback_enabled = Arc::new(AtomicBool::new(true));
    let event_remote_playback_enabled = Arc::clone(&remote_playback_enabled);
    let disconnected = Arc::new(AtomicBool::new(false));
    let event_disconnected = Arc::clone(&disconnected);
    let events_task = tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                RoomEvent::Reconnecting => warn!("voice room reconnecting"),
                RoomEvent::Reconnected => info!("voice room reconnected"),
                RoomEvent::Disconnected { reason } => {
                    info!(reason = ?reason, "voice room disconnected");
                    event_disconnected.store(true, Ordering::Relaxed);
                    break;
                }
                RoomEvent::ConnectionStateChanged(state) => {
                    debug!(state = ?state, "voice room connection state changed");
                }
                RoomEvent::TrackSubscribed {
                    track,
                    publication,
                    participant,
                    ..
                } => {
                    let track_id = publication.sid().to_string();
                    if let RemoteTrack::Audio(track) = track
                        && !event_remote_playback_enabled.load(Ordering::Relaxed)
                    {
                        track.disable();
                    }
                    info!(
                        track_id = %track_id,
                        track = %publication.name(),
                        participant = %participant.identity(),
                        "subscribed to remote voice track"
                    );
                }
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    let track_id = publication.sid().to_string();
                    info!(track_id = %track_id, "unsubscribed from remote voice track");
                }
                _ => {}
            }
        }
        event_disconnected.store(true, Ordering::Relaxed);
    });

    let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    if muted {
        track.mute();
    }
    let publication = room
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                ..Default::default()
            },
        )
        .await
        .context("failed to publish CLI microphone")?;

    info!(
        room = %room_name,
        url = %url,
        microphone = %recording_device_name,
        speaker = playout_device_name.as_deref().unwrap_or("<default>"),
        "published CLI microphone and subscribed to voice room"
    );

    Ok(VoiceMediaSession {
        room,
        _audio: audio,
        publication,
        disconnected,
        remote_playback_enabled,
        events_task,
    })
}
