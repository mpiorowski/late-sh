//! Watch: plays a video the TUI asks for inside the user's own terminal,
//! through their `mpv` (kitty graphics output, shared-memory frames where
//! the terminal reads them) and the `yt-dlp` mpv's ytdl hook calls. The
//! server never touches media: the TUI reserves a black rectangle, sends
//! `video_play` with it, and keeps every key. This module owns the mpv
//! child, its IPC socket, and the relay that carries mpv's frames to the
//! tty with mpv's own terminal-mode writes filtered out; it does the work
//! and returns data, and the pair loop in `ws.rs` sends, logs, and
//! silences the house audio around it.

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env,
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    thread::JoinHandle,
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
const SEEK_FAR_STEP_S: i64 = 60;
/// HLS variants first: YouTube serves an open-ended range request (the
/// only kind ffmpeg makes) at playback speed, so a direct stream never
/// caches ahead and every seek stalls for seconds, while HLS segments are
/// bounded requests that arrive at full speed. Direct streams stay as the
/// fallback for a video without HLS.
const YTDL_FORMAT: &str = "bestvideo[protocol^=m3u8][height<=720]+bestaudio[protocol^=m3u8]/bestvideo[height<=720]+bestaudio/best[height<=720]";
/// A relay sequence held past this many bytes is written as it is: mpv's
/// graphics commands are chunked at 4 KB, so nothing legitimate gets here.
const RELAY_HOLD_CAP: usize = 8 * 1024 * 1024;

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

/// The Kitty-graphics terminals the CLI recognizes from its env, the same
/// hints it forwards for images. The server's own detection is the second
/// opinion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KittyTerminal {
    Kitty,
    Ghostty,
    WezTerm,
    Konsole,
    Rio,
}

impl KittyTerminal {
    /// Whether the terminal reads kitty's shared-memory transfer (`t=s`).
    /// mpv does not fall back when it does not: the frames go nowhere and
    /// the video is a black box that reports `playing`. Only terminals
    /// verified to implement it get `--vo-kitty-use-shm=yes`.
    pub(super) fn supports_shm(self) -> bool {
        match self {
            Self::Kitty | Self::Ghostty => true,
            Self::WezTerm | Self::Konsole | Self::Rio => false,
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
    /// The top-left cell, 1-based. mpv moves the cursor there and places
    /// every frame at the cursor with no centering, so this cell is under
    /// the image whatever its aspect.
    fn origin_cell(self) -> (u32, u32) {
        (u32::from(self.left) + 1, u32::from(self.top) + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum VideoControl {
    TogglePause,
    SeekBack,
    SeekForward,
    /// Shift and the arrows: a minute.
    SeekBackFar,
    SeekForwardFar,
    /// Back to the top.
    Restart,
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
    /// mpv exited before the video ended; `code` is `None` when a signal
    /// ended it. `reason` is the last error line of mpv's log file, the
    /// only place mpv says why with `--terminal=no`.
    Exited {
        code: Option<i32>,
        reason: Option<String>,
    },
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
            Self::Exited {
                code: Some(code),
                reason: Some(reason),
            } => format!("mpv exited with status {code}: {reason}"),
            Self::Exited {
                code: Some(code),
                reason: None,
            } => format!("mpv exited with status {code}"),
            Self::Exited {
                code: None,
                reason: Some(reason),
            } => format!("mpv was killed by a signal: {reason}"),
            Self::Exited {
                code: None,
                reason: None,
            } => "mpv was killed by a signal".to_string(),
        }
    }
}

/// The last error or fatal line of an mpv `--log-file`, without its
/// timestamp: `[e][stream] Failed to open ...`. mpv truncates the file on
/// every run, so the text is this play's alone.
pub(super) fn mpv_log_reason(log: &str) -> Option<String> {
    log.lines().rev().find_map(|line| {
        let level_start = line.find("][")?;
        let rest = &line[level_start + 1..];
        match rest.starts_with("[e]") || rest.starts_with("[f]") {
            true => Some(rest.trim_end().to_string()),
            false => None,
        }
    })
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
    /// Shared-memory frames, when the terminal reads them.
    use_shm: bool,
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
            use_shm: use_shared_memory(&vars),
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
    if kitty_terminal(vars).is_none() {
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

/// Shared-memory frames: the terminal table, unless `LATE_VIDEO_SHM` is
/// exactly `yes` or `no`. The override exists to try the other path on a
/// terminal that tears or one the table is wrong about.
pub(super) fn use_shared_memory(vars: &HashMap<String, String>) -> bool {
    match env_value(vars, "LATE_VIDEO_SHM") {
        Some("yes") => true,
        Some("no") => false,
        Some(_) | None => kitty_terminal(vars).is_some_and(KittyTerminal::supports_shm),
    }
}

/// Which Kitty-graphics terminal the env describes, if any.
pub(super) fn kitty_terminal(vars: &HashMap<String, String>) -> Option<KittyTerminal> {
    if env_value(vars, "TERM") == Some("xterm-kitty")
        || env_value(vars, "KITTY_WINDOW_ID").is_some()
    {
        return Some(KittyTerminal::Kitty);
    }
    match env_value(vars, "TERM_PROGRAM") {
        Some("ghostty") => return Some(KittyTerminal::Ghostty),
        Some("WezTerm") => return Some(KittyTerminal::WezTerm),
        Some("rio") => return Some(KittyTerminal::Rio),
        Some(_) | None => {}
    }
    if env_value(vars, "GHOSTTY_RESOURCES_DIR").is_some() {
        return Some(KittyTerminal::Ghostty);
    }
    if env_value(vars, "WEZTERM_EXECUTABLE").is_some() {
        return Some(KittyTerminal::WezTerm);
    }
    if env_value(vars, "KONSOLE_VERSION").is_some() {
        return Some(KittyTerminal::Konsole);
    }
    None
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
pub(super) struct MpvLaunch<'a> {
    pub(super) url: &'a str,
    pub(super) rect: VideoRect,
    pub(super) tty: TtySize,
    pub(super) use_shm: bool,
    pub(super) ipc_path: &'a Path,
    pub(super) ytdlp_override: Option<&'a Path>,
    /// mpv's own log: with `--terminal=no` nothing reaches stderr, so this
    /// file is where a failed play says why.
    pub(super) log_path: &'a Path,
    /// Where to begin, in seconds; zero is the top and adds no argument.
    pub(super) start_s: u64,
}

/// The whole mpv command line. The six geometry numbers are the contract
/// with the TUI: mpv's `--vo-kitty-left/top` are 1-based where the TUI's
/// cells are 0-based, and the pixel size comes from this tty's cell size.
///
/// `--vo-kitty-alt-screen=no` is mandatory: `yes` switches to the alternate
/// screen and wipes the TUI on exit. `--vo-kitty-config-clear=no` keeps mpv
/// from clearing on every reconfigure. Neither stops mpv's own writes of
/// cursor, mouse and delete-all-images commands around the frames; the
/// relay (`TtyFilter`) strips those, and `settle_sequence` removes the
/// frames it leaves behind.
pub(super) fn mpv_args(launch: &MpvLaunch<'_>) -> Vec<OsString> {
    let MpvLaunch {
        url,
        rect,
        tty,
        use_shm,
        ipc_path,
        ytdlp_override,
        log_path,
        start_s,
    } = launch;
    let (left, top) = rect.origin_cell();
    let (width, height) = tty.pixels_for(*rect);
    let shm = match use_shm {
        true => "yes",
        false => "no",
    };
    let mut args: Vec<OsString> = vec![
        "--no-config".into(),
        "--really-quiet".into(),
        "--terminal=no".into(),
        path_arg("--log-file=", log_path),
        "--vo=kitty".into(),
        format!("--vo-kitty-use-shm={shm}").into(),
        "--vo-kitty-alt-screen=no".into(),
        "--vo-kitty-config-clear=no".into(),
        format!("--vo-kitty-left={left}").into(),
        format!("--vo-kitty-top={top}").into(),
        format!("--vo-kitty-cols={}", rect.cols).into(),
        format!("--vo-kitty-rows={}", rect.rows).into(),
        format!("--vo-kitty-width={width}").into(),
        format!("--vo-kitty-height={height}").into(),
        "--ytdl=yes".into(),
        format!("--ytdl-format={YTDL_FORMAT}").into(),
    ];
    if *start_s > 0 {
        args.push(format!("--start={start_s}").into());
    }
    if let Some(ytdlp) = ytdlp_override {
        args.push(path_arg("--script-opts=ytdl_hook-ytdl_path=", ytdlp));
    }
    // Full level, as any video player opens: the house volume is for the
    // radio, which is silenced while a video plays, and a video at 30
    // percent of itself is just quiet. `m` and the system mixer remain.
    args.extend([
        "--volume=100".into(),
        "--keep-open=no".into(),
        path_arg("--input-ipc-server=", ipc_path),
        "--".into(),
        (*url).into(),
    ]);
    args
}

/// `--flag=<path>`, the path kept as the OS gave it.
fn path_arg(flag: &str, path: &Path) -> OsString {
    let mut arg = OsString::from(flag);
    arg.push(path);
    arg
}

/// The mpv IPC message for one control, before its `request_id`.
pub(super) fn ipc_command(control: VideoControl) -> Value {
    match control {
        VideoControl::TogglePause => json!({ "command": ["cycle", "pause"] }),
        VideoControl::SeekBack => json!({ "command": ["seek", -SEEK_STEP_S] }),
        VideoControl::SeekForward => json!({ "command": ["seek", SEEK_STEP_S] }),
        VideoControl::SeekBackFar => json!({ "command": ["seek", -SEEK_FAR_STEP_S] }),
        VideoControl::SeekForwardFar => json!({ "command": ["seek", SEEK_FAR_STEP_S] }),
        VideoControl::Restart => json!({ "command": ["seek", 0, "absolute"] }),
        VideoControl::ToggleMute => json!({ "command": ["cycle", "mute"] }),
    }
}

/// The socket commands that move mpv's output to `rect`: the six geometry
/// options, then the output torn down and rebuilt so mpv reads them (it
/// reads them only when it configures the output, and setting `vo` to its
/// current value is a no-op, so the swap goes through `null`). The stream
/// and its position carry on underneath.
pub(super) fn place_commands(rect: VideoRect, tty: TtySize) -> Vec<Value> {
    let (left, top) = rect.origin_cell();
    let (width, height) = tty.pixels_for(rect);
    let set = |name: &str, value: Value| json!({ "command": ["set_property", name, value] });
    vec![
        set("vo-kitty-left", json!(left)),
        set("vo-kitty-top", json!(top)),
        set("vo-kitty-cols", json!(rect.cols)),
        set("vo-kitty-rows", json!(rect.rows)),
        set("vo-kitty-width", json!(width)),
        set("vo-kitty-height", json!(height)),
        set("vo", json!("null")),
        set("vo", json!("kitty")),
    ]
}

/// What a `video_place` did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PlaceOutcome {
    /// mpv's output is in the new rectangle; the old frame is settled.
    Moved,
    /// mpv's socket is not up yet: applied once it is, from `poll`.
    Deferred,
    /// No such play, or the rectangle it already has.
    Unchanged,
    /// The socket broke: mpv is gone and this says so.
    Failed(VideoUpdate),
}

/// The `video_state` pair event for one update.
pub(super) fn video_state_payload(play_id: u64, state: &VideoState) -> Value {
    let mut payload = match state {
        VideoState::Starting => json!({ "state": "starting" }),
        VideoState::Playing {
            position_s,
            duration_s,
        } => json!({ "state": "playing", "position_s": position_s, "duration_s": duration_s }),
        VideoState::Paused {
            position_s,
            duration_s,
        } => json!({ "state": "paused", "position_s": position_s, "duration_s": duration_s }),
        VideoState::Ended => json!({ "state": "ended" }),
        VideoState::Failed(failure) => json!({ "state": "failed", "message": failure.message() }),
        VideoState::Stopped => json!({ "state": "stopped" }),
    };
    payload["event"] = json!("video_state");
    payload["play_id"] = json!(play_id);
    payload
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

/// The kitty command that removes every frame mpv placed in `rect`: a
/// delete of the placements under the rectangle's top-left cell, where mpv
/// anchors each frame. `P` frees the image data as well, so a long video
/// does not leave its frames in the terminal's image store; `q=2` keeps
/// the terminal from answering on stdin, which the CLI would forward to
/// the server as keys. The TUI's own images elsewhere are untouched.
pub(super) fn settle_sequence(rect: VideoRect) -> String {
    let (x, y) = rect.origin_cell();
    format!("\x1b_Ga=d,d=P,x={x},y={y},q=2\x1b\\")
}

/// Write [`settle_sequence`] to the tty, under the same stdout lock the SSH
/// forwarder and the relay write under.
pub(super) fn settle_terminal(rect: VideoRect) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(settle_sequence(rect).as_bytes())?;
    out.flush()
}

/// Carries mpv's stdout to the tty with mpv's terminal-mode writes removed
/// and every escape sequence delivered whole.
///
/// mpv's kitty output writes, besides the frames, a hide-cursor and an
/// any-motion mouse enable on start, a show-cursor and mouse disable on
/// exit, and a delete-all-images command at both ends. Each of those would
/// change state the TUI owns (the mouse mode it chose, its hidden cursor,
/// its own images), so they are dropped here instead of restored later.
/// Everything else (cursor moves, the graphics commands that carry frames)
/// passes through, but only once a sequence is complete: a graphics
/// command split across two reads would otherwise let the SSH forwarder's
/// bytes land inside its payload.
#[derive(Default)]
pub(super) struct TtyFilter {
    /// The start of an escape sequence the input has not finished yet.
    held: Vec<u8>,
}

/// DEC private modes mpv's kitty output sets and resets; the TUI owns all
/// three (cursor visibility, any-motion mouse tracking, the alternate
/// screen).
const STRIPPED_PRIVATE_MODES: [&[u8]; 3] = [b"25", b"1003", b"1049"];

impl TtyFilter {
    pub(super) fn feed(&mut self, input: &[u8]) -> Vec<u8> {
        let mut bytes = std::mem::take(&mut self.held);
        bytes.extend_from_slice(input);
        let mut out = Vec::with_capacity(bytes.len());
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != 0x1b {
                out.push(bytes[at]);
                at += 1;
                continue;
            }
            match sequence_end(&bytes[at..]) {
                SequenceEnd::Complete(len) => {
                    let sequence = &bytes[at..at + len];
                    if !is_mpv_mode_write(sequence) {
                        out.extend_from_slice(sequence);
                    }
                    at += len;
                }
                SequenceEnd::Incomplete if bytes.len() - at > RELAY_HOLD_CAP => {
                    out.extend_from_slice(&bytes[at..]);
                    at = bytes.len();
                }
                SequenceEnd::Incomplete => {
                    self.held = bytes[at..].to_vec();
                    at = bytes.len();
                }
                SequenceEnd::NotASequence => {
                    out.push(bytes[at]);
                    at += 1;
                }
            }
        }
        out
    }
}

enum SequenceEnd {
    /// The sequence's length in bytes.
    Complete(usize),
    /// More input is needed to find its end.
    Incomplete,
    /// A lone ESC or one followed by something that is neither CSI nor APC.
    NotASequence,
}

/// Where the CSI (`ESC [` up to a final byte) or APC (`ESC _` up to
/// `ESC \`) sequence at the start of `bytes` ends.
fn sequence_end(bytes: &[u8]) -> SequenceEnd {
    match bytes.get(1) {
        None => SequenceEnd::Incomplete,
        Some(b'[') => match bytes[2..].iter().position(|b| (0x40..=0x7e).contains(b)) {
            Some(offset) => SequenceEnd::Complete(2 + offset + 1),
            None => SequenceEnd::Incomplete,
        },
        Some(b'_') => match bytes.windows(2).skip(2).position(|pair| pair == b"\x1b\\") {
            Some(offset) => SequenceEnd::Complete(2 + offset + 2),
            None => SequenceEnd::Incomplete,
        },
        Some(_) => SequenceEnd::NotASequence,
    }
}

fn is_mpv_mode_write(sequence: &[u8]) -> bool {
    if sequence == b"\x1b_Ga=d;\x1b\\" {
        return true;
    }
    let Some(mode) = sequence.strip_prefix(b"\x1b[?") else {
        return false;
    };
    let Some(mode) = mode.strip_suffix(b"h").or_else(|| mode.strip_suffix(b"l")) else {
        return false;
    };
    STRIPPED_PRIVATE_MODES.contains(&mode)
}

/// Read mpv's stdout until it closes, writing each read's complete
/// sequences to the tty under the stdout lock.
fn spawn_relay(mut from_mpv: ChildStdout) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut filter = TtyFilter::default();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let n = match from_mpv.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            let out = filter.feed(&buf[..n]);
            if out.is_empty() {
                continue;
            }
            let mut tty = io::stdout().lock();
            if tty.write_all(&out).and_then(|()| tty.flush()).is_err() {
                break;
            }
        }
    })
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
    request_id: Option<u64>,
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
}

enum IpcLink {
    Waiting { since: Instant },
    Connected(MpvIpc),
}

struct Playback {
    play_id: u64,
    rect: VideoRect,
    /// The tty read at spawn: it sized the first rectangle's pixels and
    /// sizes every move's.
    tty: TtySize,
    child: Child,
    /// Ends when mpv's stdout closes; joined after the kill so the settle
    /// delete follows the last frame. `None` once joined.
    relay: Option<JoinHandle<()>>,
    ipc: IpcLink,
    /// A `video_place` that arrived before the socket was up: the
    /// rectangle whose frame to settle once the move goes through.
    pending_place: Option<VideoRect>,
}

impl Playback {
    /// Settle the frame at `old`, then send the move to `self.rect` over
    /// the socket. Only called once the socket is up.
    async fn move_output(&mut self, old: VideoRect) -> Result<(), VideoFailure> {
        if let Err(error) = settle_terminal(old) {
            return Err(VideoFailure::Inspect(format!(
                "settling the old frame: {error}"
            )));
        }
        let IpcLink::Connected(ipc) = &mut self.ipc else {
            unreachable!("a move is only sent over a connected ipc link");
        };
        for command in place_commands(self.rect, self.tty) {
            match ipc.request(command).await {
                Ok(IpcReply::Success(_) | IpcReply::Error) => {}
                Err(error) => return Err(VideoFailure::Ipc(error.to_string())),
            }
        }
        Ok(())
    }
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

    /// Start `url` in `rect` from `start_s`, stopping whatever was playing
    /// first. The old play gets no report: the server superseded it.
    pub(super) fn play(
        &mut self,
        play_id: u64,
        url: String,
        rect: VideoRect,
        start_s: u64,
    ) -> VideoUpdate {
        let residue = self.take_and_kill().map(|old| old.rect);
        let state = match self.spawn(play_id, url, rect, start_s) {
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

    /// Move a play's output to `rect` in place: the old frame is settled
    /// first (the delete targets its origin cell, which the new frame may
    /// cover, so it must go before the rebuild), then the geometry and the
    /// output swap go over the socket. Before the socket is up the move
    /// waits for `poll`.
    pub(super) async fn place(&mut self, play_id: u64, rect: VideoRect) -> PlaceOutcome {
        let Some(playback) = self.current_for(play_id) else {
            return PlaceOutcome::Unchanged;
        };
        if playback.rect == rect {
            return PlaceOutcome::Unchanged;
        }
        let old = playback.rect;
        playback.rect = rect;
        match &mut playback.ipc {
            IpcLink::Waiting { .. } => {
                playback.pending_place = Some(playback.pending_place.unwrap_or(old));
                PlaceOutcome::Deferred
            }
            IpcLink::Connected(_) => match playback.move_output(old).await {
                Ok(()) => PlaceOutcome::Moved,
                Err(failure) => PlaceOutcome::Failed(self.fail(failure)),
            },
        }
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
                    code => VideoState::Failed(VideoFailure::Exited {
                        code,
                        reason: fs::read_to_string(&self.tools.log_path)
                            .ok()
                            .and_then(|log| mpv_log_reason(&log)),
                    }),
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
        if let Some(old) = playback.pending_place.take()
            && let Err(failure) = playback.move_output(old).await
        {
            return Some(self.fail(failure));
        }
        let play_id = playback.play_id;
        let IpcLink::Connected(ipc) = &mut playback.ipc else {
            unreachable!("the ipc link was connected just above");
        };
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
        start_s: u64,
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
            use_shm: self.tools.use_shm,
            ipc_path: &self.tools.ipc_path,
            ytdlp_override: self.tools.ytdlp_override.as_deref(),
            log_path: &self.tools.log_path,
            start_s,
        });
        // mpv never writes to the tty itself: its stdout is a pipe the relay
        // carries to the tty with mpv's terminal-mode writes filtered out.
        let child = Command::new(&self.tools.mpv)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => return Err(VideoFailure::Spawn(error.to_string())),
        };
        let Some(from_mpv) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VideoFailure::Spawn("mpv has no stdout pipe".to_string()));
        };
        Ok(Playback {
            play_id,
            rect,
            tty,
            child,
            relay: Some(spawn_relay(from_mpv)),
            ipc: IpcLink::Waiting {
                since: Instant::now(),
            },
            pending_place: None,
        })
    }

    fn take_and_kill(&mut self) -> Option<Playback> {
        let mut playback = self.current.take()?;
        let _ = playback.child.kill();
        let _ = playback.child.wait();
        // The pipe closed with the child, so the relay is at its end: wait
        // for it so the settle delete lands after mpv's last frame.
        if let Some(relay) = playback.relay.take() {
            let _ = relay.join();
        }
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
