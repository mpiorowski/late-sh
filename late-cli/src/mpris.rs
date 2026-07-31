use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
#[cfg(target_os = "linux")]
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, watch};

/// Read handles onto the mute and volume the CLI is actually playing at.
/// MPRIS reports these but never writes them: a desktop client's control
/// travels as a [`DesktopCommand`] up the pair WebSocket, and the atomics only
/// change when the server's fan-out comes back on the socket path.
#[derive(Clone)]
pub(super) struct AudioControls {
    pub(super) muted: Arc<AtomicBool>,
    pub(super) volume_percent: Arc<AtomicU8>,
}

/// The MPRIS transport commands a desktop client can send us. Playback itself
/// is the server's, and pause/resume of a live stream is not a thing, so mute
/// is the one thing a command can mean and every one resolves to a target
/// mute state.
///
/// Gated with the rest of the command path: only a real MPRIS server issues
/// these, so off-Linux they are dead code and `-D warnings` fails the build.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransportCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
}

#[cfg(target_os = "linux")]
impl TransportCommand {
    fn mutes(self, currently_muted: bool) -> bool {
        match self {
            Self::Play => false,
            Self::Pause | Self::Stop => true,
            Self::PlayPause => !currently_muted,
        }
    }
}

/// One audio control issued by a desktop media client (a widget's play/pause,
/// media keys, a volume slider). Not applied locally: the pair WebSocket loop
/// sends it to the server, which fans the resulting `set_muted`/`set_volume`
/// out to every paired client, this CLI and the webview helper alike. That
/// round trip is what makes a widget press reach YouTube, which plays in the
/// helper process, exactly like a TUI `m` keypress does.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopCommand {
    SetMuted { muted: bool },
    SetVolume { volume_percent: u8 },
}

/// Only a real MPRIS server produces desktop commands and off-Linux there is
/// none, so the twin is uninhabited: the channel and the pair loop's send path
/// compile unchanged, while "a command arrived" stays unrepresentable.
#[cfg(not(target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopCommand {}

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

/// Capacity of the desktop command queue between the MPRIS interface and the
/// pair WebSocket loop. Commands are single key presses; a loop that stops
/// draining for longer than a handful of them is reconnecting, and commands
/// dropped then are presses the user will simply repeat.
const DESKTOP_COMMAND_QUEUE_CAP: usize = 8;

pub(super) struct DesktopMedia {
    /// Only the projected track travels: mute and volume are read live from
    /// [`AudioControls`] at publish time, so there is one source of truth no
    /// matter which side changed them. Dropping this ends the publisher task,
    /// whose `changed()` returns `Err` once the last sender is gone.
    track: watch::Sender<Option<TrackMetadata>>,
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
    ///
    /// Also returns the receiving end of the desktop command queue. The pair
    /// WebSocket loop drains it and forwards each command to the server; the
    /// state the MPRIS interface reports only changes when the server's
    /// fan-out lands back on the socket and [`Self::republish_audio_state`]
    /// nudges the task, so a play/pause from a widget and one from the TUI
    /// converge through the same path.
    pub(super) fn new(controls: AudioControls) -> (Self, mpsc::Receiver<DesktopCommand>) {
        let (track, mut track_rx) = watch::channel(None);
        let (commands, commands_rx) = mpsc::channel(DESKTOP_COMMAND_QUEUE_CAP);
        let task_controls = controls.clone();
        tokio::spawn(async move {
            let publisher = MprisPublisher::new(controls, commands).await;
            // The track sender lives in `DesktopMedia`, so its `Err` is what
            // ends this task. A `watch` send notifies even when the value is
            // unchanged, which is what lets `republish_audio_state` reuse it.
            while track_rx.changed().await.is_ok() {
                let track = track_rx.borrow_and_update().clone();
                publisher
                    .update(
                        track.as_ref(),
                        task_controls.muted.load(Ordering::Relaxed),
                        task_controls.volume_percent.load(Ordering::Relaxed),
                    )
                    .await;
            }
        });
        let media = Self {
            track,
            source: None,
            station: None,
            stream_url: None,
            youtube_current: None,
            icecast_tracks: HashMap::new(),
            radio_tracks: HashMap::new(),
        };
        (media, commands_rx)
    }

    /// No publisher task, so sends land in a channel nobody reads. Tests here
    /// cover metadata projection, which is pure.
    #[cfg(test)]
    fn for_test() -> Self {
        let (track, _) = watch::channel(None);
        Self {
            track,
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
    ) {
        self.source = Some(source);
        self.station = station;
        self.stream_url = stream_url;
        self.publish();
    }

    pub(super) fn update_youtube(&mut self, current: Option<YoutubeTrack>) {
        self.youtube_current = current;
        if self.source == Some(MediaSource::Youtube) {
            self.publish();
        }
    }

    pub(super) fn update_icecast(&mut self, mounts: HashMap<String, IcecastTrack>) {
        self.icecast_tracks = mounts;
        if self.source == Some(MediaSource::Icecast) {
            self.publish();
        }
    }

    pub(super) fn update_radio(&mut self, stations: HashMap<String, RadioTrack>) {
        self.radio_tracks = stations;
        if self.source == Some(MediaSource::Radio) {
            self.publish();
        }
    }

    /// Mute and volume are read from [`AudioControls`] by the publisher, so a
    /// change to either only needs to nudge the task; a `watch` send notifies
    /// its receiver even when the track it carries is unchanged.
    pub(super) fn republish_audio_state(&self) {
        self.publish();
    }

    /// A closed receiver means the publisher task has ended (no session bus,
    /// or the runtime is shutting down). There is nothing to publish to and
    /// nothing the pair loop can do about it, so the send result is dropped
    /// deliberately rather than logged on every track change.
    fn publish(&self) {
        drop(self.track.send(self.current_track()));
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
    use super::{AudioControls, DesktopCommand, TrackMetadata, TransportCommand};
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
    use tokio::sync::mpsc;

    const STATUS_STOPPED: u8 = 0;
    const STATUS_PLAYING: u8 = 1;
    const STATUS_PAUSED: u8 = 2;

    pub(super) struct Publisher {
        server: Server<MprisInterface>,
    }

    struct MprisInterface {
        metadata: RwLock<Metadata>,
        status: AtomicU8,
        volume_bits: AtomicU64,
        position_micros: AtomicI64,
        controls: AudioControls,
        commands: mpsc::Sender<DesktopCommand>,
    }

    impl MprisInterface {
        /// Every transport command resolves to the absolute mute state it
        /// targets, read against the flag the CLI is actually playing with.
        /// Absolute rather than a toggle so all paired clients converge even
        /// if one had drifted.
        fn transport(&self, command: TransportCommand) {
            let muted = command.mutes(self.controls.muted.load(Ordering::Relaxed));
            self.send_command(DesktopCommand::SetMuted { muted });
        }

        /// `try_send` because a D-Bus handler must never wait on the pair
        /// loop: a full queue means the loop is wedged or reconnecting, and
        /// a dropped press is one the user will repeat, unlike a stalled
        /// session bus.
        fn send_command(&self, command: DesktopCommand) {
            if let Err(err) = self.commands.try_send(command) {
                tracing::warn!(?command, error = %err, "dropping desktop media command");
            }
        }
    }

    impl Publisher {
        pub(super) async fn new(
            controls: AudioControls,
            commands: mpsc::Sender<DesktopCommand>,
        ) -> ZbusResult<Self> {
            let suffix = format!("late.instance{}", std::process::id());
            let server = Server::new(
                &suffix,
                MprisInterface {
                    metadata: RwLock::new(Metadata::new()),
                    status: AtomicU8::new(STATUS_STOPPED),
                    volume_bits: AtomicU64::new(1.0_f64.to_bits()),
                    position_micros: AtomicI64::new(0),
                    controls,
                    commands,
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
            // Muted is reported as Paused rather than a zeroed volume: it is
            // what a desktop widget draws a play button for, and it round
            // trips through `pause`/`play` back to the same mute flag.
            let status = match (track.is_some(), muted) {
                (false, _) => PlaybackStatus::Stopped,
                (true, true) => PlaybackStatus::Paused,
                (true, false) => PlaybackStatus::Playing,
            };
            let volume = f64::from(volume_percent) / 100.0;
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
                match status {
                    PlaybackStatus::Playing => STATUS_PLAYING,
                    PlaybackStatus::Paused => STATUS_PAUSED,
                    PlaybackStatus::Stopped => STATUS_STOPPED,
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

    /// MPRIS volume is a double in 0.0..=1.0; the wire carries a percent.
    /// Clamped before scaling, so this lands in 0..=100 and the cast cannot
    /// truncate or lose a sign (a NaN write saturates to 0).
    pub(super) fn volume_percent_from_mpris(volume: Volume) -> u8 {
        (volume.clamp(0.0, 1.0) * 100.0).round() as u8
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
            self.transport(TransportCommand::Pause);
            Ok(())
        }

        async fn play_pause(&self) -> fdo::Result<()> {
            self.transport(TransportCommand::PlayPause);
            Ok(())
        }

        async fn stop(&self) -> fdo::Result<()> {
            self.transport(TransportCommand::Stop);
            Ok(())
        }

        async fn play(&self) -> fdo::Result<()> {
            self.transport(TransportCommand::Play);
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

        async fn set_volume(&self, volume: Volume) -> ZbusResult<()> {
            // Advertising CanControl means clients may write this, so honour
            // it rather than accepting the call and dropping it. Raising the
            // slider off zero is also how a widget expects to unmute; every
            // client applies that on the `set_volume` fan-out.
            self.send_command(DesktopCommand::SetVolume {
                volume_percent: volume_percent_from_mpris(volume),
            });
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
            Ok(true)
        }

        async fn can_pause(&self) -> fdo::Result<bool> {
            Ok(true)
        }

        async fn can_seek(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_control(&self) -> fdo::Result<bool> {
            Ok(true)
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{AudioControls, DesktopCommand, TrackMetadata};
    use std::{
        convert::Infallible,
        future::{Ready, ready},
    };
    use tokio::sync::mpsc;

    pub(super) struct Publisher;

    /// These mirror the Linux publisher's awaited signatures so the shared
    /// wrapper needs no `cfg`, but there is nothing to await off-Linux. They
    /// return a ready future rather than being `async fn` so no suppression
    /// is needed for a future that completes immediately. Dropping the
    /// command sender here is what closes the pair loop's receiver.
    impl Publisher {
        pub(super) fn new(
            _controls: AudioControls,
            _commands: mpsc::Sender<DesktopCommand>,
        ) -> Ready<Result<Self, Infallible>> {
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
    pub(super) async fn new(
        controls: AudioControls,
        commands: mpsc::Sender<DesktopCommand>,
    ) -> Self {
        match platform::Publisher::new(controls, commands).await {
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
}

#[cfg(test)]
#[path = "mpris_test.rs"]
mod mpris_test;
