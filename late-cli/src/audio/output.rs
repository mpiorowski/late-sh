use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

use ringbuf::{
    HeapCons, HeapProd,
    traits::{Consumer, Observer},
};

use super::{AudioBackendProfile, AudioSpec};

pub(super) type PlaybackQueue = HeapProd<f32>;
pub(super) type PlaybackQueueReader = HeapCons<f32>;

struct PlaybackOutputState {
    queue: PlaybackQueueReader,
    played_samples: Arc<AtomicU64>,
    source_channels: usize,
    muted: Arc<AtomicBool>,
    volume_percent: Arc<AtomicU8>,
    /// When false, the user has selected a source the native audio thread
    /// cannot decode directly (today: YouTube). Driven by
    /// `SetPlaybackSource` over the pair WS.
    source_is_icecast: Arc<AtomicBool>,
    stream_generation: Arc<AtomicU64>,
    stream_flushed_generation: Arc<AtomicU64>,
    last_flushed_generation: u64,
    source_frame: Vec<f32>,
}

pub(super) struct BuiltOutputStream {
    pub(super) stream: cpal::Stream,
    pub(super) sample_rate: u32,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_output_stream(
    spec: AudioSpec,
    queue: PlaybackQueueReader,
    played_samples: Arc<AtomicU64>,
    muted: Arc<AtomicBool>,
    volume_percent: Arc<AtomicU8>,
    icecast_output_available: Arc<AtomicBool>,
    source_is_icecast: Arc<AtomicBool>,
    stream_generation: Arc<AtomicU64>,
    stream_flushed_generation: Arc<AtomicU64>,
    audio_output_device: Option<&str>,
    profile: AudioBackendProfile,
) -> Result<BuiltOutputStream> {
    let host = cpal::default_host();
    let device = output_device(&host, audio_output_device)?;
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
    let mut stream_config = config.config();
    apply_profile_buffer_size(&mut stream_config, config.buffer_size(), profile);
    let output_available_for_errors = Arc::clone(&icecast_output_available);
    let err_fn = move |err| {
        output_available_for_errors.store(false, Ordering::Relaxed);
        eprintln!("audio output stream error: {err}");
    };
    let mut output_state = PlaybackOutputState {
        queue,
        played_samples,
        source_channels: spec.channels,
        muted,
        volume_percent,
        source_is_icecast,
        stream_generation,
        stream_flushed_generation,
        last_flushed_generation: 0,
        source_frame: vec![0.0; spec.channels],
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i8], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i16], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u16], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U8 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u8], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i32], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u32], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I64 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i64], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U64 => device.build_output_stream(
            &stream_config,
            move |data: &mut [u64], _| write_output_data(data, channels, &mut output_state),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F64 => device.build_output_stream(
            &stream_config,
            move |data: &mut [f64], _| write_output_data(data, channels, &mut output_state),
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

pub(super) fn output_sample_rate_for(
    spec: AudioSpec,
    audio_output_device: Option<&str>,
) -> Result<u32> {
    let host = cpal::default_host();
    let device = output_device(&host, audio_output_device)?;
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

fn output_device(host: &cpal::Host, audio_output_device: Option<&str>) -> Result<cpal::Device> {
    let Some(name) = audio_output_device else {
        return host
            .default_output_device()
            .context("no default audio output device found");
    };

    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("audio output device name cannot be blank");
    }

    let mut available = Vec::new();
    for device in host
        .output_devices()
        .context("failed to enumerate audio output devices")?
    {
        match device.name() {
            Ok(device_name) if device_name == name => return Ok(device),
            Ok(device_name) => available.push(device_name),
            Err(err) => available.push(format!("<unavailable name: {err}>")),
        }
    }

    available.sort();
    available.dedup();
    if available.is_empty() {
        anyhow::bail!("audio output device '{name}' not found; no output devices are available");
    }

    anyhow::bail!(
        "audio output device '{name}' not found; available output devices: {}",
        available.join(", ")
    );
}

fn write_output_data<T>(output: &mut [T], channels: usize, state: &mut PlaybackOutputState)
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let target_generation = state.stream_generation.load(Ordering::SeqCst);
    if target_generation != state.last_flushed_generation {
        while state.queue.try_pop().is_some() {}
        state.last_flushed_generation = target_generation;
        state
            .stream_flushed_generation
            .store(target_generation, Ordering::SeqCst);
        fill_silence(output);
        return;
    }

    // `muted` is the user's intent (`m` keybind). `source_is_icecast` is the
    // structural gate: a YouTube preference means the CLI has nothing direct
    // to decode, so we emit silence even if the user toggled unmuted.
    let muted =
        state.muted.load(Ordering::Relaxed) || !state.source_is_icecast.load(Ordering::Relaxed);
    let linear = state.volume_percent.load(Ordering::Relaxed) as f32 / 100.0;
    let volume = linear * linear;
    let source_channels = state.source_channels;

    for frame in output.chunks_mut(channels) {
        let had_frame = if state.queue.occupied_len() >= source_channels {
            for slot in &mut state.source_frame {
                *slot = state.queue.try_pop().unwrap_or(0.0);
            }
            true
        } else {
            false
        };

        for (idx, out) in frame.iter_mut().enumerate() {
            let sample = if had_frame {
                map_output_sample(&state.source_frame, idx, channels)
            } else {
                0.0
            };
            let sample = if muted { 0.0 } else { sample * volume };
            *out = T::from_sample(sample);
        }

        if had_frame {
            state.played_samples.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn fill_silence<T>(output: &mut [T])
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    for sample in output {
        *sample = T::from_sample(0.0);
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

fn map_output_sample(source_frame: &[f32], output_idx: usize, output_channels: usize) -> f32 {
    match (source_frame.len(), output_channels) {
        (_, 0) | (0, _) => 0.0,
        (1, _) => source_frame[0],
        (2, 1) => (source_frame[0] + source_frame[1]) * 0.5,
        (2, _) => source_frame[output_idx % 2],
        (src, n) if src == n => source_frame[output_idx],
        (_, 1) => downmix_to_mono(source_frame),
        (src, _) if src > output_channels => source_frame[output_idx],
        (src, _) if output_idx < src => source_frame[output_idx],
        _ => *source_frame.last().unwrap_or(&0.0),
    }
}

fn downmix_to_mono(source_frame: &[f32]) -> f32 {
    if source_frame.is_empty() {
        return 0.0;
    }
    source_frame.iter().copied().sum::<f32>() / source_frame.len() as f32
}

fn apply_profile_buffer_size(
    config: &mut cpal::StreamConfig,
    supported: &cpal::SupportedBufferSize,
    profile: AudioBackendProfile,
) {
    if profile != AudioBackendProfile::Wsl {
        return;
    }

    const WSL_BUFFER_FRAMES: u32 = 2048;
    config.buffer_size = match *supported {
        cpal::SupportedBufferSize::Range { min, max } => {
            cpal::BufferSize::Fixed(WSL_BUFFER_FRAMES.clamp(min, max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    };
}

#[cfg(test)]
#[path = "output_test.rs"]
mod output_test;
