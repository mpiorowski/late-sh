use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use futures_util::{SinkExt, StreamExt};
use nix::{
    libc,
    pty::{Winsize, openpty},
    unistd::setsid,
};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use serde::Deserialize;
use serde_json::json;
use shlex::Shlex;
use std::{
    collections::VecDeque,
    env, fs,
    io::{self, Cursor, IsTerminal, Read, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use symphonia::core::{
    audio::{AudioBufferRef, SampleBuffer},
    codecs::{Decoder, DecoderOptions},
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, ReadOnlySource},
    meta::MetadataOptions,
    probe::Hint,
};
use symphonia::default::{get_codecs, get_probe};
use tokio::{
    process::{Child, Command},
    sync::{broadcast, oneshot},
    time::interval,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

const CLI_MODE_ENV: &str = "LATE_CLI_MODE";
const CLI_TOKEN_PREFIX: &str = "LATE_SESSION_TOKEN=";
const DEFAULT_SSH_TARGET: &str = "late.sh";
const DEFAULT_AUDIO_BASE_URL: &str = "https://audio.late.sh";
const DEFAULT_API_BASE_URL: &str = "https://api.late.sh";
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_ulong = libc::TIOCSCTTY as libc::c_ulong;
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
const TIOCSCTTY_IOCTL_REQUEST: libc::c_ulong = libc::TIOCSCTTY;

#[derive(Debug, Clone)]
struct Config {
    ssh_target: String,
    ssh_bin: Vec<String>,
    audio_base_url: String,
    api_base_url: String,
    verbose: bool,
}

struct RawModeGuard(bool);

impl RawModeGuard {
    fn enable_if_tty() -> Self {
        if !std::io::stdin().is_terminal() {
            return Self(false);
        }
        match enable_raw_mode() {
            Ok(()) => Self(true),
            Err(err) => {
                eprintln!("warning: failed to enable raw mode: {err}");
                Self(false)
            }
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = disable_raw_mode();
        }
    }
}

#[derive(Debug, Clone)]
struct VizSample {
    bands: [f32; 8],
    rms: f32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
}

struct AudioRuntime {
    _stream: cpal::Stream,
    analyzer_tx: broadcast::Sender<VizSample>,
    played_samples: Arc<AtomicU64>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
    muted: Arc<AtomicBool>,
    volume_percent: Arc<AtomicU8>,
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

enum SshExit {
    Process(std::process::ExitStatus),
    StdoutClosed,
}

struct SymphoniaStreamDecoder {
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

struct StreamingLinearResampler {
    channels: usize,
    source_rate: u32,
    target_rate: u32,
    position: f64,
    previous_frame: Option<Vec<f32>>,
}

struct SshProcess {
    child: Child,
    output_task: tokio::task::JoinHandle<Result<()>>,
    input_task: tokio::task::JoinHandle<Result<()>>,
    resize_handle: PtyResizeHandle,
    input_gate: Arc<AtomicBool>,
}

#[derive(Clone)]
struct PtyResizeHandle {
    master: Arc<fs::File>,
}

impl PtyResizeHandle {
    fn resize_to_current(&self) -> Result<()> {
        let (cols, rows) = terminal_size_or_default();
        resize_pty(&self.master, cols, rows)
    }
}

impl SymphoniaStreamDecoder {
    fn new_http(url: &str) -> Result<Self> {
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
            let decoder = get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
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
        if self.channels == 0 || input.is_empty() || !input.len().is_multiple_of(self.channels) {
            return Vec::new();
        }

        if self.source_rate == self.target_rate {
            self.previous_frame = Some(input[input.len() - self.channels..input.len()].to_vec());
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

fn push_interleaved_samples(out: &mut Vec<f32>, decoded: AudioBufferRef<'_>) -> Result<()> {
    let spec = *decoded.spec();
    let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
    buf.copy_interleaved_ref(decoded);
    out.extend_from_slice(buf.samples());
    Ok(())
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

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_args(env::args().skip(1))?;
    init_logging(config.verbose)?;
    debug!(?config, "resolved cli config");
    let ssh_identity = ensure_client_identity()?;
    let _raw_mode = RawModeGuard::enable_if_tty();

    info!("starting audio runtime");
    let audio = AudioRuntime::start(config.audio_base_url.clone())
        .await
        .map_err(|err| {
            let hint = audio_startup_hint();
            anyhow::anyhow!("failed to start local audio: {err:#}\n\n{hint}")
        })?;
    info!(sample_rate = audio.sample_rate, "audio runtime ready");
    info!("starting ssh session");
    let (token_tx, token_rx) = oneshot::channel();
    let SshProcess {
        mut child,
        mut output_task,
        input_task,
        resize_handle,
        input_gate,
    } = spawn_ssh(&config, &ssh_identity, token_tx).await?;
    let resize_task = tokio::spawn(forward_resize_events(resize_handle));

    let token = tokio::time::timeout(Duration::from_secs(10), token_rx)
        .await
        .context(
            "timed out waiting for SSH session token (is the server reachable? \
             try: ssh late.sh)",
        )?
        .context("ssh session token channel closed")?;
    flush_stdin_input_queue();
    input_gate.store(true, Ordering::Relaxed);
    info!("received session token and starting websocket pairing");

    let api_base_url = config.api_base_url.clone();
    let played_samples = Arc::clone(&audio.played_samples);
    let sample_rate = audio.sample_rate;
    let muted = Arc::clone(&audio.muted);
    let volume_percent = Arc::clone(&audio.volume_percent);
    let mut frames = audio.analyzer_tx.subscribe();

    let ws_task = tokio::spawn(async move {
        let mut retries = 0;
        const MAX_RETRIES: usize = 10;
        loop {
            if let Err(err) = run_viz_ws(
                &api_base_url,
                &token,
                &mut frames,
                &played_samples,
                sample_rate,
                &muted,
                &volume_percent,
            )
            .await
            {
                retries += 1;
                if retries > MAX_RETRIES {
                    error!(error = ?err, "visualizer websocket task failed {MAX_RETRIES} times consecutively; giving up");
                    break;
                }
                error!(error = ?err, attempt = retries, "visualizer websocket task failed; reconnecting in 2s...");
            } else {
                retries = 0;
                info!("visualizer websocket closed cleanly; reconnecting in 2s...");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    let mut stdout_result = None;
    let mut stdout_task_consumed = false;
    let status = match tokio::select! {
        status = child.wait() => {
            let status = status.context("ssh process failed to exit cleanly")?;
            SshExit::Process(status)
        }
        stdout = &mut output_task => {
            stdout_task_consumed = true;
            match stdout {
                Ok(Ok(())) => {
                    info!("ssh stdout closed; treating session as ended");
                    stdout_result = Some(Ok(Ok(())));
                }
                Ok(Err(err)) => return Err(err.context("ssh stdout forwarding failed")),
                Err(err) => return Err(anyhow::anyhow!("ssh stdout task join failed: {err}")),
            }
            SshExit::StdoutClosed
        }
    } {
        SshExit::Process(status) => {
            info!(%status, "ssh session exited");
            Some(status)
        }
        SshExit::StdoutClosed => {
            if let Err(err) = child.start_kill() {
                debug!(error = ?err, "failed to kill lingering ssh wrapper after stdout closed");
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            None
        }
    };

    audio.stop.store(true, Ordering::Relaxed);
    resize_task.abort();
    input_task.abort();
    ws_task.abort();
    if !stdout_task_consumed && output_task.is_finished() {
        stdout_result = Some(output_task.await);
    } else if !stdout_task_consumed {
        output_task.abort();
        let _ = output_task.await;
    }

    if let Some(status) = status {
        let stdout_closed_cleanly = matches!(stdout_result, Some(Ok(Ok(()))));
        if !(status.success() || status.code() == Some(255) && stdout_closed_cleanly) {
            anyhow::bail!("ssh exited with status {status}");
        }
    }

    Ok(())
}

impl Config {
    fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut ssh_target =
            env::var("LATE_SSH_TARGET").unwrap_or_else(|_| DEFAULT_SSH_TARGET.to_string());
        let mut ssh_bin =
            parse_ssh_bin_spec(&env::var("LATE_SSH_BIN").unwrap_or_else(|_| "ssh".to_string()))?;
        let mut audio_base_url =
            env::var("LATE_AUDIO_BASE_URL").unwrap_or_else(|_| DEFAULT_AUDIO_BASE_URL.to_string());
        let mut api_base_url =
            env::var("LATE_API_BASE_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
        let mut verbose = false;

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ssh-target" => ssh_target = next_value(&mut args, "--ssh-target")?,
                "--ssh-bin" => ssh_bin = parse_ssh_bin_spec(&next_value(&mut args, "--ssh-bin")?)?,
                "--audio-base-url" => audio_base_url = next_value(&mut args, "--audio-base-url")?,
                "--api-base-url" => api_base_url = next_value(&mut args, "--api-base-url")?,
                "--verbose" | "-v" => verbose = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument '{other}'"),
            }
        }

        Ok(Self {
            ssh_target,
            ssh_bin,
            audio_base_url,
            api_base_url,
            verbose,
        })
    }
}

fn init_logging(verbose: bool) -> Result<()> {
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) if verbose => EnvFilter::new("warn,symphonia=error,late=debug"),
        Err(_) => return Ok(()),
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize logging: {err}"))?;

    Ok(())
}

impl AudioRuntime {
    async fn start(audio_base_url: String) -> Result<Self> {
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

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {flag}"))
}

fn print_help() {
    println!(
        "late\n\
         \n\
         Minimal local launcher for late.sh.\n\
         \n\
         Options:\n\
           --ssh-target <host>        SSH target (default: late.sh)\n\
           --ssh-bin <command>        SSH client command, including optional args (default: ssh)\n\
           --audio-base-url <url>     Audio base URL, without or with /stream\n\
           --api-base-url <url>       API base URL used for /api/ws/pair\n\
           -v, --verbose              Enable debug logging to stderr\n\
         \n\
         Runtime hotkeys:\n\
           No local audio hotkeys; use the paired TUI client controls.\n"
    );
}

fn ensure_client_identity() -> Result<PathBuf> {
    let dedicated_key = dedicated_identity_path()?;
    if dedicated_key.exists() {
        return Ok(dedicated_key);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "no SSH identity found; generate {} manually or rerun in an interactive terminal",
            dedicated_key.display()
        );
    }

    prompt_generate_identity(&dedicated_key)?;
    Ok(dedicated_key)
}

fn ssh_dir() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".ssh"))
}

fn dedicated_identity_path() -> Result<PathBuf> {
    Ok(ssh_dir()?.join("id_late_sh_ed25519"))
}

fn prompt_generate_identity(path: &Path) -> Result<()> {
    use std::io::Write;

    print!(
        "No SSH key found for late.sh.\n\
         Generate a dedicated Ed25519 key at {}? [y/N]: ",
        path.display()
    );
    std::io::stdout()
        .flush()
        .context("failed to flush prompt")?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("failed to read prompt response")?;

    if !is_affirmative(input.trim()) {
        anyhow::bail!("SSH key generation declined");
    }

    generate_identity(path)
}

fn is_affirmative(input: &str) -> bool {
    matches!(input, "y" | "Y" | "yes" | "YES" | "Yes")
}

fn generate_identity(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("generated identity path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }

    let status = std::process::Command::new("ssh-keygen")
        .arg("-t")
        .arg("ed25519")
        .arg("-f")
        .arg(path)
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg("late.sh cli")
        .status()
        .context("failed to run ssh-keygen")?;

    if !status.success() {
        anyhow::bail!("ssh-keygen exited with status {status}");
    }

    Ok(())
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

fn probe_stream_spec(audio_base_url: &str) -> Result<AudioSpec> {
    let decoder = SymphoniaStreamDecoder::new_http(&trim_stream_suffix(audio_base_url))
        .context("failed to create audio decoder for stream probe")?;
    Ok(decoder.spec())
}

fn audio_startup_hint() -> String {
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

                    match SymphoniaStreamDecoder::new_http(&trim_stream_suffix(&audio_base_url)) {
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
                    let samples: Vec<f32> = played_ring.iter().skip(start).copied().collect();
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

async fn spawn_ssh(
    config: &Config,
    identity_file: &Path,
    token_tx: oneshot::Sender<String>,
) -> Result<SshProcess> {
    let (cols, rows) = terminal_size_or_default();
    let winsize = pty_winsize(cols, rows);
    let pty = openpty(Some(&winsize), None).context("failed to allocate local ssh pty")?;
    let master = Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    let (ssh_program, ssh_args) = config
        .ssh_bin
        .split_first()
        .context("ssh client command is empty")?;
    let mut cmd = Command::new(ssh_program);
    cmd.env(CLI_MODE_ENV, "1")
        .args(ssh_args)
        .arg("-i")
        .arg(identity_file)
        .arg("-tt")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg(format!("SendEnv={CLI_MODE_ENV}"))
        .arg(&config.ssh_target)
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("failed to clone ssh pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("failed to clone ssh pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("failed to clone ssh pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    unsafe {
        cmd.pre_exec(move || {
            setsid().map_err(nix_to_io_error)?;
            if libc::ioctl(slave_fd, TIOCSCTTY_IOCTL_REQUEST, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn().context("failed to start ssh session")?;
    drop(slave);

    let output_pty = master
        .try_clone()
        .context("failed to clone ssh pty master for output forwarding")?;
    let input_pty = master
        .try_clone()
        .context("failed to clone ssh pty master for input forwarding")?;
    let input_gate = Arc::new(AtomicBool::new(false));
    let input_gate_for_task = Arc::clone(&input_gate);

    let output_task = tokio::task::spawn_blocking(move || forward_ssh_output(output_pty, token_tx));
    let input_task =
        tokio::task::spawn_blocking(move || forward_stdin(input_pty, input_gate_for_task));

    Ok(SshProcess {
        child,
        output_task,
        input_task,
        resize_handle: PtyResizeHandle { master },
        input_gate,
    })
}

fn parse_ssh_bin_spec(spec: &str) -> Result<Vec<String>> {
    let parts: Vec<String> = Shlex::new(spec).collect();
    if parts.is_empty() {
        anyhow::bail!("ssh client command cannot be empty");
    }
    Ok(parts)
}

fn terminal_size_or_default() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

fn pty_winsize(cols: u16, rows: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

fn nix_to_io_error(err: nix::Error) -> io::Error {
    io::Error::from_raw_os_error(err as i32)
}

fn resize_pty(master: &fs::File, cols: u16, rows: u16) -> Result<()> {
    let winsize = pty_winsize(cols, rows);
    let rc = unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &winsize) };
    if rc == -1 {
        return Err(io::Error::last_os_error()).context("failed to resize local ssh pty");
    }
    debug!(cols, rows, "resized local ssh pty");
    Ok(())
}

async fn forward_resize_events(handle: PtyResizeHandle) {
    let Ok(mut sigwinch) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
    else {
        return;
    };

    while sigwinch.recv().await.is_some() {
        if let Err(err) = handle.resize_to_current() {
            debug!(error = ?err, "failed to forward local terminal resize");
        }
    }
}

fn forward_ssh_output(mut pty: fs::File, token_tx: oneshot::Sender<String>) -> Result<()> {
    let mut pending = Vec::new();
    let mut buf = [0u8; 4096];
    let mut out = std::io::stdout();
    let mut token_sent = false;
    let mut token_tx = Some(token_tx);

    loop {
        let n = match pty.read(&mut buf) {
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        };
        if n == 0 {
            break;
        }

        if token_sent {
            out.write_all(&buf[..n])?;
            out.flush()?;
            continue;
        }

        pending.extend_from_slice(&buf[..n]);

        while !pending.is_empty() && !token_sent {
            match parse_cli_banner(&pending) {
                BannerState::NeedMore => break,
                BannerState::Token { token, consumed } => {
                    if let Some(token_tx) = token_tx.take() {
                        let _ = token_tx.send(token);
                    }
                    debug!("captured cli session token banner");
                    if consumed < pending.len() {
                        out.write_all(&pending[consumed..])?;
                        out.flush()?;
                    }
                    pending.clear();
                    token_sent = true;
                }
                BannerState::Passthrough { consumed } => {
                    out.write_all(&pending[..consumed])?;
                    out.flush()?;
                    pending.drain(..consumed);
                }
            }
        }
    }

    if !pending.is_empty() {
        out.write_all(&pending)?;
        out.flush()?;
    }

    Ok(())
}

fn flush_stdin_input_queue() {
    if !std::io::stdin().is_terminal() {
        return;
    }

    let rc = unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) };
    if rc == -1 {
        debug!(
            error = ?io::Error::last_os_error(),
            "failed to flush pending stdin before enabling ssh input"
        );
    }
}

fn forward_stdin(mut pty: fs::File, input_gate: Arc<AtomicBool>) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stdin.read(&mut buf) {
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        };
        if n == 0 {
            break;
        }
        if !input_gate.load(Ordering::Relaxed) {
            continue;
        }
        pty.write_all(&buf[..n])?;
    }
    Ok(())
}

async fn run_viz_ws(
    api_base_url: &str,
    token: &str,
    frames: &mut broadcast::Receiver<VizSample>,
    played_samples: &AtomicU64,
    sample_rate: u32,
    muted: &AtomicBool,
    volume_percent: &AtomicU8,
) -> Result<()> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!(%ws_url, "connecting pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .with_context(|| format!("timed out connecting to pair websocket at {ws_url}"))?
        .with_context(|| format!("failed to connect to pair websocket at {ws_url}"))?;
    info!("pair websocket established");
    let mut heartbeat = interval(Duration::from_secs(1));
    send_client_state(&mut ws, muted, volume_percent).await?;

    loop {
        tokio::select! {
            recv = frames.recv() => {
                let frame = match recv {
                    Ok(frame) => frame,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let position_ms = playback_position_ms(played_samples, sample_rate);
                let payload = json!({
                    "event": "viz",
                    "position_ms": position_ms,
                    "bands": frame.bands,
                    "rms": frame.rms,
                });
                ws.send(Message::Text(payload.to_string().into())).await?;
            }
            _ = heartbeat.tick() => {
                let payload = json!({
                    "event": "heartbeat",
                    "position_ms": playback_position_ms(played_samples, sample_rate),
                });
                ws.send(Message::Text(payload.to_string().into())).await?;
            }
            maybe_msg = ws.next() => {
                let Some(msg) = maybe_msg else {
                    break;
                };
                match msg? {
                    Message::Text(text) if apply_pair_control(&text, muted, volume_percent)? => {
                            send_client_state(&mut ws, muted, volume_percent).await?;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

async fn send_client_state(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    muted: &AtomicBool,
    volume_percent: &AtomicU8,
) -> Result<()> {
    let payload = json!({
        "event": "client_state",
        "client_kind": "cli",
        "muted": muted.load(Ordering::Relaxed),
        "volume_percent": volume_percent.load(Ordering::Relaxed),
    });
    ws.send(Message::Text(payload.to_string().into())).await?;
    Ok(())
}

fn apply_pair_control(text: &str, muted: &AtomicBool, volume_percent: &AtomicU8) -> Result<bool> {
    match serde_json::from_str::<PairControlMessage>(text)? {
        PairControlMessage::ToggleMute => {
            let now_muted = muted.fetch_xor(true, Ordering::Relaxed) ^ true;
            info!(muted = now_muted, "applied paired mute toggle");
            Ok(true)
        }
        PairControlMessage::VolumeUp => {
            let new_volume = bump_volume(volume_percent, 5);
            info!(volume_percent = new_volume, "applied paired volume up");
            Ok(true)
        }
        PairControlMessage::VolumeDown => {
            let new_volume = bump_volume(volume_percent, -5);
            info!(volume_percent = new_volume, "applied paired volume down");
            Ok(true)
        }
    }
}

fn bump_volume(volume_percent: &AtomicU8, delta: i16) -> u8 {
    let current = volume_percent.load(Ordering::Relaxed) as i16;
    let next = (current + delta).clamp(0, 100) as u8;
    volume_percent.store(next, Ordering::Relaxed);
    next
}

fn playback_position_ms(played_samples: &AtomicU64, sample_rate: u32) -> u64 {
    played_samples.load(Ordering::Relaxed) * 1000 / sample_rate as u64
}

fn trim_stream_suffix(audio_base_url: &str) -> String {
    audio_base_url
        .trim_end_matches('/')
        .trim_end_matches("/stream")
        .to_string()
}

fn pair_ws_url(api_base_url: &str, token: &str) -> Result<String> {
    let base = api_base_url.trim_end_matches('/');
    let scheme_fixed = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if base.starts_with("ws://") || base.starts_with("wss://") {
        base.to_string()
    } else {
        anyhow::bail!("api base url must start with http://, https://, ws://, or wss://");
    };

    Ok(format!(
        "{}/api/ws/pair?token={token}",
        scheme_fixed.trim_end_matches('/')
    ))
}

enum BannerState {
    NeedMore,
    Token { token: String, consumed: usize },
    Passthrough { consumed: usize },
}

fn parse_cli_banner(buf: &[u8]) -> BannerState {
    let Some(newline_idx) = buf.iter().position(|b| *b == b'\n') else {
        return BannerState::NeedMore;
    };

    let line = &buf[..=newline_idx];
    let text = String::from_utf8_lossy(line);
    if let Some(rest) = text.strip_prefix(CLI_TOKEN_PREFIX) {
        return BannerState::Token {
            token: rest.trim().to_string(),
            consumed: newline_idx + 1,
        };
    }

    BannerState::Passthrough {
        consumed: newline_idx + 1,
    }
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
    fn affirmative_prompt_accepts_expected_inputs() {
        assert!(is_affirmative("y"));
        assert!(is_affirmative("Y"));
        assert!(is_affirmative("yes"));
        assert!(!is_affirmative("n"));
        assert!(!is_affirmative(""));
    }

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

    #[test]
    fn pair_ws_url_rewrites_scheme() {
        assert_eq!(
            pair_ws_url("https://api.late.sh", "abc").unwrap(),
            "wss://api.late.sh/api/ws/pair?token=abc"
        );
        assert_eq!(
            pair_ws_url("http://localhost:4000", "abc").unwrap(),
            "ws://localhost:4000/api/ws/pair?token=abc"
        );
    }

    #[test]
    fn apply_pair_control_toggles_muted_state() {
        let muted = AtomicBool::new(false);
        let volume_percent = AtomicU8::new(100);

        apply_pair_control(r#"{"event":"toggle_mute"}"#, &muted, &volume_percent).unwrap();
        assert!(muted.load(Ordering::Relaxed));

        apply_pair_control(r#"{"event":"toggle_mute"}"#, &muted, &volume_percent).unwrap();
        assert!(!muted.load(Ordering::Relaxed));
    }

    #[test]
    fn apply_pair_control_adjusts_volume() {
        let muted = AtomicBool::new(false);
        let volume_percent = AtomicU8::new(50);

        apply_pair_control(r#"{"event":"volume_up"}"#, &muted, &volume_percent).unwrap();
        assert_eq!(volume_percent.load(Ordering::Relaxed), 55);

        apply_pair_control(r#"{"event":"volume_down"}"#, &muted, &volume_percent).unwrap();
        assert_eq!(volume_percent.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn parse_cli_banner_extracts_token_and_consumed_bytes() {
        let buf = b"LATE_SESSION_TOKEN=abc-123\r\n\x1b[?1049h";
        match parse_cli_banner(buf) {
            BannerState::Token { token, consumed } => {
                assert_eq!(token, "abc-123");
                assert_eq!(consumed, 28);
            }
            _ => panic!("expected token banner"),
        }
    }

    #[test]
    fn parse_cli_banner_passthroughs_regular_output() {
        let buf = b"hello\r\nworld";
        match parse_cli_banner(buf) {
            BannerState::Passthrough { consumed } => assert_eq!(consumed, 7),
            _ => panic!("expected passthrough"),
        }
    }

    #[test]
    fn terminal_size_default_fallback_is_sane() {
        let (cols, rows) = terminal_size_or_default();
        assert!(cols > 0);
        assert!(rows > 0);
    }

    #[test]
    fn pty_winsize_maps_rows_and_cols() {
        let winsize = pty_winsize(120, 40);
        assert_eq!(winsize.ws_col, 120);
        assert_eq!(winsize.ws_row, 40);
    }

    #[test]
    fn parse_ssh_bin_spec_splits_command_and_args() {
        assert_eq!(
            parse_ssh_bin_spec("ssh -p 2222").unwrap(),
            vec!["ssh".to_string(), "-p".to_string(), "2222".to_string()]
        );
    }

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
