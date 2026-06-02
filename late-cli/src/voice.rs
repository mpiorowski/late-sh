use anyhow::{Context, Result};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tracing::{debug, info, warn};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use futures_util::StreamExt;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use livekit::{
    PlatformAudio,
    options::TrackPublishOptions,
    prelude::{
        LocalAudioTrack, LocalTrack, LocalTrackPublication, RemoteAudioTrack, RemoteTrack, Room,
        RoomEvent, RoomOptions, TrackSource,
    },
    webrtc::{audio_frame::AudioFrame, audio_stream::native::NativeAudioStream},
};

#[derive(Default)]
pub(super) struct VoiceRuntimeState {
    pub(super) joined: bool,
    pub(super) room: Option<String>,
    pub(super) muted: bool,
    pub(super) deafened: bool,
    pub(super) speaking: bool,
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    media: Option<VoiceMediaSession>,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
struct VoiceMediaSession {
    room: Room,
    _audio: PlatformAudio,
    publication: LocalTrackPublication,
    playback: Option<VoicePlayback>,
    disconnected: Arc<AtomicBool>,
    events_task: tokio::task::JoinHandle<()>,
    remote_audio_tasks: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl VoiceMediaSession {
    fn set_remote_playback_enabled(&self, enabled: bool) {
        if let Some(playback) = &self.playback {
            playback.set_enabled(enabled);
        }

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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
struct VoicePlayback {
    _stream: cpal::Stream,
    handle: VoicePlaybackHandle,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[derive(Clone)]
struct VoicePlaybackHandle {
    queue: Arc<Mutex<VecDeque<f32>>>,
    enabled: Arc<AtomicBool>,
    sample_rate: u32,
    capacity_samples: usize,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl VoicePlayback {
    fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default voice output device found")?;
        let config = device
            .default_output_config()
            .context("failed to inspect default voice output config")?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let capacity_samples = sample_rate as usize * 2;
        let handle = VoicePlaybackHandle {
            queue: Arc::new(Mutex::new(VecDeque::with_capacity(capacity_samples))),
            enabled: Arc::new(AtomicBool::new(true)),
            sample_rate,
            capacity_samples,
        };
        let err_fn = |err| warn!(error = ?err, "voice output stream error");
        let stream_config = config.config();
        let stream = match config.sample_format() {
            cpal::SampleFormat::I8 => build_voice_output_stream::<i8>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::F32 => build_voice_output_stream::<f32>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::I16 => build_voice_output_stream::<i16>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::U16 => build_voice_output_stream::<u16>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::U8 => build_voice_output_stream::<u8>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::I32 => build_voice_output_stream::<i32>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::U32 => build_voice_output_stream::<u32>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::I64 => build_voice_output_stream::<i64>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::U64 => build_voice_output_stream::<u64>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            cpal::SampleFormat::F64 => build_voice_output_stream::<f64>(
                &device,
                &stream_config,
                channels,
                handle.clone(),
                err_fn,
            )?,
            other => anyhow::bail!("unsupported voice output sample format: {other:?}"),
        };
        stream
            .play()
            .context("failed to start voice output stream")?;
        info!(
            sample_rate,
            channels, "started CLI voice playback output stream"
        );
        Ok(Self {
            _stream: stream,
            handle,
        })
    }

    fn handle(&self) -> VoicePlaybackHandle {
        self.handle.clone()
    }

    fn set_enabled(&self, enabled: bool) {
        self.handle.set_enabled(enabled);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn build_voice_output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    output_channels: usize,
    playback: VoicePlaybackHandle,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| write_voice_output(data, output_channels, &playback),
        err_fn,
        None,
    )?;
    Ok(stream)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn write_voice_output<T>(output: &mut [T], output_channels: usize, playback: &VoicePlaybackHandle)
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let enabled = playback.enabled.load(Ordering::Relaxed);
    let Ok(mut queue) = playback.queue.lock() else {
        for out in output {
            *out = T::from_sample(0.0);
        }
        return;
    };

    for frame in output.chunks_mut(output_channels.max(1)) {
        let sample = if enabled {
            queue.pop_front().unwrap_or(0.0)
        } else {
            0.0
        };
        for out in frame {
            *out = T::from_sample(sample);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl VoicePlaybackHandle {
    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if !enabled && let Ok(mut queue) = self.queue.lock() {
            queue.clear();
        }
    }

    fn push_frame(&self, frame: AudioFrame<'static>) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let samples = frame_to_mono_f32(&frame);
        if samples.is_empty() {
            return;
        }

        let Ok(mut queue) = self.queue.lock() else {
            return;
        };
        let overflow = queue
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.capacity_samples);
        for _ in 0..overflow {
            let _ = queue.pop_front();
        }
        queue.extend(samples);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn frame_to_mono_f32(frame: &AudioFrame<'_>) -> Vec<f32> {
    let channels = frame.num_channels.max(1) as usize;
    let samples_per_channel = frame.samples_per_channel as usize;
    let mut output = Vec::with_capacity(samples_per_channel);
    for sample_idx in 0..samples_per_channel {
        let base = sample_idx * channels;
        if base >= frame.data.len() {
            break;
        }
        let mut sum = 0.0;
        let mut count = 0usize;
        for channel_idx in 0..channels {
            let Some(sample) = frame.data.get(base + channel_idx) else {
                continue;
            };
            sum += *sample as f32 / i16::MAX as f32;
            count += 1;
        }
        if count > 0 {
            output.push((sum / count as f32).clamp(-1.0, 1.0));
        }
    }
    output
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn spawn_remote_voice_playback(
    track_id: String,
    track: RemoteAudioTrack,
    playback: VoicePlaybackHandle,
    tasks: &Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
) {
    let mut stream = NativeAudioStream::new(track.rtc_track(), playback.sample_rate as i32, 1);
    let task_id = track_id.clone();
    let task = tokio::spawn(async move {
        info!(track_id = %task_id, "started remote voice playback stream");
        let mut received_frames = 0u64;
        while let Some(frame) = stream.next().await {
            if received_frames == 0 {
                info!(
                    track_id = %task_id,
                    sample_rate = frame.sample_rate,
                    channels = frame.num_channels,
                    samples_per_channel = frame.samples_per_channel,
                    "received first remote voice frame"
                );
            }
            received_frames += 1;
            playback.push_frame(frame);
        }
        info!(
            track_id = %task_id,
            received_frames,
            "remote voice playback stream ended"
        );
    });

    if let Ok(mut tasks) = tasks.lock()
        && let Some(previous) = tasks.insert(track_id, task)
    {
        previous.abort();
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

        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            let media = connect_voice_media(&room, &url, &token, muted).await?;
            self.media = Some(media);
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = (&url, &token);
            anyhow::bail!("voice media is not supported on this platform");
        }

        self.joined = true;
        self.room = Some(room);
        self.muted = false;
        self.deafened = false;
        self.speaking = false;
        self.set_muted(muted);
        self.set_deafened(deafened);

        Ok(())
    }

    pub(super) async fn leave(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if let Some(media) = self.media.take() {
            let VoiceMediaSession {
                room,
                _audio,
                publication: _,
                playback: _,
                disconnected: _,
                events_task,
                remote_audio_tasks,
            } = media;
            if let Ok(mut tasks) = remote_audio_tasks.lock() {
                for (_, task) in tasks.drain() {
                    task.abort();
                }
            }
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

        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if let Some(media) = self.media.as_ref() {
            media.set_remote_playback_enabled(!deafened);
        }
    }

    pub(super) fn media_disconnected(&self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            self.media
                .as_ref()
                .is_some_and(|media| media.disconnected.load(Ordering::Relaxed))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

    let playback = match VoicePlayback::start() {
        Ok(playback) => Some(playback),
        Err(err) => {
            warn!(
                error = ?err,
                "failed to start CLI voice playback; microphone publishing will still work"
            );
            None
        }
    };
    let playback_handle = playback.as_ref().map(VoicePlayback::handle);
    let remote_audio_tasks = Arc::new(Mutex::new(HashMap::new()));
    let room_options = RoomOptions::default();
    let (room, mut events) = Room::connect(url, token, room_options)
        .await
        .with_context(|| format!("failed to connect voice room {room_name:?}"))?;
    let event_playback = playback_handle.clone();
    let event_remote_audio_tasks = Arc::clone(&remote_audio_tasks);
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
                    if let RemoteTrack::Audio(track) = track {
                        if let Some(playback) = event_playback.clone() {
                            if !playback.enabled.load(Ordering::Relaxed) {
                                track.disable();
                            }
                            spawn_remote_voice_playback(
                                track_id.clone(),
                                track,
                                playback,
                                &event_remote_audio_tasks,
                            );
                        } else {
                            warn!(
                                track_id = %track_id,
                                "received remote voice track but CLI voice playback is unavailable"
                            );
                        }
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
                    if let Ok(mut tasks) = event_remote_audio_tasks.lock()
                        && let Some(task) = tasks.remove(&track_id)
                    {
                        task.abort();
                    }
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
        playback,
        disconnected,
        events_task,
        remote_audio_tasks,
    })
}
