use anyhow::{Context, Result};
use cpal::traits::StreamTrait;
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{
    collections::VecDeque,
    env,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    thread,
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
    _stream: cpal::Stream,
    pub(super) analyzer_tx: broadcast::Sender<VizSample>,
    pub(super) played_samples: Arc<AtomicU64>,
    pub(super) sample_rate: u32,
    pub(super) stop: Arc<AtomicBool>,
    pub(super) muted: Arc<AtomicBool>,
    pub(super) volume_percent: Arc<AtomicU8>,
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

impl AudioRuntime {
    pub(super) async fn start(audio_base_url: String) -> Result<Self> {
        let probe_url = audio_base_url.clone();
        let source_spec = tokio::task::spawn_blocking(move || probe_stream_spec(&probe_url))
            .await
            .context("audio stream probe task failed")??;
        let output_sample_rate = output_sample_rate_for(source_spec)?;
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(
            output_sample_rate as usize * source_spec.channels,
        )));
        let played_ring = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let played_samples = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let muted = Arc::new(AtomicBool::new(false));
        let volume_percent = Arc::new(AtomicU8::new(30));
        let (analyzer_tx, _) = broadcast::channel(32);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let stream = build_output_stream(
            source_spec,
            Arc::clone(&queue),
            Arc::clone(&played_ring),
            Arc::clone(&played_samples),
            Arc::clone(&muted),
            Arc::clone(&volume_percent),
        )?;
        let output_sample_rate = stream.sample_rate;
        let stream = stream.stream;
        spawn_decoder_thread(
            audio_base_url,
            queue,
            source_spec,
            output_sample_rate,
            Arc::clone(&stop),
            ready_tx,
        );
        spawn_playback_analyzer_thread(
            Arc::clone(&played_ring),
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
            _stream: stream,
            analyzer_tx,
            played_samples,
            sample_rate: output_sample_rate,
            stop,
            muted,
            volume_percent,
        })
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

fn spawn_decoder_thread(
    audio_base_url: String,
    queue: PlaybackQueue,
    source_spec: AudioSpec,
    output_sample_rate: u32,
    stop: Arc<AtomicBool>,
    ready_tx: mpsc::SyncSender<Result<()>>,
) {
    thread::spawn(move || {
        let mut decoder_opt =
            match SymphoniaStreamDecoder::new_http(&trim_stream_suffix(&audio_base_url)) {
                Ok(decoder) => {
                    let _ = ready_tx.send(Ok(()));
                    Some(decoder)
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err.context("failed to create audio decoder")));
                    return;
                }
            };

        let max_buffer_samples = output_sample_rate as usize * source_spec.channels * 2;
        let mut chunk = Vec::with_capacity(1024 * source_spec.channels);
        let mut resampler = StreamingLinearResampler::new(
            source_spec.channels,
            source_spec.sample_rate,
            output_sample_rate,
        );
        let mut retries = 0;
        const MAX_RETRIES: usize = 10;

        while !stop.load(Ordering::Relaxed) {
            chunk.clear();

            if let Some(decoder) = &mut decoder_opt {
                for _ in 0..(1024 * source_spec.channels) {
                    match decoder.next() {
                        Some(sample) => chunk.push(sample),
                        None => {
                            decoder_opt = None;
                            break;
                        }
                    }
                }
            }

            if chunk.is_empty() {
                if decoder_opt.is_none() {
                    retries += 1;
                    if retries > MAX_RETRIES {
                        tracing::error!(
                            "audio stream failed {} times consecutively; giving up",
                            MAX_RETRIES
                        );
                        break;
                    }
                    tracing::warn!(
                        attempt = retries,
                        "audio stream ended or errored, reconnecting in 2s..."
                    );
                    thread::sleep(Duration::from_secs(2));

                    match SymphoniaStreamDecoder::new_http(&trim_stream_suffix(
                        &audio_base_url,
                    )) {
                        Ok(new_decoder) => {
                            tracing::info!("audio stream reconnected");
                            decoder_opt = Some(new_decoder);
                            retries = 0;
                        }
                        Err(err) => {
                            tracing::error!(error = ?err, "failed to reconnect audio stream");
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
                continue;
            }

            let chunk = resampler.process(&chunk);
            if chunk.is_empty() {
                continue;
            }

            loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }

                let mut queue_guard = queue.lock().unwrap_or_else(|e| e.into_inner());
                if queue_guard.len() + chunk.len() <= max_buffer_samples {
                    queue_guard.extend(chunk.iter().copied());
                    break;
                }
                drop(queue_guard);
                thread::sleep(Duration::from_millis(5));
            }
        }
    });
}

fn spawn_playback_analyzer_thread(
    played_ring: PlayedRing,
    analyzer_tx: broadcast::Sender<VizSample>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let cfg = AnalyzerConfig::default();
        let bands = log_bands(sample_rate as f32, cfg.fft_size, cfg.band_count);
        let fft = FftPlanner::new().plan_fft_forward(cfg.fft_size);
        let mut scratch = vec![Complex::new(0.0, 0.0); cfg.fft_size];
        let tick = Duration::from_millis(1000 / cfg.target_hz.max(1));

        while !stop.load(Ordering::Relaxed) {
            let frame = {
                let played_ring = played_ring.lock().unwrap_or_else(|e| e.into_inner());
                if played_ring.len() < cfg.fft_size {
                    None
                } else {
                    let start = played_ring.len() - cfg.fft_size;
                    let samples: Vec<f32> =
                        played_ring.iter().skip(start).copied().collect();
                    let (mut bands_out, mut rms) =
                        analyze_frame(&samples, &*fft, &mut scratch, &bands);
                    normalize_bands(&mut bands_out, &mut rms, cfg.gain);
                    Some(VizSample {
                        bands: bands_out,
                        rms,
                    })
                }
            };

            if let Some(frame) = frame {
                let _ = analyzer_tx.send(frame);
            }

            thread::sleep(tick);
        }
    });
}

fn log_bands(sample_rate: f32, n_fft: usize, band_count: usize) -> Vec<(usize, usize)> {
    let nyquist = sample_rate / 2.0;
    let min_hz: f32 = 60.0;
    let max_hz = nyquist.min(12000.0);
    let log_min = min_hz.ln();
    let log_max = max_hz.ln();

    (0..band_count)
        .map(|i| {
            let t0 = i as f32 / band_count as f32;
            let t1 = (i + 1) as f32 / band_count as f32;
            let f0 = (log_min + (log_max - log_min) * t0).exp();
            let f1 = (log_min + (log_max - log_min) * t1).exp();
            let b0 = ((f0 / nyquist) * (n_fft as f32 / 2.0)).floor().max(1.0) as usize;
            let b1 = ((f1 / nyquist) * (n_fft as f32 / 2.0))
                .ceil()
                .max(b0 as f32 + 1.0) as usize;
            (b0, b1)
        })
        .collect()
}

fn analyze_frame(
    samples: &[f32],
    fft: &dyn Fft<f32>,
    scratch: &mut [Complex<f32>],
    bands: &[(usize, usize)],
) -> ([f32; 8], f32) {
    let n = samples.len();
    for (i, s) in samples.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos();
        scratch[i] = Complex::new(s * w, 0.0);
    }

    fft.process(scratch);

    let mut mags = vec![0.0f32; n / 2];
    for (i, c) in scratch.iter().take(n / 2).enumerate() {
        mags[i] = (c.re * c.re + c.im * c.im).sqrt();
    }

    let mut out = [0.0f32; 8];
    for (bi, (b0, b1)) in bands.iter().enumerate() {
        let start = (*b0).min(mags.len());
        let end = (*b1).min(mags.len());
        let mut sum = 0.0;
        if end > start {
            for m in &mags[start..end] {
                sum += *m;
            }
            out[bi] = sum / ((end - start) as f32);
        }
    }

    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
    (out, rms)
}

fn soft_compress(x: f32) -> f32 {
    let k = 2.0;
    (k * x) / (1.0 + k * x)
}

fn normalize_bands(bands: &mut [f32], rms: &mut f32, gain: f32) {
    for b in bands.iter_mut() {
        *b = soft_compress(*b * gain).clamp(0.0, 1.0);
    }
    *rms = soft_compress(*rms * gain).clamp(0.0, 1.0);
}

#[derive(Debug, Clone)]
struct AnalyzerConfig {
    fft_size: usize,
    band_count: usize,
    gain: f32,
    target_hz: u64,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        AnalyzerConfig {
            fft_size: 1024,
            band_count: 8,
            gain: 3.0,
            target_hz: 15,
        }
    }
}

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
}
