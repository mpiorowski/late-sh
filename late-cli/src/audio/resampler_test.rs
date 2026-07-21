use super::*;

#[test]
fn resampler_passthrough_preserves_native_rate_frames() {
    let mut resampler = StreamingLinearResampler::new(2, 44_100, 44_100);
    let input = vec![0.1, -0.1, 0.25, -0.25];
    assert_eq!(resampler.process(&input), input);
}

#[test]
fn resampler_outputs_audio_when_upsampling() {
    let mut resampler = StreamingLinearResampler::new(1, 44_100, 48_000);
    let input = vec![0.0, 1.0, 0.0, -1.0];
    let output = resampler.process(&input);
    assert!(output.len() >= input.len());
    assert!(output.iter().all(|sample| (-1.0..=1.0).contains(sample)));
}
