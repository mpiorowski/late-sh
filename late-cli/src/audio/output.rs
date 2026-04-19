use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};

use super::AudioSpec;

pub(super) type PlaybackQueue = Arc<Mutex<VecDeque<f32>>>;
pub(super) type PlayedRing = Arc<Mutex<VecDeque<f32>>>;

#[derive(Clone)]
struct PlaybackOutputState {
    queue: PlaybackQueue,
    played_ring: PlayedRing,
    played_samples: Arc<AtomicU64>,
    source_channels: usize,
    muted: Arc<AtomicBool>,
    volume_percent: Arc<AtomicU8>,
}

pub(super) struct BuiltOutputStream {
    pub(super) stream: cpal::Stream,
    pub(super) sample_rate: u32,
}

pub(super) fn build_output_stream(
    spec: AudioSpec,
    queue: PlaybackQueue,
    played_ring: PlayedRing,
    played_samples: Arc<AtomicU64>,
    muted: Arc<AtomicBool>,
    volume_percent: Arc<AtomicU8>,
) -> Result<BuiltOutputStream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("no default audio output device found")?;
    let supported: Vec<_> = device
        .supported_output_configs()
        .context("failed to inspect supported output configurations")?
        .collect();

    let config = choose_output_config(&supported, spec).with_context(|| {
        format!(
            "no supported output configuration found for sample rate {} Hz",
            spec.sample_rate
        )
    })?;
    let channels = config.channels() as usize;
    let sample_rate = config.sample_rate().0;
    let stream_config = config.config();
    let err_fn = |err| eprintln!("audio output stream error: {err}");
    let output_state = PlaybackOutputState {
        queue,
        played_ring,
        played_samples,
        source_channels: spec.channels,
        muted,
        volume_percent,
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i8], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i16], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u16], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U8 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u8], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i32], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u32], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I64 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i64], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U64 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u64], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F64 => device.build_output_stream(
            &stream_config,
            move |data: &mut [f64], _| write_output_data(data, channels, &output_state),
            err_fn,
            None,
        )?,
        other => anyhow::bail!("unsupported sample format: {other:?}"),
    };

    Ok(BuiltOutputStream {
        stream,
        sample_rate,
    })
}

pub(super) fn output_sample_rate_for(spec: AudioSpec) -> Result<u32> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("no default audio output device found")?;
    let supported: Vec<_> = device
        .supported_output_configs()
        .context("failed to inspect supported output configurations")?
        .collect();
    let config = choose_output_config(&supported, spec).with_context(|| {
        format!(
            "no supported output configuration found for sample rate {} Hz",
            spec.sample_rate
        )
    })?;
    Ok(config.sample_rate().0)
}

fn write_output_data<T>(output: &mut [T], channels: usize, state: &PlaybackOutputState)
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut queue = state.queue.lock().unwrap_or_else(|e| e.into_inner());
    let mut played_ring = state.played_ring.lock().unwrap_or_else(|e| e.into_inner());
    let muted = state.muted.load(Ordering::Relaxed);
    let linear = state.volume_percent.load(Ordering::Relaxed) as f32 / 100.0;
    let volume = linear * linear;
    let source_channels = state.source_channels;

    for frame in output.chunks_mut(channels) {
        let mut source_frame = vec![0.0f32; source_channels];
        let mut pulled = 0usize;
        for slot in &mut source_frame {
            if let Some(sample) = queue.pop_front() {
                *slot = sample;
                pulled += 1;
            } else {
                break;
            }
        }

        let had_frame = pulled == source_channels;
        let output_frame = if had_frame {
            map_output_frame(&source_frame, channels)
        } else {
            vec![0.0; channels]
        };

        for (out, sample) in frame.iter_mut().zip(output_frame.iter().copied()) {
            let sample = if muted { 0.0 } else { sample * volume };
            *out = T::from_sample(sample);
        }

        if had_frame {
            let analyzer_sample = mix_for_analyzer(&source_frame);
            let analyzer_sample = if muted { 0.0 } else { analyzer_sample * volume };
            played_ring.push_back(analyzer_sample);
            while played_ring.len() > 4096 {
                played_ring.pop_front();
            }
            state.played_samples.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn output_config_rank(
    channels: usize,
    sample_format: cpal::SampleFormat,
    sample_rate: u32,
    spec: AudioSpec,
) -> (u8, u32, u8, usize) {
    let channel_rank = if channels == spec.channels {
        0
    } else if spec.channels == 1 && channels >= 1 {
        1
    } else if spec.channels == 2 && channels >= 2 {
        2
    } else {
        3
    };

    let format_rank = match sample_format {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::F64 => 1,
        cpal::SampleFormat::I32 | cpal::SampleFormat::U32 => 2,
        cpal::SampleFormat::I16 | cpal::SampleFormat::U16 => 3,
        cpal::SampleFormat::I8 | cpal::SampleFormat::U8 => 4,
        cpal::SampleFormat::I64 | cpal::SampleFormat::U64 => 5,
        _ => 6,
    };

    (
        channel_rank,
        sample_rate.abs_diff(spec.sample_rate),
        format_rank,
        channels,
    )
}

fn choose_output_config(
    supported: &[cpal::SupportedStreamConfigRange],
    spec: AudioSpec,
) -> Option<cpal::SupportedStreamConfig> {
    let mut chosen = None;
    let mut chosen_rank = None;

    for config in supported {
        let sample_rate = preferred_output_sample_rate(config, spec.sample_rate);
        let rank = output_config_rank(
            config.channels() as usize,
            config.sample_format(),
            sample_rate,
            spec,
        );
        let candidate = config.with_sample_rate(cpal::SampleRate(sample_rate));
        if chosen_rank.is_none_or(|current| rank < current) {
            chosen = Some(candidate);
            chosen_rank = Some(rank);
        }
    }

    chosen
}

fn preferred_output_sample_rate(
    config: &cpal::SupportedStreamConfigRange,
    desired_sample_rate: u32,
) -> u32 {
    desired_sample_rate.clamp(config.min_sample_rate().0, config.max_sample_rate().0)
}

fn map_output_frame(source_frame: &[f32], output_channels: usize) -> Vec<f32> {
    match (source_frame.len(), output_channels) {
        (_, 0) => Vec::new(),
        (0, n) => vec![0.0; n],
        (1, n) => vec![source_frame[0]; n],
        (2, 1) => vec![(source_frame[0] + source_frame[1]) * 0.5],
        (2, n) => (0..n).map(|idx| source_frame[idx % 2]).collect(),
        (src, n) if src == n => source_frame.to_vec(),
        (_, 1) => vec![mix_for_analyzer(source_frame)],
        (src, n) if src > n => source_frame[..n].to_vec(),
        (_, n) => {
            let mut out = Vec::with_capacity(n);
            out.extend_from_slice(source_frame);
            let last = *source_frame.last().unwrap_or(&0.0);
            out.resize(n, last);
            out
        }
    }
}

fn mix_for_analyzer(source_frame: &[f32]) -> f32 {
    if source_frame.is_empty() {
        return 0.0;
    }
    source_frame.iter().copied().sum::<f32>() / source_frame.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_stereo_to_stereo_without_downmixing() {
        let mapped = map_output_frame(&[0.25, -0.5], 2);
        assert_eq!(mapped, vec![0.25, -0.5]);
    }

    #[test]
    fn maps_stereo_to_quad_by_repeating_lr_pairs() {
        let mapped = map_output_frame(&[0.25, -0.5], 4);
        assert_eq!(mapped, vec![0.25, -0.5, 0.25, -0.5]);
    }

    #[test]
    fn maps_stereo_to_mono_for_analyzer_mix() {
        let mapped = map_output_frame(&[0.25, -0.5], 1);
        assert!((mapped[0] + 0.125).abs() < 1e-6);
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
}
