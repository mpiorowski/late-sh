use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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

mod decoder {
    use anyhow::{Context, Result};
    use std::io::{self, Cursor, Read};
    use symphonia::core::{
        audio::{AudioBufferRef, SampleBuffer},
        codecs::{Decoder, DecoderOptions},
        formats::{FormatOptions, FormatReader},
        io::{MediaSourceStream, ReadOnlySource},
        meta::MetadataOptions,
        probe::Hint,
    };
    use symphonia::default::{get_codecs, get_probe};

    use super::AudioSpec;

    pub(super) struct SymphoniaStreamDecoder {
        format: Box<dyn FormatReader>,
        decoder: Box<dyn Decoder>,
        track_id: u32,
        sample_buf: Vec<f32>,
        sample_pos: usize,
        spec: AudioSpec,
    }

    struct PrefixThenRead<R> {
        prefix: Cursor<Vec<u8>>,
        inner: R,
    }

    impl<R> PrefixThenRead<R> {
        fn new(prefix: Vec<u8>, inner: R) -> Self {
            Self {
                prefix: Cursor::new(prefix),
                inner,
            }
        }
    }

    impl<R: Read> Read for PrefixThenRead<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.prefix.read(buf)?;
            if n > 0 {
                return Ok(n);
            }
            self.inner.read(buf)
        }
    }

    impl SymphoniaStreamDecoder {
        pub(super) fn new_http(url: &str) -> Result<Self> {
            let stream_url = url.to_string() + "/stream";
            let mut resp = reqwest::blocking::get(&stream_url)
                .context("http get")?
                .error_for_status()
                .with_context(|| format!("stream request failed for {stream_url}"))?;
            let prefix = read_until_mp3_sync(&mut resp)
                .with_context(|| format!("failed to align MP3 stream from {stream_url}"))?;
            let source = ReadOnlySource::new(PrefixThenRead::new(prefix, resp));

            let mss = MediaSourceStream::new(Box::new(source), Default::default());
            let mut hint = Hint::new();
            hint.with_extension("mp3");

            let probed = get_probe().format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )?;

            let format = probed.format;
            let (track_id, spec, decoder) = {
                let track = format.default_track().context("no default track")?;
                let sample_rate = track.codec_params.sample_rate.context("no sample rate")?;
                let channels = track
                    .codec_params
                    .channels
                    .context("no channel layout")?
                    .count();
                let decoder =
                    get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
                (
                    track.id,
                    AudioSpec {
                        sample_rate,
                        channels,
                    },
                    decoder,
                )
            };

            Ok(Self {
                format,
                decoder,
                track_id,
                sample_buf: Vec::new(),
                sample_pos: 0,
                spec,
            })
        }

        fn refill(&mut self) -> Result<bool> {
            loop {
                let packet = match self.format.next_packet() {
                    Ok(packet) => packet,
                    Err(symphonia::core::errors::Error::IoError(_)) => return Ok(false),
                    Err(err) => return Err(err.into()),
                };
                if packet.track_id() != self.track_id {
                    continue;
                }

                let decoded = self.decoder.decode(&packet)?;
                self.sample_buf.clear();
                self.sample_pos = 0;
                push_interleaved_samples(&mut self.sample_buf, decoded)?;
                return Ok(true);
            }
        }

        fn spec(&self) -> AudioSpec {
            self.spec
        }
    }

    impl Iterator for SymphoniaStreamDecoder {
        type Item = f32;

        fn next(&mut self) -> Option<Self::Item> {
            if self.sample_pos >= self.sample_buf.len() {
                match self.refill() {
                    Ok(true) => {}
                    Ok(false) => return None,
                    Err(err) => {
                        tracing::warn!(error = ?err, "decoder refill error, treating as eof");
                        return None;
                    }
                }
            }

            let sample = self.sample_buf.get(self.sample_pos).copied();
            self.sample_pos += 1;
            sample
        }
    }

    fn read_until_mp3_sync<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
        const MAX_SCAN_BYTES: usize = 64 * 1024;
        const CHUNK_SIZE: usize = 4096;

        let mut buf = Vec::with_capacity(CHUNK_SIZE * 2);
        let mut chunk = [0u8; CHUNK_SIZE];

        while buf.len() < MAX_SCAN_BYTES {
            let read = reader
                .read(&mut chunk)
                .context("failed to read from audio stream")?;
            if read == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..read]);

            if let Some(offset) = find_mp3_sync_offset(&buf) {
                return Ok(buf.split_off(offset));
            }
        }

        anyhow::bail!("could not find MP3 frame sync in first {} bytes", buf.len())
    }

    fn find_mp3_sync_offset(bytes: &[u8]) -> Option<usize> {
        if bytes.starts_with(b"ID3") {
            return Some(0);
        }

        for i in 0..=bytes.len().saturating_sub(3) {
            let b0 = bytes[i];
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];

            if b0 != 0xFF || (b1 & 0xE0) != 0xE0 {
                continue;
            }

            let version = (b1 >> 3) & 0x03;
            let layer = (b1 >> 1) & 0x03;
            let bitrate_idx = (b2 >> 4) & 0x0F;
            let sample_rate_idx = (b2 >> 2) & 0x03;

            if version == 0x01 || layer == 0x00 {
                continue;
            }
            if bitrate_idx == 0x00 || bitrate_idx == 0x0F {
                continue;
            }
            if sample_rate_idx == 0x03 {
                continue;
            }

            return Some(i);
        }

        None
    }

    fn push_interleaved_samples(out: &mut Vec<f32>, decoded: AudioBufferRef<'_>) -> Result<()> {
        let spec = *decoded.spec();
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        out.extend_from_slice(buf.samples());
        Ok(())
    }

    pub(super) fn probe_stream_spec(audio_base_url: &str) -> Result<AudioSpec> {
        let decoder = SymphoniaStreamDecoder::new_http(&trim_stream_suffix(audio_base_url))
            .context("failed to create audio decoder for stream probe")?;
        Ok(decoder.spec())
    }

    pub(super) fn trim_stream_suffix(audio_base_url: &str) -> String {
        audio_base_url
            .trim_end_matches('/')
            .trim_end_matches("/stream")
            .to_string()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn trim_stream_suffix_normalizes_base_url() {
            assert_eq!(
                trim_stream_suffix("http://audio.late.sh/stream"),
                "http://audio.late.sh"
            );
            assert_eq!(
                trim_stream_suffix("http://audio.late.sh/"),
                "http://audio.late.sh"
            );
        }

        #[test]
        fn find_mp3_sync_offset_finds_frame_after_garbage() {
            let mut bytes = vec![0x12, 0x34, 0x56, 0x78];
            bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x64, 0x00, 0x00]);
            assert_eq!(find_mp3_sync_offset(&bytes), Some(4));
        }

        #[test]
        fn find_mp3_sync_offset_accepts_id3_header() {
            assert_eq!(find_mp3_sync_offset(b"ID3\x04\x00\x00"), Some(0));
        }

        #[test]
        fn find_mp3_sync_offset_checks_last_possible_offset() {
            let bytes = [0x00, 0xFF, 0xFB, 0x90];
            assert_eq!(find_mp3_sync_offset(&bytes), Some(1));
        }
    }
}

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

type PlaybackQueue = Arc<Mutex<VecDeque<f32>>>;
type PlayedRing = Arc<Mutex<VecDeque<f32>>>;

#[derive(Debug, Clone, Copy)]
struct AudioSpec {
    sample_rate: u32,
    channels: usize,
}

#[derive(Clone)]
struct PlaybackOutputState {
    queue: PlaybackQueue,
    played_ring: PlayedRing,
    played_samples: Arc<AtomicU64>,
    source_channels: usize,
    muted: Arc<AtomicBool>,
    volume_percent: Arc<AtomicU8>,
}

struct BuiltOutputStream {
    stream: cpal::Stream,
    sample_rate: u32,
}

struct StreamingLinearResampler {
    channels: usize,
    source_rate: u32,
    target_rate: u32,
    position: f64,
    previous_frame: Option<Vec<f32>>,
}

impl StreamingLinearResampler {
    fn new(channels: usize, source_rate: u32, target_rate: u32) -> Self {
        Self {
            channels,
            source_rate,
            target_rate,
            position: 0.0,
            previous_frame: None,
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.channels == 0
            || input.is_empty()
            || !input.len().is_multiple_of(self.channels)
        {
            return Vec::new();
        }

        if self.source_rate == self.target_rate {
            self.previous_frame =
                Some(input[input.len() - self.channels..input.len()].to_vec());
            return input.to_vec();
        }

        let input_frames = input.len() / self.channels;
        let combined_frames = input_frames + usize::from(self.previous_frame.is_some());
        if combined_frames < 2 {
            self.previous_frame = Some(input.to_vec());
            return Vec::new();
        }

        let step = self.source_rate as f64 / self.target_rate as f64;
        let available_intervals = (combined_frames - 1) as f64;
        let mut output = Vec::new();

        while self.position < available_intervals {
            let left_idx = self.position.floor() as usize;
            let right_idx = left_idx + 1;
            let frac = (self.position - left_idx as f64) as f32;
            for channel in 0..self.channels {
                let left = self.frame_sample(input, left_idx, channel);
                let right = self.frame_sample(input, right_idx, channel);
                output.push(left + (right - left) * frac);
            }
            self.position += step;
        }

        self.position -= available_intervals;
        self.previous_frame = Some(input[input.len() - self.channels..input.len()].to_vec());
        output
    }

    fn frame_sample(&self, input: &[f32], frame_idx: usize, channel: usize) -> f32 {
        if let Some(prev) = &self.previous_frame {
            if frame_idx == 0 {
                return prev[channel];
            }
            return input[(frame_idx - 1) * self.channels + channel];
        }

        input[frame_idx * self.channels + channel]
    }
}

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

fn build_output_stream(
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

fn output_sample_rate_for(spec: AudioSpec) -> Result<u32> {
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
}
