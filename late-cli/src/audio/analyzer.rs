use ringbuf::traits::Consumer;
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::broadcast;

use super::{PlayedRing, VizSample};

/// Samples per analysis window.
const FFT_SIZE: usize = 1024;
/// Spectrum bands per frame; the pair-WS `viz` payload is fixed at 8.
const BAND_COUNT: usize = 8;
/// Lowest and highest frequency the bands cover, log-spaced between.
const MIN_HZ: f32 = 60.0;
const MAX_HZ: f32 = 12_000.0;
/// Linear gain applied before soft compression into 0..1.
const GAIN: f32 = 3.0;
/// Frames per second sent to the TUI.
const TARGET_HZ: u64 = 15;

/// Analyzes what the output callback actually played and broadcasts one
/// spectrum frame per tick. A tick with no newly played samples sends
/// nothing: a muted CLI, a YouTube-selected CLI, or an underrun produces
/// no frames, and the TUI falls back to its ambient band on its own.
pub(super) fn spawn_playback_analyzer_thread(
    mut played_ring: PlayedRing,
    analyzer_tx: broadcast::Sender<VizSample>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut analyzer = SpectrumAnalyzer::new(sample_rate);
        let mut window: VecDeque<f32> = VecDeque::with_capacity(FFT_SIZE);
        let tick = Duration::from_millis(1000 / TARGET_HZ);

        while !stop.load(Ordering::Relaxed) {
            let mut fresh = false;
            while let Some(sample) = played_ring.try_pop() {
                if window.len() == FFT_SIZE {
                    window.pop_front();
                }
                window.push_back(sample);
                fresh = true;
            }

            if fresh && window.len() == FFT_SIZE {
                let frame = analyzer.analyze(window.make_contiguous());
                // Err means no pair socket is subscribed right now (not yet
                // connected, or reconnecting); the frame has no audience.
                let _ = analyzer_tx.send(frame);
            }

            thread::sleep(tick);
        }
    });
}

/// FFT plan, Hann window, and band layout for one output sample rate,
/// reused across frames so analysis allocates nothing per frame.
pub(super) struct SpectrumAnalyzer {
    fft: Arc<dyn Fft<f32>>,
    hann: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    bands: [(usize, usize); BAND_COUNT],
}

impl SpectrumAnalyzer {
    pub(super) fn new(sample_rate: u32) -> Self {
        let hann = (0..FFT_SIZE)
            .map(|i| {
                0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE as f32 - 1.0)).cos()
            })
            .collect();
        Self {
            fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
            hann,
            scratch: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            bands: log_bands(sample_rate as f32),
        }
    }

    /// One spectrum frame for exactly [`FFT_SIZE`] mono samples.
    pub(super) fn analyze(&mut self, samples: &[f32]) -> VizSample {
        assert_eq!(samples.len(), FFT_SIZE, "analyzer window size");
        for ((slot, sample), weight) in self.scratch.iter_mut().zip(samples).zip(&self.hann) {
            *slot = Complex::new(sample * weight, 0.0);
        }
        self.fft.process(&mut self.scratch);

        let bands = self.bands.map(|(start, end)| {
            let bins = &self.scratch[start..end];
            let sum: f32 = bins.iter().map(|c| c.norm()).sum();
            soft_compress(sum / bins.len() as f32 * GAIN)
        });
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / FFT_SIZE as f32).sqrt();
        VizSample {
            bands,
            rms: soft_compress(rms * GAIN),
        }
    }
}

/// Bin ranges `[start, end)` for log-spaced bands between [`MIN_HZ`] and
/// [`MAX_HZ`] (capped at Nyquist). Every band holds at least one bin.
fn log_bands(sample_rate: f32) -> [(usize, usize); BAND_COUNT] {
    let nyquist = sample_rate / 2.0;
    let half_bins = (FFT_SIZE / 2) as f32;
    let log_min = MIN_HZ.ln();
    let log_max = MAX_HZ.min(nyquist).ln();
    std::array::from_fn(|i| {
        let f0 = (log_min + (log_max - log_min) * i as f32 / BAND_COUNT as f32).exp();
        let f1 = (log_min + (log_max - log_min) * (i + 1) as f32 / BAND_COUNT as f32).exp();
        let start = ((f0 / nyquist) * half_bins).floor().max(1.0) as usize;
        let end = ((f1 / nyquist) * half_bins).ceil().max(start as f32 + 1.0) as usize;
        (start, end.min(FFT_SIZE / 2))
    })
}

/// Maps 0..∞ into 0..1 with a soft knee, so loud passages saturate
/// gracefully instead of pinning every band at the top.
fn soft_compress(x: f32) -> f32 {
    (2.0 * x / (1.0 + 2.0 * x)).clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "analyzer_test.rs"]
mod analyzer_test;
