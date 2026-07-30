use serde::Deserialize;
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TrackMetadata {
    pub(super) identity: String,
    pub(super) title: String,
    pub(super) artist: Option<String>,
    pub(super) album: Option<String>,
    pub(super) duration_ms: Option<i64>,
    pub(super) started_at_ms: Option<i64>,
    pub(super) art_url: Option<String>,
    pub(super) url: Option<String>,
}

/// Position only ever reaches a real MPRIS server, so this is gated to the
/// platform that has one. Left ungated it is dead code everywhere else, which
/// `make check`'s `-D warnings` turns into a failed build on macOS/Windows.
#[cfg(target_os = "linux")]
impl TrackMetadata {
    fn position_ms(&self) -> i64 {
        let Some(started_at_ms) = self.started_at_ms else {
            return 0;
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .unwrap_or(started_at_ms);
        let elapsed_ms = now_ms.saturating_sub(started_at_ms).max(0);
        self.duration_ms
            .map_or(elapsed_ms, |duration_ms| elapsed_ms.min(duration_ms))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MediaSource {
    Icecast,
    Youtube,
    Radio,
}

#[derive(Debug, Deserialize, Clone)]
pub(super) struct YoutubeTrack {
    pub(super) id: String,
    pub(super) video_id: String,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) channel: Option<String>,
    #[serde(default)]
    pub(super) duration_ms: Option<i64>,
    #[serde(default)]
    pub(super) started_at_ms: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub(super) struct IcecastTrack {
    pub(super) title: String,
    #[serde(default)]
    pub(super) artist: Option<String>,
    #[serde(default)]
    pub(super) duration_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub(super) struct RadioTrack {
    pub(super) artist: String,
    pub(super) title: String,
}

/// One published state of the desktop player. The publisher only ever cares
/// about the latest one, which is what makes a `watch` the right channel: a
/// burst of queue updates collapses to the track that survived it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MediaUpdate {
    track: Option<TrackMetadata>,
    muted: bool,
    volume_percent: u8,
}

pub(super) struct DesktopMedia {
    /// Dropping this ends the publisher task: its `changed()` returns `Err`
    /// once the last sender is gone.
    updates: watch::Sender<MediaUpdate>,
    source: Option<MediaSource>,
    station: Option<String>,
    stream_url: Option<String>,
    youtube_current: Option<YoutubeTrack>,
    icecast_tracks: HashMap<String, IcecastTrack>,
    radio_tracks: HashMap<String, RadioTrack>,
}

impl DesktopMedia {
    /// Metadata projection stays on the caller's thread; every D-Bus await
    /// happens in a task of its own. The pair WebSocket loop also carries
    /// mute/volume and voice control, so it must never block on the session
    /// bus: an unreachable or wedged bus would stall audio control with
    /// nothing in the logs pointing at MPRIS.
    pub(super) fn new() -> Self {
        let (updates, mut rx) = watch::channel(MediaUpdate {
            track: None,
            muted: false,
            volume_percent: 0,
        });
        tokio::spawn(async move {
            let publisher = MprisPublisher::new().await;
            while rx.changed().await.is_ok() {
                let update = rx.borrow_and_update().clone();
                publisher
                    .update(update.track.as_ref(), update.muted, update.volume_percent)
                    .await;
            }
        });
        Self {
            updates,
            source: None,
            station: None,
            stream_url: None,
            youtube_current: None,
            icecast_tracks: HashMap::new(),
            radio_tracks: HashMap::new(),
        }
    }

    /// No publisher task, so sends land in a channel nobody reads. Tests here
    /// cover metadata projection, which is pure.
    #[cfg(test)]
    fn for_test() -> Self {
        let (updates, _) = watch::channel(MediaUpdate {
            track: None,
            muted: false,
            volume_percent: 0,
        });
        Self {
            updates,
            source: None,
            station: None,
            stream_url: None,
            youtube_current: None,
            icecast_tracks: HashMap::new(),
            radio_tracks: HashMap::new(),
        }
    }

    pub(super) fn select_source(
        &mut self,
        source: MediaSource,
        station: Option<String>,
        stream_url: Option<String>,
        muted: bool,
        volume_percent: u8,
    ) {
        self.source = Some(source);
        self.station = station;
        self.stream_url = stream_url;
        self.publish(muted, volume_percent);
    }

    pub(super) fn update_youtube(
        &mut self,
        current: Option<YoutubeTrack>,
        muted: bool,
        volume_percent: u8,
    ) {
        self.youtube_current = current;
        if self.source == Some(MediaSource::Youtube) {
            self.publish(muted, volume_percent);
        }
    }

    pub(super) fn update_icecast(
        &mut self,
        mounts: HashMap<String, IcecastTrack>,
        muted: bool,
        volume_percent: u8,
    ) {
        self.icecast_tracks = mounts;
        if self.source == Some(MediaSource::Icecast) {
            self.publish(muted, volume_percent);
        }
    }

    pub(super) fn update_radio(
        &mut self,
        stations: HashMap<String, RadioTrack>,
        muted: bool,
        volume_percent: u8,
    ) {
        self.radio_tracks = stations;
        if self.source == Some(MediaSource::Radio) {
            self.publish(muted, volume_percent);
        }
    }

    pub(super) fn update_audio_state(&self, muted: bool, volume_percent: u8) {
        self.publish(muted, volume_percent);
    }

    /// A closed receiver means the publisher task has ended (no session bus,
    /// or the runtime is shutting down). There is nothing to publish to and
    /// nothing the pair loop can do about it, so the send result is dropped
    /// deliberately rather than logged on every track change.
    fn publish(&self, muted: bool, volume_percent: u8) {
        let update = MediaUpdate {
            track: self.current_track(),
            muted,
            volume_percent,
        };
        drop(self.updates.send(update));
    }

    fn current_track(&self) -> Option<TrackMetadata> {
        match self.source? {
            MediaSource::Youtube => Some(self.youtube_track()),
            MediaSource::Icecast => Some(self.icecast_track()),
            MediaSource::Radio => Some(self.radio_track()),
        }
    }

    fn youtube_track(&self) -> TrackMetadata {
        let Some(current) = &self.youtube_current else {
            return TrackMetadata {
                identity: "youtube:fallback".to_string(),
                title: "fallback stream".to_string(),
                artist: Some("late.sh".to_string()),
                album: Some("late.sh · YouTube".to_string()),
                duration_ms: None,
                started_at_ms: None,
                art_url: None,
                url: Some("https://late.sh/listen".to_string()),
            };
        };
        TrackMetadata {
            identity: format!("youtube:{}", current.id),
            title: nonblank(current.title.as_deref())
                .unwrap_or("YouTube video")
                .to_string(),
            artist: nonblank(current.channel.as_deref()).map(str::to_string),
            album: Some("late.sh · YouTube".to_string()),
            duration_ms: current.duration_ms.filter(|duration| *duration >= 0),
            started_at_ms: current.started_at_ms,
            art_url: Some(format!(
                "https://i.ytimg.com/vi/{}/hqdefault.jpg",
                current.video_id
            )),
            url: Some(format!(
                "https://www.youtube.com/watch?v={}",
                current.video_id
            )),
        }
    }

    fn icecast_track(&self) -> TrackMetadata {
        let station = self.station.as_deref().unwrap_or("icecast");
        let details = self.icecast_tracks.get(station);
        let title = details
            .and_then(|track| nonblank(Some(&track.title)))
            .unwrap_or(station);
        let artist = details
            .and_then(|track| nonblank(track.artist.as_deref()))
            .map(str::to_string);
        TrackMetadata {
            identity: format!(
                "icecast:{station}:{}:{}",
                artist.as_deref().unwrap_or_default(),
                title
            ),
            title: title.to_string(),
            artist,
            album: Some(format!("late.sh · {station}")),
            duration_ms: details
                .and_then(|track| track.duration_seconds)
                .and_then(|seconds| i64::try_from(seconds).ok())
                .and_then(|seconds| seconds.checked_mul(1000)),
            started_at_ms: None,
            art_url: None,
            url: self.stream_url.clone(),
        }
    }

    fn radio_track(&self) -> TrackMetadata {
        let station = self.station.as_deref().unwrap_or("radio");
        let details = self.radio_tracks.get(station);
        let title = details
            .and_then(|track| nonblank(Some(&track.title)))
            .unwrap_or(station);
        let artist = details
            .and_then(|track| nonblank(Some(&track.artist)))
            .map(str::to_string)
            .or_else(|| Some("Nightride FM".to_string()));
        TrackMetadata {
            identity: format!(
                "radio:{station}:{}:{}",
                artist.as_deref().unwrap_or_default(),
                title
            ),
            title: title.to_string(),
            artist,
            album: Some(format!("late.sh · {station}")),
            duration_ms: None,
            started_at_ms: None,
            art_url: None,
            url: self.stream_url.clone(),
        }
    }
}

fn nonblank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::TrackMetadata;
    use mpris_server::{
        LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property,
        RootInterface, Server, Time, TrackId, Volume,
        zbus::{Result as ZbusResult, fdo},
    };
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
        sync::{
            RwLock,
            atomic::{AtomicI64, AtomicU8, AtomicU64, Ordering},
        },
    };

    const STATUS_STOPPED: u8 = 0;
    const STATUS_PLAYING: u8 = 1;

    pub(super) struct Publisher {
        server: Server<MprisInterface>,
    }

    struct MprisInterface {
        metadata: RwLock<Metadata>,
        status: AtomicU8,
        volume_bits: AtomicU64,
        position_micros: AtomicI64,
    }

    impl Publisher {
        pub(super) async fn new() -> ZbusResult<Self> {
            let suffix = format!("late.instance{}", std::process::id());
            let server = Server::new(
                &suffix,
                MprisInterface {
                    metadata: RwLock::new(Metadata::new()),
                    status: AtomicU8::new(STATUS_STOPPED),
                    volume_bits: AtomicU64::new(1.0_f64.to_bits()),
                    position_micros: AtomicI64::new(0),
                },
            )
            .await?;
            Ok(Self { server })
        }

        pub(super) async fn update(
            &self,
            track: Option<&TrackMetadata>,
            muted: bool,
            volume_percent: u8,
        ) -> ZbusResult<()> {
            let metadata = track.map_or_else(Metadata::new, metadata_for_track);
            let status = if track.is_some() {
                PlaybackStatus::Playing
            } else {
                PlaybackStatus::Stopped
            };
            let volume = if muted {
                0.0
            } else {
                f64::from(volume_percent) / 100.0
            };
            let position_micros = track
                .map(TrackMetadata::position_ms)
                .unwrap_or_default()
                .saturating_mul(1000);

            {
                let mut current = self
                    .server
                    .imp()
                    .metadata
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *current = metadata.clone();
            }
            self.server.imp().status.store(
                if status == PlaybackStatus::Playing {
                    STATUS_PLAYING
                } else {
                    STATUS_STOPPED
                },
                Ordering::Relaxed,
            );
            self.server
                .imp()
                .volume_bits
                .store(volume.to_bits(), Ordering::Relaxed);
            self.server
                .imp()
                .position_micros
                .store(position_micros, Ordering::Relaxed);

            self.server
                .properties_changed([
                    Property::Metadata(metadata),
                    Property::PlaybackStatus(status),
                    Property::Volume(volume),
                ])
                .await
        }
    }

    pub(super) fn metadata_for_track(track: &TrackMetadata) -> Metadata {
        let mut hasher = DefaultHasher::new();
        track.identity.hash(&mut hasher);
        let track_id = TrackId::try_from(format!("/sh/late/track/{:016x}", hasher.finish()))
            .expect("generated MPRIS track path must be valid");

        let mut builder = Metadata::builder()
            .trackid(track_id)
            .title(track.title.clone());
        if let Some(artist) = &track.artist {
            builder = builder.artist([artist.clone()]);
        }
        if let Some(album) = &track.album {
            builder = builder.album(album.clone());
        }
        if let Some(duration_ms) = track.duration_ms.filter(|duration| *duration >= 0) {
            builder = builder.length(Time::from_millis(duration_ms));
        }
        if let Some(art_url) = &track.art_url {
            builder = builder.art_url(art_url.clone());
        }
        if let Some(url) = &track.url {
            builder = builder.url(url.clone());
        }
        builder.build()
    }

    impl RootInterface for MprisInterface {
        async fn raise(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn quit(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn can_quit(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn fullscreen(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn set_fullscreen(&self, _fullscreen: bool) -> ZbusResult<()> {
            Ok(())
        }

        async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_raise(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn has_track_list(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn identity(&self) -> fdo::Result<String> {
            Ok("late.sh".to_string())
        }

        async fn desktop_entry(&self) -> fdo::Result<String> {
            Ok("late".to_string())
        }

        async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    impl PlayerInterface for MprisInterface {
        async fn next(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn previous(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn pause(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn play_pause(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn stop(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn play(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn seek(&self, _offset: Time) -> fdo::Result<()> {
            Ok(())
        }

        async fn set_position(&self, _track_id: TrackId, _position: Time) -> fdo::Result<()> {
            Ok(())
        }

        async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
            Ok(())
        }

        async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
            Ok(if self.status.load(Ordering::Relaxed) == STATUS_PLAYING {
                PlaybackStatus::Playing
            } else {
                PlaybackStatus::Stopped
            })
        }

        async fn loop_status(&self) -> fdo::Result<LoopStatus> {
            Ok(LoopStatus::None)
        }

        async fn set_loop_status(&self, _loop_status: LoopStatus) -> ZbusResult<()> {
            Ok(())
        }

        async fn rate(&self) -> fdo::Result<PlaybackRate> {
            Ok(1.0)
        }

        async fn set_rate(&self, _rate: PlaybackRate) -> ZbusResult<()> {
            Ok(())
        }

        async fn shuffle(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn set_shuffle(&self, _shuffle: bool) -> ZbusResult<()> {
            Ok(())
        }

        async fn metadata(&self) -> fdo::Result<Metadata> {
            Ok(self
                .metadata
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone())
        }

        async fn volume(&self) -> fdo::Result<Volume> {
            Ok(f64::from_bits(self.volume_bits.load(Ordering::Relaxed)))
        }

        async fn set_volume(&self, _volume: Volume) -> ZbusResult<()> {
            Ok(())
        }

        async fn position(&self) -> fdo::Result<Time> {
            Ok(Time::from_micros(
                self.position_micros.load(Ordering::Relaxed),
            ))
        }

        async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
            Ok(1.0)
        }

        async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
            Ok(1.0)
        }

        async fn can_go_next(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_go_previous(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_play(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_pause(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_seek(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_control(&self) -> fdo::Result<bool> {
            Ok(false)
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::TrackMetadata;
    use std::{
        convert::Infallible,
        future::{Ready, ready},
    };

    pub(super) struct Publisher;

    /// These mirror the Linux publisher's awaited signatures so the shared
    /// wrapper needs no `cfg`, but there is nothing to await off-Linux. They
    /// return a ready future rather than being `async fn` so no suppression
    /// is needed for a future that completes immediately.
    impl Publisher {
        pub(super) fn new() -> Ready<Result<Self, Infallible>> {
            ready(Ok(Self))
        }

        pub(super) fn update(
            &self,
            _track: Option<&TrackMetadata>,
            _muted: bool,
            _volume_percent: u8,
        ) -> Ready<Result<(), Infallible>> {
            ready(Ok(()))
        }
    }
}

pub(super) struct MprisPublisher {
    publisher: Option<platform::Publisher>,
}

impl MprisPublisher {
    pub(super) async fn new() -> Self {
        match platform::Publisher::new().await {
            Ok(publisher) => {
                #[cfg(target_os = "linux")]
                tracing::info!("desktop MPRIS publisher ready");
                Self {
                    publisher: Some(publisher),
                }
            }
            Err(err) => {
                tracing::debug!(
                    error = %err,
                    "desktop MPRIS unavailable; continuing without media publication"
                );
                Self { publisher: None }
            }
        }
    }

    pub(super) async fn update(
        &self,
        track: Option<&TrackMetadata>,
        muted: bool,
        volume_percent: u8,
    ) {
        let Some(publisher) = &self.publisher else {
            return;
        };
        if let Err(err) = publisher.update(track, muted, volume_percent).await {
            tracing::warn!(error = %err, "failed to update desktop MPRIS state");
        }
    }

    #[cfg(test)]
    pub(super) const fn disabled() -> Self {
        Self { publisher: None }
    }
}

#[cfg(test)]
#[path = "mpris_test.rs"]
mod mpris_test;
