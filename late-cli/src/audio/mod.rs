use anyhow::{Context, Result};
use cpal::traits::StreamTrait;
use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64},
        mpsc,
    },
    time::Duration,
};
use tokio::sync::broadcast;

mod decoder;

use decoder::{SymphoniaStreamDecoder, probe_stream_spec, trim_stream_suffix};

#[derive(Debug, Clone)]
pub(super) struct VizSample {
    pub(super) bands: [f32; 8],
    pub(super) rms: f32,
}

pub(super) struct AudioRuntime {
    _stream: Option<cpal::Stream>,
    pub(super) analyzer_tx: broadcast::Sender<VizSample>,
    pub(super) played_samples: Arc<AtomicU64>,
    pub(super) sample_rate: u32,
    pub(super) stop: Arc<AtomicBool>,
    pub(super) muted: Arc<AtomicBool>,
    pub(super) volume_percent: Arc<AtomicU8>,
    /// True when the user's audio_source preference is Icecast and the CLI
    /// should actually emit samples. False when the user picked YouTube — the
    /// CLI can't decode YouTube, so we silence the output without touching the
    /// user-controlled `muted` flag. Driven by `SetPlaybackSource` over the
    /// pair WS.
    pub(super) source_is_icecast: Arc<AtomicBool>,
    pub(super) enabled: bool,
}

#[derive(Debug, Clone, Copy)]
struct AudioSpec {
    sample_rate: u32,
    channels: usize,
}

mod resampler;

use resampler::StreamingLinearResampler;

mod output;

use output::{PlaybackQueue, PlayedRing, build_output_stream, output_sample_rate_for};
use ringbuf::{HeapRb, traits::Split};

const AUDIO_STARTUP_RETRIES: usize = 3;
const AUDIO_STARTUP_RETRY_DELAY: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AudioBackendProfile {
    Default,
    Wsl,
}

impl AudioRuntime {
    pub(super) async fn start(
        audio_base_url: String,
        audio_output_device: Option<String>,
    ) -> Result<Self> {
        if local_audio_disabled_on_this_platform() {
            return Ok(Self::disabled());
        }

        let profile = if is_wsl() {
            AudioBackendProfile::Wsl
        } else {
            AudioBackendProfile::Default
        };

        match Self::start_enabled(audio_base_url, audio_output_device, profile).await {
            Ok(runtime) => Ok(runtime),
            Err(err) if profile == AudioBackendProfile::Wsl => {
                let hint = audio_startup_hint();
                eprintln!(
                    "late: local WSL audio could not start; continuing without CLI audio.\n\
                     late: use browser pairing or the Windows-native late.exe for audio.\n\
                     late: {err:#}\n\n{hint}"
                );
                tracing::warn!(error = ?err, "WSL audio startup failed; continuing without local audio");
                Ok(Self::disabled())
            }
            Err(err) => Err(err),
        }
    }

    async fn start_enabled(
        audio_base_url: String,
        audio_output_device: Option<String>,
        profile: AudioBackendProfile,
    ) -> Result<Self> {
        let probe_url = audio_base_url.clone();
        let source_spec = tokio::task::spawn_blocking(move || {
            probe_stream_spec_with_retries(&probe_url, AUDIO_STARTUP_RETRIES)
        })
        .await
        .context("audio stream probe task failed")??;
        let output_sample_rate =
            output_sample_rate_for(source_spec, audio_output_device.as_deref())?;
        let queue_capacity = output_sample_rate as usize * source_spec.channels * 2;
        let (queue_tx, queue_rx) = HeapRb::<f32>::new(queue_capacity).split();
        let (played_tx, played_rx) = HeapRb::<f32>::new(4096).split();
        let played_samples = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        // Boot silent. The cpal output stream is started before the pair-WS
        // has had a chance to deliver the user's intended initial mute
        // state, so playing samples right away would bleed audio for the
        // round-trip duration. The server's first reply to client_state
        // unmutes us if the user's preference is "play on connect".
        let muted = Arc::new(AtomicBool::new(true));
        let volume_percent = Arc::new(AtomicU8::new(30));
        // Default to Icecast (play). The server's pair-WS connect always
        // sends SetPlaybackSource right after register, which flips this if
        // the user's persisted preference is Youtube.
        let source_is_icecast = Arc::new(AtomicBool::new(true));
        let (analyzer_tx, _) = broadcast::channel(32);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let stream = build_output_stream(
            source_spec,
            queue_rx,
            played_tx,
            Arc::clone(&played_samples),
            Arc::clone(&muted),
            Arc::clone(&volume_percent),
            Arc::clone(&source_is_icecast),
            audio_output_device.as_deref(),
            profile,
        )?;
        let output_sample_rate = stream.sample_rate;
        let stream = stream.stream;
        spawn_decoder_thread(
            audio_base_url,
            queue_tx,
            source_spec,
            output_sample_rate,
            Arc::clone(&stop),
            ready_tx,
            prebuffer_samples(profile, output_sample_rate, source_spec.channels),
        );
        spawn_playback_analyzer_thread(
            played_rx,
            analyzer_tx.clone(),
            output_sample_rate,
            Arc::clone(&stop),
        );
        ready_rx
            .recv()
            .context("failed to receive decoder startup status")??;
        stream
            .play()
            .context("failed to start audio output stream")?;

        Ok(Self {
            _stream: Some(stream),
            analyzer_tx,
            played_samples,
            sample_rate: output_sample_rate,
            stop,
            muted,
            volume_percent,
            source_is_icecast,
            enabled: true,
        })
    }

    fn disabled() -> Self {
        let (analyzer_tx, _) = broadcast::channel(32);
        Self {
            _stream: None,
            analyzer_tx,
            played_samples: Arc::new(AtomicU64::new(0)),
            sample_rate: 1,
            stop: Arc::new(AtomicBool::new(false)),
            muted: Arc::new(AtomicBool::new(false)),
            volume_percent: Arc::new(AtomicU8::new(0)),
            source_is_icecast: Arc::new(AtomicBool::new(true)),
            enabled: false,
        }
    }
}

fn prebuffer_samples(profile: AudioBackendProfile, sample_rate: u32, channels: usize) -> usize {
    match profile {
        AudioBackendProfile::Default => 0,
        // WSLg's RDP/PulseAudio bridge is prone to startup underruns. Buffer a
        // short half-second runway there without increasing native-platform
        // latency.
        AudioBackendProfile::Wsl => (sample_rate as usize * channels) / 2,
    }
}

fn probe_stream_spec_with_retries(audio_base_url: &str, max_retries: usize) -> Result<AudioSpec> {
    let mut attempt = 0;
    loop {
        match probe_stream_spec(audio_base_url) {
            Ok(spec) => return Ok(spec),
            Err(err) if attempt < max_retries => {
                attempt += 1;
                tracing::warn!(
                    error = ?err,
                    attempt,
                    max_retries,
                    "audio stream probe failed during startup; retrying"
                );
                std::thread::sleep(AUDIO_STARTUP_RETRY_DELAY);
            }
            Err(err) => return Err(err),
        }
    }
}

pub(super) fn audio_startup_hint() -> String {
    if is_wsl() {
        if missing_wsl_audio_env() {
            return "WSL was detected, but no Linux audio bridge appears configured.\n\
                    Checked env: DISPLAY, WAYLAND_DISPLAY, PULSE_SERVER.\n\
                    To enable audio:\n\
                    - On WSLg, update WSL/Windows and verify audio works in another Linux app\n\
                    - Otherwise run a PulseAudio server on Windows and set PULSE_SERVER\n\
                    - Then rerun `late`"
                .to_string();
        }

        return "WSL was detected and audio startup still failed.\n\
                Verify audio works in another Linux app first, then rerun `late`.\n\
                If you use a Windows PulseAudio server, confirm `PULSE_SERVER` points to it."
            .to_string();
    }

    "Check that this machine has a usable default audio output device and that another app can play sound, then rerun `late`."
        .to_string()
}

fn is_wsl() -> bool {
    env::var_os("WSL_DISTRO_NAME").is_some() || env::var_os("WSL_INTEROP").is_some()
}

fn missing_wsl_audio_env() -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY", "PULSE_SERVER"]
        .into_iter()
        .all(env_var_missing_or_blank)
}

fn env_var_missing_or_blank(key: &str) -> bool {
    env::var(key).map_or(true, |value| value.trim().is_empty())
}

const fn local_audio_disabled_on_this_platform() -> bool {
    #[cfg(target_os = "android")]
    {
        true
    }

    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

mod decoder_thread;

use decoder_thread::spawn_decoder_thread;

mod analyzer;

use analyzer::spawn_playback_analyzer_thread;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_missing_or_blank_treats_missing_and_blank_as_missing() {
        let key = "LATE_TEST_AUDIO_HINT_ENV";

        unsafe { env::remove_var(key) };
        assert!(env_var_missing_or_blank(key));

        unsafe { env::set_var(key, "   ") };
        assert!(env_var_missing_or_blank(key));

        unsafe { env::set_var(key, "set") };
        assert!(!env_var_missing_or_blank(key));

        unsafe { env::remove_var(key) };
    }

    #[test]
    fn disabled_runtime_uses_zeroed_playback_state() {
        let runtime = AudioRuntime::disabled();

        assert!(!runtime.enabled);
        assert_eq!(runtime.sample_rate, 1);
        assert_eq!(
            runtime
                .played_samples
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            runtime
                .volume_percent
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert!(!runtime.muted.load(std::sync::atomic::Ordering::Relaxed));
    }
}
