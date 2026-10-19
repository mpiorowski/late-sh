//! Watch: plays a video the TUI asks for inside the user's own terminal,
//! through their `mpv` (kitty graphics output, shared-memory frames) and the
//! `yt-dlp` mpv's ytdl hook calls. The server never touches media: the TUI
//! reserves a black rectangle, sends `video_play` with it, and keeps every
//! key. This module owns the mpv child and its IPC socket; it does the work
//! and returns data, and the pair loop in `ws.rs` sends, logs, and silences
//! the house audio around it.

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env,
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

use super::{
    config::SshMode,
    ws::{open_state_log, state_log_path},
};

/// The first mpv release with `--vo=kitty`.
const MIN_MPV: (u32, u32) = (0, 36);
/// How long a fresh mpv gets to open its IPC socket before the play fails.
const IPC_CONNECT_DEADLINE: Duration = Duration::from_secs(5);
const IPC_REPLY_TIMEOUT: Duration = Duration::from_millis(500);
/// How long `quit` gets before the child is killed.
const QUIT_GRACE: Duration = Duration::from_millis(500);
const QUIT_POLL: Duration = Duration::from_millis(50);
const SEEK_STEP_S: i64 = 10;
const YTDL_FORMAT: &str = "bestvideo[height<=720]+bestaudio/best[height<=720]";

/// One failing precondition for the `video` capability. Sent as
/// `client_state.video_blockers` so the server's setup box can say which
/// rows fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VideoBlocker {
    /// OpenSSH and old modes: the CLI does not own the tty.
    NotNative,
    /// Only Linux and macOS have been tried.
    Platform,
    /// tmux or screen sits between mpv and the terminal.
    Multiplexer,
    /// The env does not look like a Kitty-graphics terminal.
    NotKitty,
    /// The tty reports no pixel size, so mpv cannot size its placement.
    NoPixelSize,
    NoMpv,
    /// Older than 0.36, or a version string that does not parse.
    MpvTooOld,
    NoYtDlp,
}

impl VideoBlocker {
    pub(super) fn wire_name(self) -> &'static str {
        match self {
            Self::NotNative => "not_native",
            Self::Platform => "platform",
            Self::Multiplexer => "multiplexer",
            Self::NotKitty => "not_kitty",
            Self::NoPixelSize => "no_pixel_size",
            Self::NoMpv => "no_mpv",
            Self::MpvTooOld => "mpv_too_old",
            Self::NoYtDlp => "no_ytdlp",
        }
    }
}

/// What `mpv --version` said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MpvProbe {
    NotFound,
    Found { major: u32, minor: u32 },
    Unrecognized,
}

/// The CLI's own tty as `TIOCGWINSZ` reports it, every field nonzero. A
/// terminal that reports zero pixels cannot host the video, so it has no
/// `TtySize` at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TtySize {
    cols: u16,
    rows: u16,
    xpixel: u16,
    ypixel: u16,
}

impl TtySize {
    pub(super) fn new(cols: u16, rows: u16, xpixel: u16, ypixel: u16) -> Option<Self> {
        match (cols, rows, xpixel, ypixel) {
            (0, _, _, _) | (_, 0, _, _) | (_, _, 0, _) | (_, _, _, 0) => None,
            _ => Some(Self {
                cols,
                rows,
                xpixel,
                ypixel,
            }),
        }
    }

    #[cfg(unix)]
    fn read() -> Option<Self> {
        use nix::libc;

        let mut winsize = libc::winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCGWINSZ only writes a `winsize` into the pointer given.
        let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize) };
        match rc {
            0 => Self::new(
                winsize.ws_col,
                winsize.ws_row,
                winsize.ws_xpixel,
                winsize.ws_ypixel,
            ),
            _ => None,
        }
    }

    #[cfg(not(unix))]
    fn read() -> Option<Self> {
        None
    }

    /// The pixel size of `rect`, from this tty's cells.
    fn pixels_for(self, rect: VideoRect) -> (u32, u32) {
        (
            u32::from(rect.cols) * u32::from(self.xpixel) / u32::from(self.cols),
            u32::from(rect.rows) * u32::from(self.ypixel) / u32::from(self.rows),
        )
    }
}

/// The rectangle the TUI reserved, in 0-based terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VideoRect {
    pub(super) left: u16,
    pub(super) top: u16,
    pub(super) cols: u16,
    pub(super) rows: u16,
}

impl VideoRect {
    /// The middle cell, 1-based, which the video always covers whatever its
    /// aspect (mpv centers it inside the rectangle).
    fn center_cell(self) -> (u32, u32) {
        (
            u32::from(self.left) + u32::from(self.cols) / 2 + 1,
            u32::from(self.top) + u32::from(self.rows) / 2 + 1,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum VideoControl {
    TogglePause,
    SeekBack,
    SeekForward,
    ToggleMute,
}

/// Reported to the server as `video_state`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VideoState {
    Starting,
    Playing {
        position_s: u64,
        duration_s: Option<u64>,
    },
    Paused {
        position_s: u64,
        duration_s: Option<u64>,
    },
    /// mpv reached the end and exited.
    Ended,
    Failed(VideoFailure),
    /// The server asked for the stop and mpv is gone. Like `Ended` and
    /// `Failed` it is final: whatever mpv left in the terminal is settled.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VideoFailure {
    /// Not an `http://` or `https://` URL; never handed to mpv.
    UnsupportedUrl,
    /// Neither now nor at startup did the tty report a pixel size.
    NoPixelSize,
    /// mpv could not be started.
    Spawn(String),
    /// mpv could not be inspected after it started.
    Inspect(String),
    /// mpv started but never opened its IPC socket.
    IpcTimeout,
    /// The IPC socket broke while mpv was running.
    Ipc(String),
    /// mpv exited before the video ended; `None` when a signal ended it.
    Exited(Option<i32>),
}

impl VideoFailure {
    /// The `video_state.message` text, for the server's log only.
    pub(super) fn message(&self) -> String {
        match self {
            Self::UnsupportedUrl => "refused a non-http url".to_string(),
            Self::NoPixelSize => "the terminal reports no pixel size".to_string(),
            Self::Spawn(error) => format!("failed to start mpv: {error}"),
            Self::Inspect(error) => format!("failed to inspect mpv: {error}"),
            Self::IpcTimeout => "mpv never opened its ipc socket".to_string(),
            Self::Ipc(error) => format!("mpv ipc failed: {error}"),
            Self::Exited(Some(code)) => format!("mpv exited with status {code}"),
            Self::Exited(None) => "mpv was killed by a signal".to_string(),
        }
    }
}

/// One `video_state` for the server, plus the rectangle whose last mpv
/// frame is now orphaned on screen (`clear_frame`) when a child went away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VideoUpdate {
    pub(super) play_id: u64,
    pub(super) state: VideoState,
    pub(super) residue: Option<VideoRect>,
}

/// Everything a play needs from the machine, found once at startup.
pub(super) struct VideoTools {
    mpv: PathBuf,
    /// `LATE_YTDLP_BIN`; otherwise mpv finds `yt-dlp` on `PATH` itself.
    ytdlp_override: Option<PathBuf>,
    /// Fallback when the tty cannot be read again at spawn.
    startup_tty: Option<TtySize>,
    ipc_path: PathBuf,
    log_path: PathBuf,
}

pub(super) struct VideoProbe {
    pub(super) blockers: Vec<VideoBlocker>,
    pub(super) tools: VideoTools,
}

/// Check every precondition for the `video` capability on this machine.
pub(super) fn probe(ssh_mode: SshMode, platform: &str) -> VideoProbe {
    let vars: HashMap<String, String> = env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let tty = TtySize::read();
    let mpv = nonempty_env_path("LATE_MPV_BIN").unwrap_or_else(|| PathBuf::from("mpv"));
    let mpv_probe = probe_mpv(&mpv);
    let ytdlp_override = nonempty_env_path("LATE_YTDLP_BIN");
    let ytdlp_found = match &ytdlp_override {
        Some(path) => path.is_file(),
        None => on_path("yt-dlp"),
    };
    VideoProbe {
        blockers: blockers(ssh_mode, platform, &vars, tty, mpv_probe, ytdlp_found),
        tools: VideoTools {
            mpv,
            ytdlp_override,
            startup_tty: tty,
            ipc_path: ipc_socket_path(),
            log_path: state_log_path("LATE_VIDEO_LOG", "video.log"),
        },
    }
}

/// Every failing precondition, not the first. Empty means `video` is
/// advertised.
pub(super) fn blockers(
    ssh_mode: SshMode,
    platform: &str,
    vars: &HashMap<String, String>,
    tty: Option<TtySize>,
    mpv: MpvProbe,
    ytdlp_found: bool,
) -> Vec<VideoBlocker> {
    let mut blockers = Vec::new();
    match ssh_mode {
        SshMode::Native => {}
        SshMode::OpenSsh | SshMode::Subprocess => blockers.push(VideoBlocker::NotNative),
    }
    match platform {
        "linux" | "macos" => {}
        _ => blockers.push(VideoBlocker::Platform),
    }
    if in_multiplexer(vars) {
        blockers.push(VideoBlocker::Multiplexer);
    }
    if !is_kitty_family(vars) {
        blockers.push(VideoBlocker::NotKitty);
    }
    match tty {
        Some(_) => {}
        None => blockers.push(VideoBlocker::NoPixelSize),
    }
    match mpv {
        MpvProbe::Found { major, minor } if (major, minor) >= MIN_MPV => {}
        MpvProbe::Found { .. } | MpvProbe::Unrecognized => blockers.push(VideoBlocker::MpvTooOld),
        MpvProbe::NotFound => blockers.push(VideoBlocker::NoMpv),
    }
    if !ytdlp_found {
        blockers.push(VideoBlocker::NoYtDlp);
    }
    blockers
}

fn env_value<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}

fn in_multiplexer(vars: &HashMap<String, String>) -> bool {
    env_value(vars, "TMUX").is_some()
        || env_value(vars, "STY").is_some()
        || env_value(vars, "TERM")
            .is_some_and(|term| term.starts_with("tmux") || term.starts_with("screen"))
}

/// The same hints the CLI forwards for images. The server's own detection is
/// the second opinion.
fn is_kitty_family(vars: &HashMap<String, String>) -> bool {
    env_value(vars, "TERM") == Some("xterm-kitty")
        || env_value(vars, "KITTY_WINDOW_ID").is_some()
        || matches!(
            env_value(vars, "TERM_PROGRAM"),
            Some("ghostty" | "WezTerm" | "rio")
        )
        || env_value(vars, "GHOSTTY_RESOURCES_DIR").is_some()
        || env_value(vars, "WEZTERM_EXECUTABLE").is_some()
        || env_value(vars, "KONSOLE_VERSION").is_some()
}

/// Parse the first line of `mpv --version`: `mpv v0.41.0 Copyright ...`,
/// `mpv 0.36.0`, or a git build's `mpv v0.38.0-dirty`.
pub(super) fn parse_mpv_version(text: &str) -> MpvProbe {
    let Some(rest) = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("mpv "))
    else {
        return MpvProbe::Unrecognized;
    };
    let rest = rest.strip_prefix('v').unwrap_or(rest);
    let mut numbers = rest.split(|c: char| !c.is_ascii_digit());
    let major = numbers.next().and_then(|part| part.parse().ok());
    let minor = numbers.next().and_then(|part| part.parse().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => MpvProbe::Found { major, minor },
        _ => MpvProbe::Unrecognized,
    }
}

fn probe_mpv(mpv: &Path) -> MpvProbe {
    let output = Command::new(mpv)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) => parse_mpv_version(&String::from_utf8_lossy(&output.stdout)),
        // Missing or not runnable reads the same to the user: no usable mpv.
        Err(_) => MpvProbe::NotFound,
    }
}

fn on_path(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

fn nonempty_env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn ipc_socket_path() -> PathBuf {
    let dir = nonempty_env_path("XDG_RUNTIME_DIR").unwrap_or_else(env::temp_dir);
    dir.join(format!("late-video-{}.sock", std::process::id()))
}

/// Where a respawned mpv picks up after a geometry change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Resume {
    pub(super) position_s: f64,
    pub(super) paused: bool,
    pub(super) muted: bool,
}

pub(super) struct MpvLaunch<'a> {
    pub(super) url: &'a str,
    pub(super) rect: VideoRect,
    pub(super) tty: TtySize,
    pub(super) volume_percent: u8,
    pub(super) resume: Option<Resume>,
    pub(super) ipc_path: &'a Path,
    pub(super) ytdlp_override: Option<&'a Path>,
}

/// The whole mpv command line. The six geometry numbers are the contract
/// with the TUI: mpv's `--vo-kitty-left/top` are 1-based where the TUI's
/// cells are 0-based, and the pixel size comes from this tty's cell size.
///
/// `--vo-kitty-config-clear=no` is mandatory: `yes` writes `ESC[2J` and
/// wipes the TUI around the video on every reconfigure. `--vo-kitty-alt-screen=no`
/// is mandatory too: `yes` switches to the alternate screen and wipes the
/// TUI on exit. With it off mpv leaves its last frame on screen, which is
/// what `clear_frame` is for.
pub(super) fn mpv_args(launch: &MpvLaunch<'_>) -> Vec<OsString> {
    let MpvLaunch {
        url,
        rect,
        tty,
        volume_percent,
        resume,
        ipc_path,
        ytdlp_override,
    } = launch;
    let (width, height) = tty.pixels_for(*rect);
    let mut args: Vec<OsString> = [
        "--no-config",
        "--really-quiet",
        "--terminal=no",
        "--vo=kitty",
        "--vo-kitty-use-shm=yes",
        "--vo-kitty-alt-screen=no",
        "--vo-kitty-config-clear=no",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.extend(
        [
            format!("--vo-kitty-left={}", u32::from(rect.left) + 1),
            format!("--vo-kitty-top={}", u32::from(rect.top) + 1),
            format!("--vo-kitty-cols={}", rect.cols),
            format!("--vo-kitty-rows={}", rect.rows),
            format!("--vo-kitty-width={width}"),
            format!("--vo-kitty-height={height}"),
            "--ytdl=yes".to_string(),
            format!("--ytdl-format={YTDL_FORMAT}"),
        ]
        .map(OsString::from),
    );
    if let Some(ytdlp) = ytdlp_override {
        let mut arg = OsString::from("--script-opts=ytdl_hook-ytdl_path=");
        arg.push(ytdlp);
        args.push(arg);
    }
    args.push(format!("--volume={volume_percent}").into());
    args.push("--keep-open=no".into());
    match resume {
        Some(resume) => {
            args.push(format!("--start={:.1}", resume.position_s).into());
            if resume.paused {
                args.push("--pause".into());
            }
            if resume.muted {
                args.push("--mute=yes".into());
            }
        }
        None => {}
    }
    let mut ipc = OsString::from("--input-ipc-server=");
    ipc.push(ipc_path);
    args.push(ipc);
    args.push("--".into());
    args.push(url.into());
    args
}

/// The mpv IPC message for one control, before its `request_id`.
pub(super) fn ipc_command(control: VideoControl) -> Value {
    match control {
        VideoControl::TogglePause => json!({ "command": ["cycle", "pause"] }),
        VideoControl::SeekBack => json!({ "command": ["seek", -SEEK_STEP_S] }),
        VideoControl::SeekForward => json!({ "command": ["seek", SEEK_STEP_S] }),
        VideoControl::ToggleMute => json!({ "command": ["cycle", "mute"] }),
    }
}

/// The `video_state` pair event for one update.
pub(super) fn video_state_payload(play_id: u64, state: &VideoState) -> Value {
    match state {
        VideoState::Starting => {
            json!({ "event": "video_state", "play_id": play_id, "state": "starting" })
        }
        VideoState::Playing {
            position_s,
            duration_s,
        } => json!({
            "event": "video_state",
            "play_id": play_id,
            "state": "playing",
            "position_s": position_s,
            "duration_s": duration_s,
        }),
        VideoState::Paused {
            position_s,
            duration_s,
        } => json!({
            "event": "video_state",
            "play_id": play_id,
            "state": "paused",
            "position_s": position_s,
            "duration_s": duration_s,
        }),
        VideoState::Ended => {
            json!({ "event": "video_state", "play_id": play_id, "state": "ended" })
        }
        VideoState::Failed(failure) => json!({
            "event": "video_state",
            "play_id": play_id,
            "state": "failed",
            "message": failure.message(),
        }),
        VideoState::Stopped => {
            json!({ "event": "video_state", "play_id": play_id, "state": "stopped" })
        }
    }
}

/// The state mpv's properties describe. No position yet means it is still
/// resolving or buffering.
pub(super) fn playback_state(
    time_pos: Option<f64>,
    paused: bool,
    duration: Option<f64>,
) -> VideoState {
    let Some(time_pos) = time_pos else {
        return VideoState::Starting;
    };
    let position_s = time_pos.max(0.0) as u64;
    let duration_s = duration.map(|duration| duration.max(0.0) as u64);
    match paused {
        true => VideoState::Paused {
            position_s,
            duration_s,
        },
        false => VideoState::Playing {
            position_s,
            duration_s,
        },
    }
}

/// Delete the placement mpv left at `rect`'s center. Kitty's `d=p` deletes
/// only placements intersecting that cell, so the TUI's own images elsewhere
/// survive. `q=2` keeps the terminal from answering on stdin, which the CLI
/// would forward to the server as keys.
pub(super) fn clear_frame(rect: VideoRect) -> io::Result<()> {
    let (x, y) = rect.center_cell();
    let mut out = io::stdout().lock();
    write!(out, "\x1b_Ga=d,d=p,x={x},y={y},q=2\x1b\\")?;
    out.flush()
}

#[cfg(unix)]
type IpcStream = tokio::net::UnixStream;
#[cfg(not(unix))]
type IpcStream = tokio::io::DuplexStream;

#[cfg(unix)]
async fn connect_ipc_stream(path: &Path) -> io::Result<IpcStream> {
    tokio::net::UnixStream::connect(path).await
}

/// `video` is never advertised off Unix, so the server never gets here.
#[cfg(not(unix))]
fn connect_ipc_stream(_path: &Path) -> std::future::Ready<io::Result<IpcStream>> {
    std::future::ready(Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "mpv ipc needs unix sockets",
    )))
}

enum IpcReply {
    Success(Value),
    /// mpv refused: a property not available yet, a seek past the end.
    Error,
}

#[derive(Deserialize)]
struct IpcMessage {
    #[serde(default)]
    request_id: Option<u64>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    data: Value,
}

struct MpvIpc {
    reader: BufReader<ReadHalf<IpcStream>>,
    writer: WriteHalf<IpcStream>,
    next_request_id: u64,
}

impl MpvIpc {
    async fn connect(path: &Path) -> io::Result<Self> {
        let (reader, writer) = tokio::io::split(connect_ipc_stream(path).await?);
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
            next_request_id: 1,
        })
    }

    async fn request(&mut self, mut message: Value) -> io::Result<IpcReply> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        message["request_id"] = json!(request_id);
        let line = format!("{message}\n");
        match tokio::time::timeout(IPC_REPLY_TIMEOUT, self.exchange(&line, request_id)).await {
            Ok(reply) => reply,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "mpv ipc reply timed out",
            )),
        }
    }

    async fn exchange(&mut self, line: &str, request_id: u64) -> io::Result<IpcReply> {
        self.writer.write_all(line.as_bytes()).await?;
        let mut reply = String::new();
        loop {
            reply.clear();
            if self.reader.read_line(&mut reply).await? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "mpv closed its ipc socket",
                ));
            }
            // Events share the socket with replies; anything that is not
            // this request's reply is skipped.
            let Ok(message) = serde_json::from_str::<IpcMessage>(&reply) else {
                continue;
            };
            if message.request_id != Some(request_id) {
                continue;
            }
            return Ok(match message.error.as_deref() {
                Some("success") => IpcReply::Success(message.data),
                _ => IpcReply::Error,
            });
        }
    }

    async fn property(&mut self, name: &str) -> io::Result<Option<Value>> {
        match self
            .request(json!({ "command": ["get_property", name] }))
            .await?
        {
            IpcReply::Success(value) => Ok(Some(value)),
            IpcReply::Error => Ok(None),
        }
    }

    async fn state(&mut self) -> io::Result<VideoState> {
        let time_pos = self.property("time-pos").await?;
        let paused = self.property("pause").await?;
        let duration = self.property("duration").await?;
        Ok(playback_state(
            time_pos.as_ref().and_then(Value::as_f64),
            paused.as_ref().and_then(Value::as_bool).unwrap_or(false),
            duration.as_ref().and_then(Value::as_f64),
        ))
    }

    /// Where to pick up after a respawn; `None` when mpv has no position yet.
    async fn resume(&mut self) -> io::Result<Option<Resume>> {
        let Some(position_s) = self
            .property("time-pos")
            .await?
            .as_ref()
            .and_then(Value::as_f64)
        else {
            return Ok(None);
        };
        let paused = self.property("pause").await?;
        let muted = self.property("mute").await?;
        Ok(Some(Resume {
            position_s,
            paused: paused.as_ref().and_then(Value::as_bool).unwrap_or(false),
            muted: muted.as_ref().and_then(Value::as_bool).unwrap_or(false),
        }))
    }
}

enum IpcLink {
    Waiting { since: Instant },
    Connected(MpvIpc),
}

struct Playback {
    play_id: u64,
    url: String,
    rect: VideoRect,
    volume_percent: u8,
    child: Child,
    ipc: IpcLink,
}

/// At most one mpv per CLI. Owned by the pair loop for the whole session,
/// so it survives reconnects; dropping it kills mpv, so no mpv outlives
/// `late`.
pub(super) struct VideoPlayer {
    tools: VideoTools,
    current: Option<Playback>,
}

impl VideoPlayer {
    pub(super) fn new(tools: VideoTools) -> Self {
        Self {
            tools,
            current: None,
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.current.is_some()
    }

    /// Start `url` in `rect`, stopping whatever was playing first. The old
    /// play gets no report: the server superseded it.
    pub(super) fn play(
        &mut self,
        play_id: u64,
        url: String,
        rect: VideoRect,
        volume_percent: u8,
    ) -> VideoUpdate {
        let residue = self.take_and_kill().map(|old| old.rect);
        let state = match self.spawn(play_id, url, rect, volume_percent, None) {
            Ok(playback) => {
                self.current = Some(playback);
                VideoState::Starting
            }
            Err(failure) => VideoState::Failed(failure),
        };
        VideoUpdate {
            play_id,
            state,
            residue,
        }
    }

    /// Move the video to `rect`. The kitty output options are startup
    /// options, so this respawns mpv at the same position, pause and mute.
    pub(super) async fn place(&mut self, play_id: u64, rect: VideoRect) -> Option<VideoUpdate> {
        let playback = self.current_for(play_id)?;
        if playback.rect == rect {
            return None;
        }
        let resume = match &mut playback.ipc {
            IpcLink::Connected(ipc) => match ipc.resume().await {
                Ok(resume) => resume,
                Err(error) => return Some(self.fail(VideoFailure::Ipc(error.to_string()))),
            },
            IpcLink::Waiting { .. } => None,
        };
        let old = self.take_and_kill()?;
        let state = match self.spawn(play_id, old.url, rect, old.volume_percent, resume) {
            Ok(playback) => {
                self.current = Some(playback);
                VideoState::Starting
            }
            Err(failure) => VideoState::Failed(failure),
        };
        Some(VideoUpdate {
            play_id,
            state,
            residue: Some(old.rect),
        })
    }

    /// Relay a key. A control for another play, or before mpv's socket is
    /// up, is dropped.
    pub(super) async fn control(
        &mut self,
        play_id: u64,
        control: VideoControl,
    ) -> Option<VideoUpdate> {
        let playback = self.current_for(play_id)?;
        let IpcLink::Connected(ipc) = &mut playback.ipc else {
            return None;
        };
        match ipc.request(ipc_command(control)).await {
            Ok(IpcReply::Success(_) | IpcReply::Error) => None,
            Err(error) => Some(self.fail(VideoFailure::Ipc(error.to_string()))),
        }
    }

    /// Ask mpv to quit, then kill it if it has not within `QUIT_GRACE`.
    pub(super) async fn stop(&mut self, play_id: u64) -> Option<VideoUpdate> {
        let playback = self.current_for(play_id)?;
        if let IpcLink::Connected(ipc) = &mut playback.ipc {
            match ipc.request(json!({ "command": ["quit"] })).await {
                // A broken socket only means the kill below does the work.
                Ok(_) | Err(_) => {}
            }
        }
        let deadline = Instant::now() + QUIT_GRACE;
        while Instant::now() < deadline {
            match playback.child.try_wait() {
                Ok(None) => tokio::time::sleep(QUIT_POLL).await,
                Ok(Some(_)) | Err(_) => break,
            }
        }
        let old = self.take_and_kill()?;
        Some(VideoUpdate {
            play_id,
            state: VideoState::Stopped,
            residue: Some(old.rect),
        })
    }

    /// The 1 s tick: notice an exit, finish connecting the IPC socket, and
    /// report position. Reports every tick while playing or paused, never a
    /// repeat `starting`.
    pub(super) async fn poll(&mut self) -> Option<VideoUpdate> {
        let playback = self.current.as_mut()?;
        match playback.child.try_wait() {
            Ok(Some(status)) => {
                let state = match status.code() {
                    Some(0) => VideoState::Ended,
                    code => VideoState::Failed(VideoFailure::Exited(code)),
                };
                return Some(self.finish(state));
            }
            Ok(None) => {}
            Err(error) => return Some(self.fail(VideoFailure::Inspect(error.to_string()))),
        }
        if let IpcLink::Waiting { since } = playback.ipc {
            match MpvIpc::connect(&self.tools.ipc_path).await {
                Ok(ipc) => playback.ipc = IpcLink::Connected(ipc),
                Err(_) if since.elapsed() < IPC_CONNECT_DEADLINE => return None,
                Err(_) => return Some(self.fail(VideoFailure::IpcTimeout)),
            }
        }
        let IpcLink::Connected(ipc) = &mut playback.ipc else {
            unreachable!("the ipc link was connected just above");
        };
        let play_id = playback.play_id;
        match ipc.state().await {
            Ok(VideoState::Starting) => None,
            Ok(state) => Some(VideoUpdate {
                play_id,
                state,
                residue: None,
            }),
            Err(error) => Some(self.fail(VideoFailure::Ipc(error.to_string()))),
        }
    }

    /// Kill mpv without asking, for a pair socket that went away. The update
    /// cannot be sent; it carries the residue to clear and the play to log.
    pub(super) fn abandon(&mut self) -> Option<VideoUpdate> {
        let old = self.take_and_kill()?;
        Some(VideoUpdate {
            play_id: old.play_id,
            state: VideoState::Stopped,
            residue: Some(old.rect),
        })
    }

    fn current_for(&mut self, play_id: u64) -> Option<&mut Playback> {
        self.current
            .as_mut()
            .filter(|playback| playback.play_id == play_id)
    }

    fn fail(&mut self, failure: VideoFailure) -> VideoUpdate {
        self.finish(VideoState::Failed(failure))
    }

    fn finish(&mut self, state: VideoState) -> VideoUpdate {
        let old = self
            .take_and_kill()
            .expect("finish is only called with a playback running");
        VideoUpdate {
            play_id: old.play_id,
            state,
            residue: Some(old.rect),
        }
    }

    fn spawn(
        &self,
        play_id: u64,
        url: String,
        rect: VideoRect,
        volume_percent: u8,
        resume: Option<Resume>,
    ) -> Result<Playback, VideoFailure> {
        // The server is trusted, but the URL is the one string here that
        // ends up as a command argument, so the scheme gate stays.
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(VideoFailure::UnsupportedUrl);
        }
        let Some(tty) = TtySize::read().or(self.tools.startup_tty) else {
            return Err(VideoFailure::NoPixelSize);
        };
        match fs::remove_file(&self.tools.ipc_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(VideoFailure::Spawn(format!(
                    "removing stale ipc socket: {error}"
                )));
            }
        }
        let log = match open_state_log(&self.tools.log_path) {
            Ok(log) => log,
            Err(error) => return Err(VideoFailure::Spawn(format!("{error:#}"))),
        };
        let args = mpv_args(&MpvLaunch {
            url: &url,
            rect,
            tty,
            volume_percent,
            resume,
            ipc_path: &self.tools.ipc_path,
            ytdlp_override: self.tools.ytdlp_override.as_deref(),
        });
        // stdout stays the tty: that is where the kitty frames go.
        let child = Command::new(&self.tools.mpv)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::from(log))
            .spawn();
        match child {
            Ok(child) => Ok(Playback {
                play_id,
                url,
                rect,
                volume_percent,
                child,
                ipc: IpcLink::Waiting {
                    since: Instant::now(),
                },
            }),
            Err(error) => Err(VideoFailure::Spawn(error.to_string())),
        }
    }

    fn take_and_kill(&mut self) -> Option<Playback> {
        let mut playback = self.current.take()?;
        let _ = playback.child.kill();
        let _ = playback.child.wait();
        let _ = fs::remove_file(&self.tools.ipc_path);
        Some(playback)
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.take_and_kill();
    }
}

#[cfg(test)]
#[path = "video_test.rs"]
mod video_test;
