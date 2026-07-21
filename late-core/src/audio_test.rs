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
