use super::*;

#[test]
fn maps_stereo_to_stereo_without_downmixing() {
    assert_eq!(map_output_sample(&[0.25, -0.5], 0, 2), 0.25);
    assert_eq!(map_output_sample(&[0.25, -0.5], 1, 2), -0.5);
}

#[test]
fn maps_stereo_to_quad_by_repeating_lr_pairs() {
    assert_eq!(map_output_sample(&[0.25, -0.5], 0, 4), 0.25);
    assert_eq!(map_output_sample(&[0.25, -0.5], 1, 4), -0.5);
    assert_eq!(map_output_sample(&[0.25, -0.5], 2, 4), 0.25);
    assert_eq!(map_output_sample(&[0.25, -0.5], 3, 4), -0.5);
}

#[test]
fn maps_stereo_to_mono_for_analyzer_mix() {
    assert!((map_output_sample(&[0.25, -0.5], 0, 1) + 0.125).abs() < 1e-6);
}

#[test]
fn analyzer_mix_averages_channels() {
    assert!((mix_for_analyzer(&[0.5, -0.25, 0.25]) - (1.0 / 6.0)).abs() < 1e-6);
}

#[test]
fn preferred_output_sample_rate_uses_native_rate_when_supported() {
    let config = cpal::SupportedStreamConfigRange::new(
        2,
        cpal::SampleRate(44_100),
        cpal::SampleRate(48_000),
        cpal::SupportedBufferSize::Unknown,
        cpal::SampleFormat::F32,
    );
    assert_eq!(preferred_output_sample_rate(&config, 44_100), 44_100);
}

#[test]
fn preferred_output_sample_rate_clamps_when_native_rate_is_unsupported() {
    let config = cpal::SupportedStreamConfigRange::new(
        2,
        cpal::SampleRate(48_000),
        cpal::SampleRate(48_000),
        cpal::SupportedBufferSize::Unknown,
        cpal::SampleFormat::F32,
    );
    assert_eq!(preferred_output_sample_rate(&config, 44_100), 48_000);
}

#[test]
fn wsl_profile_requests_fixed_buffer_size() {
    let mut config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(48_000),
        buffer_size: cpal::BufferSize::Default,
    };
    apply_profile_buffer_size(
        &mut config,
        &cpal::SupportedBufferSize::Range {
            min: 512,
            max: 4096,
        },
        AudioBackendProfile::Wsl,
    );
    assert_eq!(config.buffer_size, cpal::BufferSize::Fixed(2048));
}
