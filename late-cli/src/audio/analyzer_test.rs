use super::*;

const SAMPLE_RATE: u32 = 44_100;

fn sine(hz: f32, amplitude: f32) -> Vec<f32> {
    (0..FFT_SIZE)
        .map(|i| amplitude * (std::f32::consts::TAU * hz * i as f32 / SAMPLE_RATE as f32).sin())
        .collect()
}

fn loudest_band(frame: &VizSample) -> usize {
    frame
        .bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
        .expect("bands")
}

#[test]
fn silence_analyzes_to_an_empty_spectrum() {
    let mut analyzer = SpectrumAnalyzer::new(SAMPLE_RATE);
    let frame = analyzer.analyze(&[0.0; FFT_SIZE]);
    assert_eq!(frame.bands, [0.0; BAND_COUNT]);
    assert_eq!(frame.rms, 0.0);
}

#[test]
fn a_bass_tone_lights_the_lowest_band_and_a_treble_tone_the_highest() {
    // Quiet enough that soft compression keeps the bands apart instead of
    // saturating them all near 1.
    let mut analyzer = SpectrumAnalyzer::new(SAMPLE_RATE);
    let bass = analyzer.analyze(&sine(90.0, 0.001));
    assert_eq!(loudest_band(&bass), 0, "bass bands: {:?}", bass.bands);
    let treble = analyzer.analyze(&sine(9_000.0, 0.001));
    assert_eq!(
        loudest_band(&treble),
        BAND_COUNT - 1,
        "treble bands: {:?}",
        treble.bands
    );
}

#[test]
fn bands_and_rms_stay_inside_zero_to_one_at_full_scale() {
    let mut analyzer = SpectrumAnalyzer::new(SAMPLE_RATE);
    let frame = analyzer.analyze(&sine(440.0, 1.0));
    for band in frame.bands {
        assert!((0.0..=1.0).contains(&band), "band {band}");
    }
    assert!((0.0..=1.0).contains(&frame.rms), "rms {}", frame.rms);
    assert!(frame.rms > 0.0);
}
