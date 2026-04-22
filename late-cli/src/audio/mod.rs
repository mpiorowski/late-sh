use anyhow::{Context, Result};
use cpal::traits::StreamTrait;
use ringbuf::{HeapRb, traits::Split};
use std::{
    collections::VecDeque,
    env,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::broadcast;

mod decoder;

use decoder::{SymphoniaStreamDecoder, probe_stream, trim_stream_suffix};

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
    pub(super) enabled: bool,
}

#[derive(Debug, Clone, Copy)]
struct AudioSpec {
    sample_rate: u32,
    channels: usize,
}

#[derive(Debug, Default)]
pub(super) struct AudioStats {
    pub(super) underrun_frames: AtomicU64,
}

mod resampler;

use resampler::StreamingLinearResampler;

mod output;

use output::{PlaybackBuildInputs, PlayedRing, build_output_stream, output_sample_rate_for};

impl AudioRuntime {
    pub(super) async fn start(audio_base_url: String) -> Result<Self> {
        if local_audio_disabled_on_this_platform() {
            return Ok(Self::disabled());
        }

        let probe_url = audio_base_url.clone();
        let initial_decoder = tokio::task::spawn_blocking(move || probe_stream(&probe_url))
            .await
            .context("audio stream probe task failed")??;
        let source_spec = initial_decoder.spec();
        let output_sample_rate = output_sample_rate_for(source_spec)?;
        let ring_capacity_samples =
            (output_sample_rate as usize * source_spec.channels * 2).max(1);
        let (prod, cons) = HeapRb::<f32>::new(ring_capacity_samples).split();
        let played_ring = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let played_samples = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let muted = Arc::new(AtomicBool::new(false));
        let volume_percent = Arc::new(AtomicU8::new(30));
        let stats = Arc::new(AudioStats::default());
        let (analyzer_tx, _) = broadcast::channel(32);

        let stream = build_output_stream(
            source_spec,
            PlaybackBuildInputs {
                cons,
                played_ring: Arc::clone(&played_ring),
                played_samples: Arc::clone(&played_samples),
                muted: Arc::clone(&muted),
                volume_percent: Arc::clone(&volume_percent),
                stats: Arc::clone(&stats),
            },
        )?;
        let output_sample_rate = stream.sample_rate;
        let stream = stream.stream;
        spawn_decoder_thread(
            audio_base_url,
            prod,
            source_spec,
            output_sample_rate,
            Arc::clone(&stop),
            initial_decoder,
        );
        spawn_audio_stats_thread(Arc::clone(&stats), Arc::clone(&stop));
        spawn_playback_analyzer_thread(
            Arc::clone(&played_ring),
            analyzer_tx.clone(),
            output_sample_rate,
            Arc::clone(&stop),
        );
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
            enabled: false,
        }
    }
}

fn spawn_audio_stats_thread(stats: Arc<AudioStats>, stop: Arc<AtomicBool>) {
    thread::spawn(move || {
        let interval = Duration::from_secs(5);
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(interval);
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let underruns = stats.underrun_frames.swap(0, Ordering::Relaxed);
            if underruns > 0 {
                tracing::info!(
                    underrun_frames = underruns,
                    window_secs = interval.as_secs(),
                    "audio stats",
                );
            }
        }
    });
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
