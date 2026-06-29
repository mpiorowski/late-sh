use std::collections::VecDeque;

use crate::audio_config::AnalyzerConfig;
#[derive(Debug, Clone)]
pub struct VizFrame {
    pub bands: [f32; 8], // 0..1
    pub rms: f32,        // 0..1
    pub track_pos_ms: u64,
}

/// Runs the audio analyzer loop. Blocking - call from a dedicated thread.
pub fn run_analyzer(
    cfg: AnalyzerConfig,
    tx: tokio::sync::broadcast::Sender<VizFrame>,
    mut decoder: impl Iterator<Item = f32>, // mono samples
    sample_rate: f32,
) -> anyhow::Result<()> {
    cfg.validate().map_err(anyhow::Error::msg)?;
    let bands = log_bands(sample_rate, cfg.fft_size, cfg.band_count);
    let fft = rustfft::FftPlanner::new().plan_fft_forward(cfg.fft_size);
    let mut scratch = vec![rustfft::num_complex::Complex::new(0.0, 0.0); cfg.fft_size];

    let mut ring = VecDeque::with_capacity(cfg.fft_size);

    let min_interval = std::time::Duration::from_millis(1000 / cfg.target_hz);
    let mut last_broadcast = std::time::Instant::now();
    let mut samples = Vec::with_capacity(cfg.fft_size);
    let mut had_receivers = false;

    // Track position tracking
    let mut track_samples_count: u64 = 0;

    loop {
        // 1. Fill ring buffer
        match decoder.next() {
            Some(s) => {
                ring.push_back(s);
                track_samples_count += 1;
            }
            None => return Ok(()),
        }
        if ring.len() > cfg.fft_size {
            ring.pop_front();
        } else {
            continue;
        }

        // 2. Throttle updates
        let now = std::time::Instant::now();
        if now.duration_since(last_broadcast) < min_interval {
            continue;
        }

        // 3 .Skip analysis if no one is listening
        if tx.receiver_count() == 0 {
            if had_receivers {
                tracing::info!("no viz listeners, skipping analysis");
            }
            had_receivers = false;
            last_broadcast = now;
            continue;
        }
        if !had_receivers {
            tracing::info!("viz listeners connected, resuming analysis");
            had_receivers = true;
        }

        // 4. Analyze
        samples.clear();
        samples.extend(ring.iter().copied());
        let (mut bands_out, mut rms) = analyze_frame(&samples, &*fft, &mut scratch, &bands);
        normalize_bands(&mut bands_out, &mut rms, cfg.gain);

        static SENT: std::sync::Once = std::sync::Once::new();

        // Calculate MS
        let track_pos_ms = (track_samples_count * 1000) / (sample_rate as u64);

        if let Err(e) = tx.send(VizFrame {
            bands: bands_out,
            rms,
            track_pos_ms,
        }) {
            tracing::error!(error = ?e, "viz frame send failed unexpectedly");
        }
        SENT.call_once(|| tracing::info!("first viz frame sent"));

        last_broadcast = now;
    }
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
    fft: &dyn rustfft::Fft<f32>,
    scratch: &mut [rustfft::num_complex::Complex<f32>],
    bands: &[(usize, usize)],
) -> ([f32; 8], f32) {
    use rustfft::num_complex::Complex;
    // Hann window
    let n = samples.len();
    for (i, s) in samples.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos();
        scratch[i] = Complex::new(s * w, 0.0);
    }

    fft.process(scratch);

    // Magnitudes
    let mut mags = vec![0.0f32; n / 2];
    for (i, c) in scratch.iter().take(n / 2).enumerate() {
        mags[i] = (c.re * c.re + c.im * c.im).sqrt();
    }

    // Band energy
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

    // RMS
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
    (out, rms)
}

fn soft_compress(x: f32) -> f32 {
    // simple soft knee; tweak as needed
    let k = 2.0;
    (k * x) / (1.0 + k * x)
}

fn normalize_bands(bands: &mut [f32], rms: &mut f32, gain: f32) {
    for b in bands.iter_mut() {
        *b = soft_compress(*b * gain).clamp(0.0, 1.0);
    }
    *rms = soft_compress(*rms * gain).clamp(0.0, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_compress_zero() {
        assert_eq!(soft_compress(0.0), 0.0);
    }

    #[test]
    fn soft_compress_bounds() {
        // Output should always be less than 1.0 for any finite input
        assert!(soft_compress(1.0) < 1.0);
        assert!(soft_compress(10.0) < 1.0);
        assert!(soft_compress(100.0) < 1.0);
    }

    #[test]
    fn soft_compress_monotonic() {
        // Larger input should give larger output
        let a = soft_compress(0.5);
        let b = soft_compress(1.0);
        let c = soft_compress(2.0);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn log_bands_returns_correct_count() {
        let bands = log_bands(44100.0, 1024, 8);
        assert_eq!(bands.len(), 8);
    }

    #[test]
    fn log_bands_ranges_are_valid() {
        let bands = log_bands(44100.0, 1024, 8);
        for (start, end) in &bands {
            assert!(start < end, "band start should be less than end");
            assert!(*end <= 512, "band end should not exceed nyquist bin");
        }
    }

    #[test]
    fn log_bands_are_ascending() {
        let bands = log_bands(44100.0, 1024, 8);
        for i in 1..bands.len() {
            assert!(bands[i].0 >= bands[i - 1].0, "bands should be ascending");
        }
    }

    #[test]
    fn normalize_bands_clamps_output() {
        let mut bands = [10.0; 8];
        let mut rms = 10.0;
        normalize_bands(&mut bands, &mut rms, 3.0);

        for b in bands {
            assert!((0.0..=1.0).contains(&b));
        }
        assert!((0.0..=1.0).contains(&rms));
    }

    #[test]
    fn normalize_bands_with_zero_input() {
        let mut bands = [0.0; 8];
        let mut rms = 0.0;
        normalize_bands(&mut bands, &mut rms, 3.0);

        assert_eq!(bands, [0.0; 8]);
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn soft_compress_positive_always() {
        // For any positive input, output is positive and < 1.0
        for x in [0.01, 0.1, 0.5, 1.0, 5.0, 50.0] {
            let y = soft_compress(x);
            assert!(y > 0.0, "output should be positive for x={x}");
            assert!(y < 1.0, "output should be < 1.0 for x={x}");
        }
    }

    #[test]
    fn log_bands_different_sample_rate() {
        let bands = log_bands(48000.0, 2048, 8);
        assert_eq!(bands.len(), 8);
        for (start, end) in &bands {
            assert!(start < end);
            assert!(*end <= 1024);
        }
    }

    #[test]
    fn analyze_frame_silence() {
        let samples = vec![0.0f32; 1024];
        let fft = rustfft::FftPlanner::new().plan_fft_forward(1024);
        let mut scratch = vec![rustfft::num_complex::Complex::new(0.0, 0.0); 1024];
        let bands = log_bands(44100.0, 1024, 8);

        let (band_vals, rms) = analyze_frame(&samples, &*fft, &mut scratch, &bands);

        assert_eq!(rms, 0.0);
        for b in band_vals {
            assert_eq!(b, 0.0);
        }
    }
}
