use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::{
    clipboard,
    mpris::{DesktopCommand, DesktopMedia, IcecastTrack, MediaSource, RadioTrack, YoutubeTrack},
    voice::VoiceRuntimeState,
};

pub(super) struct PairClientInfo {
    pub(super) ssh_mode: &'static str,
    pub(super) platform: &'static str,
}

pub(super) struct PlaybackState<'a> {
    pub(super) played_samples: &'a AtomicU64,
    pub(super) sample_rate: u32,
    pub(super) muted: &'a AtomicBool,
    pub(super) volume_percent: &'a AtomicU8,
    pub(super) source_is_icecast: &'a AtomicBool,
    pub(super) native_source_selected: &'a AtomicBool,
    pub(super) stream_url: &'a Arc<Mutex<String>>,
    pub(super) stream_generation: &'a AtomicU64,
    pub(super) stream_flushed_generation: &'a AtomicU64,
    pub(super) icecast_stream_url: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    /// Absolute mute/volume fan-out. The server relays a paired client's own
    /// `set_muted`/`set_volume` event (this CLI's MPRIS surface) to everyone
    /// on the token, so the command that started at a desktop widget comes
    /// back here as the state to apply.
    SetMuted {
        muted: bool,
    },
    SetVolume {
        volume_percent: u8,
    },
    RequestClipboardImage {
        /// Echoed back in the clipboard payload so the server can match the
        /// response to this exact request. None from older servers.
        #[serde(default)]
        request_id: Option<u64>,
    },
    SetPlaybackSource {
        source: PairAudioSource,
        #[serde(default)]
        stream_url: Option<String>,
        #[serde(default)]
        station: Option<String>,
    },
    QueueUpdate {
        #[serde(default)]
        current: Option<YoutubeTrack>,
    },
    NowPlayingUpdate {
        #[serde(default)]
        mounts: HashMap<String, IcecastTrack>,
    },
    RadioMetaUpdate {
        #[serde(default)]
        stations: HashMap<String, RadioTrack>,
    },
    VoiceJoin {
        room: String,
        url: String,
        token: String,
        muted: bool,
        deafened: bool,
    },
    VoiceLeave,
    VoiceSetMuted {
        muted: bool,
    },
    VoiceSetDeafened {
        deafened: bool,
    },
    /// Open a URL in the user's default browser (stream watch/go-live
    /// pages). Advertised via the `open_url` capability.
    OpenUrl {
        url: String,
    },
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PairAudioSource {
    Icecast,
    Youtube,
    Radio,
}

impl From<PairAudioSource> for MediaSource {
    fn from(source: PairAudioSource) -> Self {
        match source {
            PairAudioSource::Icecast => Self::Icecast,
            PairAudioSource::Youtube => Self::Youtube,
            PairAudioSource::Radio => Self::Radio,
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const CLIENT_CAPABILITIES: &[&str] = &["clipboard_image", "youtube", "voice", "open_url"];

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const CLIENT_CAPABILITIES: &[&str] = &[];

const WEBVIEW_CRASH_WINDOW: Duration = Duration::from_secs(60);
const WEBVIEW_CRASH_LIMIT: u8 = 3;
const WEBVIEW_CRASH_BACKOFF: Duration = Duration::from_secs(5 * 60);
pub(super) struct WebviewPlaybackController {
    api_base_url: String,
    token: String,
    child: Option<Child>,
    wants_youtube: bool,
    helper_log_path: Option<PathBuf>,
    crash_window_started: Option<Instant>,
    crash_count: u8,
    disabled_until: Option<Instant>,
}

impl WebviewPlaybackController {
    pub(super) fn new(api_base_url: String, token: String) -> Self {
        Self {
            api_base_url,
            token,
            child: None,
            wants_youtube: false,
            helper_log_path: None,
            crash_window_started: None,
            crash_count: 0,
            disabled_until: None,
        }
    }

    fn apply_playback_source(
        &mut self,
        source: PairAudioSource,
        muted: bool,
        volume_percent: u8,
    ) -> Result<()> {
        match source {
            PairAudioSource::Youtube => self.enter_youtube(muted, volume_percent),
            PairAudioSource::Icecast => self.enter_icecast(),
            PairAudioSource::Radio => self.enter_radio(),
        }
    }

    /// Heartbeat-tick watchdog: respawn the helper when the user is on
    /// YouTube and the child died. Without this the parent only notices a
    /// dead helper on the next SetPlaybackSource, which may never arrive
    /// (the server can miss the helper's disconnect entirely on a half-open
    /// TCP drop and then never replays the playback source).
    pub(super) fn maintain_helper(&mut self, muted: bool, volume_percent: u8) {
        if !self.wants_youtube || self.helper_is_running() {
            return;
        }
        // Quiet backoff pre-check: enter_youtube's backoff probe warns on
        // every call, which is too loud at a 1s cadence.
        if let Some(until) = self.disabled_until
            && Instant::now() < until
        {
            return;
        }
        if let Err(err) = self.enter_youtube(muted, volume_percent) {
            warn!(error = %err, "failed to respawn embedded YouTube webview helper");
        }
    }

    fn enter_youtube(&mut self, muted: bool, volume_percent: u8) -> Result<()> {
        self.wants_youtube = true;
        if self.helper_is_running() {
            return Ok(());
        }
        if self.helper_backoff_active() {
            return Ok(());
        }

        let helper_stderr = match webview_helper_stderr() {
            Ok(stderr) => stderr,
            Err(err) => {
                warn!(error = %err, "failed to open embedded YouTube webview helper log");
                self.record_helper_start_failure();
                return Ok(());
            }
        };
        match &helper_stderr.destination {
            WebviewHelperStderrDestination::Inherit => {
                self.helper_log_path = None;
                info!("embedded YouTube webview helper stderr inherited from parent process");
            }
            WebviewHelperStderrDestination::LogFile(path) => {
                self.helper_log_path = Some(path.clone());
                info!(
                    path = %path.display(),
                    "embedded YouTube webview helper stderr redirected to log file"
                );
            }
        }
        // Windows/macOS run the webview in-process via the `webview-pair`
        // subcommand. Everywhere else it is the standalone `late-webview`
        // binary, so `late` itself never links WebKitGTK/GTK — a missing
        // webview stack must degrade to "no embedded YouTube", never to
        // "late does not start".
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let mut command = {
            let exe = match std::env::current_exe() {
                Ok(exe) => exe,
                Err(err) => {
                    warn!(error = %err, "failed to locate current late executable for webview helper");
                    self.record_helper_start_failure();
                    return Ok(());
                }
            };
            let mut command = Command::new(exe);
            command.arg("webview-pair");
            command
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let mut command = Command::new(webview_helper_program());
        command
            .env("LATE_API_BASE_URL", &self.api_base_url)
            // Hand the session's current mute/volume to the helper so a
            // respawn mid-session comes back muted if the user muted, instead
            // of the helper's unmuted default. The parent tracks the same
            // toggle_mute/volume controls the helper receives, so these
            // atomics mirror the helper's last state.
            .env("LATE_WEBVIEW_INITIAL_MUTED", if muted { "1" } else { "0" })
            .env("LATE_WEBVIEW_INITIAL_VOLUME", volume_percent.to_string())
            // The helper is an undecorated media surface, not an accessibility
            // target. Opting out avoids host AT-SPI bridge crashes from stale
            // at-spi-bus-launcher/dbus state while leaving the terminal app's
            // own environment untouched.
            .env("NO_AT_BRIDGE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(helper_stderr.stdio);
        #[cfg(target_os = "linux")]
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            command.env("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        #[cfg(unix)]
        {
            // Keep WebKitGTK media subprocesses in the helper's process group
            // so switching away from YouTube can terminate the whole helper
            // tree on Linux setups where playback outlives the direct child.
            command.process_group(0);
        }

        let child = match command.spawn() {
            Ok(child) => child,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                warn!(
                    error = %err,
                    "late-webview helper binary not found; embedded YouTube playback is \
                     unavailable (reinstall the CLI or set LATE_WEBVIEW_BIN); radio and \
                     icecast still work, and the queue is listenable at late.sh/listen"
                );
                self.record_helper_start_failure();
                return Ok(());
            }
            Err(err) => {
                warn!(error = %err, "failed to spawn embedded YouTube webview helper");
                self.record_helper_start_failure();
                return Ok(());
            }
        };
        let mut child = child;
        if let Err(err) = write_helper_token(&mut child, &self.token) {
            warn!(error = %err, "failed to pass token to embedded YouTube webview helper");
            let _ = child.kill();
            let _ = child.wait();
            self.record_helper_start_failure();
            return Ok(());
        }
        self.child = Some(child);
        info!("started embedded YouTube webview helper");
        Ok(())
    }

    fn enter_icecast(&mut self) -> Result<()> {
        if !self.wants_youtube && self.child.is_none() {
            return Ok(());
        }
        self.wants_youtube = false;
        self.stop_helper();
        info!("resumed native Icecast playback");
        Ok(())
    }

    fn enter_radio(&mut self) -> Result<()> {
        if !self.wants_youtube && self.child.is_none() {
            return Ok(());
        }
        self.wants_youtube = false;
        self.stop_helper();
        info!("using native direct radio playback");
        Ok(())
    }

    fn helper_is_running(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                warn!(
                    ?status,
                    signal = ?exit_signal(&status),
                    signal_name = exit_signal_name(&status),
                    log_path = ?self.helper_log_path.as_deref(),
                    "embedded YouTube webview helper exited"
                );
                self.child = None;
                self.record_helper_exit();
                false
            }
            Ok(None) => true,
            Err(err) => {
                warn!(error = %err, "failed to inspect embedded YouTube webview helper");
                self.child = None;
                self.record_helper_start_failure();
                false
            }
        }
    }

    fn helper_backoff_active(&mut self) -> bool {
        let Some(until) = self.disabled_until else {
            return false;
        };
        let now = Instant::now();
        if now < until {
            let retry_in = until.saturating_duration_since(now).as_secs();
            warn!(
                retry_in_secs = retry_in,
                log_path = ?self.helper_log_path.as_deref(),
                "embedded YouTube webview helper is temporarily disabled after repeated startup failures"
            );
            return true;
        }
        self.disabled_until = None;
        self.crash_window_started = None;
        self.crash_count = 0;
        false
    }

    fn record_helper_start_failure(&mut self) {
        self.record_helper_failure("embedded YouTube webview helper failed to start repeatedly");
    }

    fn record_helper_exit(&mut self) {
        self.record_helper_failure("embedded YouTube webview helper crashed repeatedly");
    }

    fn record_helper_failure(&mut self, message: &'static str) {
        let now = Instant::now();
        match self.crash_window_started {
            Some(started) if now.duration_since(started) <= WEBVIEW_CRASH_WINDOW => {
                self.crash_count = self.crash_count.saturating_add(1);
            }
            _ => {
                self.crash_window_started = Some(now);
                self.crash_count = 1;
            }
        }

        if self.crash_count >= WEBVIEW_CRASH_LIMIT {
            self.disabled_until = Some(now + WEBVIEW_CRASH_BACKOFF);
            // Nothing takes over when the helper is disabled. Browser pairing
            // used to hand YouTube off automatically; now the user has to go
            // listen elsewhere, so the message has to say so rather than imply
            // a fallback kicked in.
            warn!(
                crash_count = self.crash_count,
                backoff_secs = WEBVIEW_CRASH_BACKOFF.as_secs(),
                log_path = ?self.helper_log_path.as_deref(),
                "{message}; pausing embedded YouTube playback, listen at late.sh/listen meanwhile"
            );
        }
    }

    fn stop_helper(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Err(err) = kill_webview_helper(&mut child) {
            warn!(error = %err, "failed to stop embedded YouTube webview helper");
            return;
        }
        let _ = child.wait();
        info!("stopped embedded YouTube webview helper");
    }
}

/// Resolve the standalone `late-webview` helper binary on platforms where
/// the webview is not compiled into `late`: explicit `LATE_WEBVIEW_BIN`
/// override, then a sibling of the current executable (the installer places
/// both binaries in the same directory), then a bare name for `$PATH`.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(super) fn webview_helper_program() -> PathBuf {
    if let Some(path) = nonempty_os_env("LATE_WEBVIEW_BIN") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("late-webview");
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from("late-webview")
}

#[cfg(unix)]
fn kill_webview_helper(child: &mut Child) -> Result<()> {
    let pid = child.id() as i32;
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(-pid),
        nix::sys::signal::Signal::SIGKILL,
    )
    .with_context(|| format!("failed to kill webview helper process group {pid}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn kill_webview_helper(child: &mut Child) -> Result<()> {
    child.kill().context("failed to kill webview helper")
}

struct WebviewHelperStderr {
    stdio: Stdio,
    destination: WebviewHelperStderrDestination,
}

enum WebviewHelperStderrDestination {
    Inherit,
    LogFile(PathBuf),
}

fn webview_helper_stderr() -> Result<WebviewHelperStderr> {
    if env_flag("LATE_WEBVIEW_DEBUG_STDERR") {
        return Ok(WebviewHelperStderr {
            stdio: Stdio::inherit(),
            destination: WebviewHelperStderrDestination::Inherit,
        });
    }

    let path = webview_helper_log_path();
    ensure_webview_log_dir(&path)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let file = options
        .open(&path)
        .with_context(|| format!("failed to open webview helper log at {}", path.display()))?;
    #[cfg(unix)]
    {
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    Ok(WebviewHelperStderr {
        stdio: Stdio::from(file),
        destination: WebviewHelperStderrDestination::LogFile(path),
    })
}

fn write_helper_token(child: &mut Child, token: &str) -> Result<()> {
    let mut stdin = child
        .stdin
        .take()
        .context("webview helper stdin pipe was not available")?;
    stdin
        .write_all(token.as_bytes())
        .context("failed to write webview helper token")?;
    stdin
        .write_all(b"\n")
        .context("failed to terminate webview helper token")?;
    Ok(())
}

fn webview_helper_log_path() -> PathBuf {
    if let Some(path) = nonempty_os_env("LATE_WEBVIEW_LOG") {
        return PathBuf::from(path);
    }

    #[cfg(unix)]
    {
        if let Some(base) = nonempty_os_env("XDG_STATE_HOME") {
            return PathBuf::from(base).join("late").join("webview.log");
        }
        if let Some(home) = nonempty_os_env("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("late")
                .join("webview.log");
        }
        if let Some(base) = nonempty_os_env("XDG_RUNTIME_DIR") {
            return PathBuf::from(base).join("late").join("webview.log");
        }
        env::temp_dir()
            .join(format!("late-{}", effective_user_id()))
            .join("webview.log")
    }

    #[cfg(windows)]
    {
        if let Some(base) = nonempty_os_env("LOCALAPPDATA") {
            return PathBuf::from(base).join("late").join("webview.log");
        }
        if let Some(profile) = nonempty_os_env("USERPROFILE") {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Local")
                .join("late")
                .join("webview.log");
        }
        return env::temp_dir().join("late").join("webview.log");
    }

    #[cfg(not(any(unix, windows)))]
    {
        env::temp_dir().join("late").join("webview.log")
    }
}

fn nonempty_os_env(key: &str) -> Option<std::ffi::OsString> {
    env::var_os(key).filter(|value| !value.is_empty())
}

fn env_flag(key: &str) -> bool {
    let Some(value) = env::var_os(key) else {
        return false;
    };
    let value = value.to_string_lossy();
    let normalized = value.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "" | "0" | "false" | "no" | "off")
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    status.signal()
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

#[cfg(unix)]
fn exit_signal_name(status: &std::process::ExitStatus) -> Option<&'static str> {
    match status.signal()? {
        6 => Some("SIGABRT"),
        9 => Some("SIGKILL"),
        11 => Some("SIGSEGV"),
        15 => Some("SIGTERM"),
        _ => None,
    }
}

#[cfg(not(unix))]
fn exit_signal_name(_status: &std::process::ExitStatus) -> Option<&'static str> {
    None
}

fn ensure_webview_log_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("webview helper log path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create webview helper log directory at {}",
            parent.display()
        )
    })?;
    #[cfg(unix)]
    {
        let metadata = fs::symlink_metadata(parent).with_context(|| {
            format!(
                "failed to inspect webview helper log directory at {}",
                parent.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            anyhow::bail!(
                "webview helper log directory is not a real directory: {}",
                parent.display()
            );
        }
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

#[cfg(unix)]
fn effective_user_id() -> u32 {
    // SAFETY: geteuid has no preconditions and does not modify memory.
    unsafe { nix::libc::geteuid() }
}

impl Drop for WebviewPlaybackController {
    fn drop(&mut self) {
        self.stop_helper();
    }
}

/// Mutable client-side runtime driven by the pair websocket loop: the webview
/// helper, voice state, and the desktop media surface with its command feed.
pub(super) struct PairRuntime<'a> {
    pub(super) webview: &'a mut WebviewPlaybackController,
    pub(super) voice: &'a mut VoiceRuntimeState,
    pub(super) desktop_media: &'a mut DesktopMedia,
    pub(super) desktop_commands: &'a mut tokio::sync::mpsc::Receiver<DesktopCommand>,
}

/// How long a pair connection has to hold before the retry loop treats it as
/// a healthy session rather than one more failure.
pub(super) const STABLE_CONNECTION: Duration = Duration::from_secs(60);
/// Delay between reconnects while the retry budget lasts.
pub(super) const PAIR_RECONNECT_DELAY: Duration = Duration::from_secs(2);
/// Delay between reconnects once the budget is spent. Pairing is never
/// abandoned: a server that comes back still gets this session back.
pub(super) const PAIR_SLOW_RECONNECT_DELAY: Duration = Duration::from_secs(60);
pub(super) const MAX_CONSECUTIVE_FAILURES: u32 = 10;

/// How one pair-websocket attempt ended, from the retry loop's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairAttempt {
    /// The socket never came up, or the server accepted it and dropped it
    /// without ever registering this client. This session is still running
    /// on its own boot defaults.
    NotEstablished,
    /// The server registered this client and the session then ended after
    /// `lived`, cleanly or with an error.
    Ended { lived: Duration },
}

/// What the pair loop does after one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReconnectPlan {
    /// Reconnect after the short delay.
    Soon,
    /// The retry budget is spent. Keep reconnecting, slowly.
    Slow,
    /// The retry budget is spent and the server never saw this session, so
    /// the stored device mute is never arriving. Release the boot mute, then
    /// keep reconnecting slowly.
    ReleaseStartupMuteThenSlow,
}

/// Reconnect policy for the pair websocket. Pure so the mute decision is
/// testable; `main` owns the sleeping, logging, and the mute write.
pub(super) struct PairRetryPolicy {
    consecutive_failures: u32,
    /// The server has registered this session at least once, so it read the
    /// `client_state` that was waiting for it and applied the stored device
    /// audio, or deliberately applied nothing because the session already
    /// matched. Either way the answer arrived and the boot mute is no longer
    /// this session's own guess.
    server_saw_session: bool,
    startup_mute_released: bool,
}

impl PairRetryPolicy {
    pub(super) fn new() -> Self {
        Self {
            consecutive_failures: 0,
            server_saw_session: false,
            startup_mute_released: false,
        }
    }

    pub(super) fn note_attempt(&mut self, attempt: PairAttempt) -> ReconnectPlan {
        match attempt {
            PairAttempt::NotEstablished => {}
            PairAttempt::Ended { lived } => {
                self.server_saw_session = true;
                if lived >= STABLE_CONNECTION {
                    self.consecutive_failures = 0;
                }
            }
        }
        self.consecutive_failures += 1;

        if self.consecutive_failures <= MAX_CONSECUTIVE_FAILURES {
            return ReconnectPlan::Soon;
        }
        // Silence is the safe failure mode. Only a session the server never
        // saw is still on its own boot mute, and only that session may drop
        // it; anything else is already running what the user chose. The slow
        // retry keeps running either way, so a session that released the mute
        // still gets the stored value applied when the server comes back.
        if self.server_saw_session || self.startup_mute_released {
            return ReconnectPlan::Slow;
        }
        self.startup_mute_released = true;
        ReconnectPlan::ReleaseStartupMuteThenSlow
    }
}

/// Open the pair socket and hand the server this client's state.
///
/// Returning `Ok` only means the `client_state` that triggers the server's
/// device-audio alignment is on the wire. Whether the server ever read it is
/// [`PairSessionEnd::server_frame_received`]'s call: the server accepts the
/// upgrade before it checks the per-IP pair limit and the per-token capacity,
/// so a rejected socket still takes this write and then dies unread.
pub(super) async fn establish_pair_session(
    api_base_url: &str,
    token: &str,
    client: &PairClientInfo,
    playback: &PlaybackState<'_>,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!("connecting pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .context("timed out connecting to pair websocket")?
        .context("failed to connect to pair websocket")?;
    info!("pair websocket established");
    send_client_state(&mut ws, client, playback).await?;
    Ok(ws)
}

/// How a pair session ended, from [`run_pair_session`].
pub(super) struct PairSessionEnd {
    /// At least one frame arrived from the server. The server sends
    /// `set_playback_source` right after registering a client and before it
    /// reads anything, so one frame proves the registration happened and the
    /// `client_state` waiting in its buffer was read. A socket the server
    /// accepted and then dropped unread never sends one.
    pub(super) server_frame_received: bool,
    pub(super) result: Result<()>,
}

impl PairSessionEnd {
    /// What this session was, from the retry loop's point of view. Only a
    /// session the server registered may count as `Ended`; anything else is
    /// still running on its boot defaults and must keep the release path open.
    pub(super) fn attempt(&self, lived: Duration) -> PairAttempt {
        if self.server_frame_received {
            PairAttempt::Ended { lived }
        } else {
            PairAttempt::NotEstablished
        }
    }
}

pub(super) async fn run_pair_session(
    mut ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    client: &PairClientInfo,
    playback: &PlaybackState<'_>,
    runtime: PairRuntime<'_>,
) -> PairSessionEnd {
    let mut server_frame_received = false;
    let result = pair_session_loop(
        &mut ws,
        client,
        playback,
        runtime,
        &mut server_frame_received,
    )
    .await;
    PairSessionEnd {
        server_frame_received,
        result,
    }
}

/// The message loop proper. Split out so every `?` exit still reports
/// `server_frame_received` through [`run_pair_session`].
async fn pair_session_loop(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    client: &PairClientInfo,
    playback: &PlaybackState<'_>,
    runtime: PairRuntime<'_>,
    server_frame_received: &mut bool,
) -> Result<()> {
    let mut heartbeat = interval(Duration::from_secs(1));
    let mut voice_state_heartbeat = interval(Duration::from_secs(15));
    let mut voice_speaking_poll = interval(Duration::from_millis(250));
    if runtime.voice.joined {
        send_voice_state(ws, runtime.voice).await?;
    }

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if runtime.voice.joined && runtime.voice.media_disconnected() {
                    warn!("voice media disconnected; leaving voice state");
                    runtime.voice.leave().await;
                    send_voice_state(ws, runtime.voice).await?;
                }
                let payload = json!({
                    "event": "heartbeat",
                    "position_ms": playback_position_ms(playback.played_samples, playback.sample_rate),
                });
                ws.send(Message::Text(payload.to_string().into())).await?;
                runtime.webview.maintain_helper(
                    playback.muted.load(Ordering::Relaxed),
                    playback.volume_percent.load(Ordering::Relaxed),
                );
            }
            // A desktop media client (widget play/pause, a media key, the
            // volume slider) issued a control. It is not applied locally: the
            // server fans the resulting set_muted/set_volume back to every
            // paired client, this CLI and the webview helper alike, which is
            // what lets a widget press mute YouTube too.
            Some(command) = runtime.desktop_commands.recv() => {
                send_desktop_command(ws, command).await?;
            }
            _ = voice_state_heartbeat.tick(), if runtime.voice.joined => {
                send_voice_state(ws, runtime.voice).await?;
            }
            _ = voice_speaking_poll.tick(), if runtime.voice.joined => {
                if runtime.voice.sync_speaking_from_media() {
                    send_voice_state(ws, runtime.voice).await?;
                }
            }
            maybe_msg = ws.next() => {
                let Some(msg) = maybe_msg else {
                    break;
                };
                *server_frame_received = true;
                match msg? {
                    Message::Text(text) => {
                        let should_send_state =
                            handle_pair_control(
                                &text,
                                ws,
                                playback,
                                runtime.webview,
                                runtime.voice,
                                runtime.desktop_media,
                            )
                            .await?;
                        if should_send_state {
                            send_client_state(ws, client, playback).await?;
                        }
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
    client: &PairClientInfo,
    playback: &PlaybackState<'_>,
) -> Result<()> {
    let payload = json!({
        "event": "client_state",
        "client_kind": "cli",
        "ssh_mode": client.ssh_mode,
        "platform": client.platform,
        "capabilities": CLIENT_CAPABILITIES,
        "muted": playback.muted.load(Ordering::Relaxed),
        "volume_percent": playback.volume_percent.load(Ordering::Relaxed),
    });
    ws.send(Message::Text(payload.to_string().into())).await?;
    Ok(())
}

async fn handle_pair_control(
    text: &str,
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    playback: &PlaybackState<'_>,
    webview: &mut WebviewPlaybackController,
    voice: &mut VoiceRuntimeState,
    desktop_media: &mut DesktopMedia,
) -> Result<bool> {
    let control = match serde_json::from_str::<PairControlMessage>(text) {
        Ok(control) => control,
        Err(_) => {
            warn!(payload = %text, "ignoring unsupported pair websocket event");
            return Ok(false);
        }
    };
    match control {
        audio_control @ (PairControlMessage::ToggleMute
        | PairControlMessage::VolumeUp
        | PairControlMessage::VolumeDown
        | PairControlMessage::SetMuted { .. }
        | PairControlMessage::SetVolume { .. }) => {
            apply_audio_pair_control(audio_control, playback.muted, playback.volume_percent);
            desktop_media.republish_audio_state();
            Ok(true)
        }
        PairControlMessage::SetPlaybackSource {
            source,
            stream_url: server_stream_url,
            station,
        } => {
            // Only reachable against an old server that omits stream_url for
            // radio; current servers resolve URLs in late-ssh stations.rs.
            // Keep this in sync with the Chillsynth entry there.
            let legacy_radio_url = "https://stream.nightride.fm/chillsynth.mp3";
            let fallback_stream_url = match source {
                PairAudioSource::Icecast => Some(playback.icecast_stream_url),
                PairAudioSource::Radio => Some(legacy_radio_url),
                PairAudioSource::Youtube => None,
            };
            let local_stream_url = server_stream_url.as_deref().or(fallback_stream_url);
            let emits_native_audio = local_stream_url.is_some();
            // Record intent before bumping the stream generation so a
            // decoder switch finishing mid-handler reads the fresh value;
            // the decoder only re-enables output while this is true.
            playback
                .native_source_selected
                .store(emits_native_audio, Ordering::Relaxed);
            let stream_changed = if let Some(url) = local_stream_url {
                if set_stream_url(playback.stream_url, playback.stream_generation, url) {
                    playback.source_is_icecast.store(false, Ordering::Relaxed);
                    info!(
                        source = ?source,
                        station = station.as_deref().unwrap_or(""),
                        "retargeting native audio stream"
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            };
            let previous = if emits_native_audio && !stream_changed {
                if playback.stream_flushed_generation.load(Ordering::SeqCst)
                    >= playback.stream_generation.load(Ordering::SeqCst)
                {
                    playback.source_is_icecast.swap(true, Ordering::Relaxed)
                } else {
                    false
                }
            } else if emits_native_audio {
                false
            } else {
                playback.source_is_icecast.swap(false, Ordering::Relaxed)
            };
            if previous != emits_native_audio && !stream_changed {
                info!(
                    source = ?source,
                    "applied playback source change"
                );
            }
            webview.apply_playback_source(
                source,
                playback.muted.load(Ordering::Relaxed),
                playback.volume_percent.load(Ordering::Relaxed),
            )?;
            desktop_media.select_source(
                source.into(),
                station,
                local_stream_url.map(str::to_string),
            );
            Ok(false)
        }
        PairControlMessage::QueueUpdate { current } => {
            desktop_media.update_youtube(current);
            Ok(false)
        }
        PairControlMessage::NowPlayingUpdate { mounts } => {
            desktop_media.update_icecast(mounts);
            Ok(false)
        }
        PairControlMessage::RadioMetaUpdate { stations } => {
            desktop_media.update_radio(stations);
            Ok(false)
        }
        PairControlMessage::RequestClipboardImage { request_id } => {
            send_clipboard_image(ws, request_id).await?;
            Ok(false)
        }
        PairControlMessage::VoiceJoin {
            room,
            url,
            token,
            muted,
            deafened,
        } => {
            match voice
                .join(room.clone(), url.clone(), token, muted, deafened)
                .await
            {
                Ok(()) => {
                    info!(
                        room = %room,
                        url = %url,
                        muted,
                        deafened,
                        "joined voice room from CLI"
                    );
                }
                Err(err) => {
                    warn!(
                        room = %room,
                        url = %url,
                        error = ?err,
                        "failed to join voice room from CLI"
                    );
                }
            }
            send_voice_state(ws, voice).await?;
            Ok(false)
        }
        PairControlMessage::VoiceLeave => {
            voice.leave().await;
            info!("left voice room from CLI");
            send_voice_state(ws, voice).await?;
            Ok(false)
        }
        PairControlMessage::VoiceSetMuted { muted } => {
            voice.set_muted(muted);
            info!(muted, "updated voice microphone mute");
            send_voice_state(ws, voice).await?;
            Ok(false)
        }
        PairControlMessage::VoiceSetDeafened { deafened } => {
            voice.set_deafened(deafened);
            info!(deafened, "updated voice deafen state");
            send_voice_state(ws, voice).await?;
            Ok(false)
        }
        PairControlMessage::OpenUrl { url } => {
            open_url_in_browser(&url);
            Ok(false)
        }
    }
}

/// Open a server-sent URL in the default browser. Only `https://`/`http://`
/// pass: the server is trusted, but a URL is the one string here that ends
/// up as a command argument, so the scheme gate stays.
fn open_url_in_browser(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        warn!(url = %url, "refusing to open non-http url");
        return;
    }
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result: std::io::Result<std::process::Child> =
        Err(std::io::Error::other("no browser opener on this platform"));
    match result {
        Ok(_) => info!(url = %url, "opened url in browser"),
        Err(err) => warn!(url = %url, error = ?err, "failed to open url in browser"),
    }
}

/// Forward a desktop media command (MPRIS play/pause, media keys, the volume
/// slider) to the server as its pair-WS event. The server fans the result
/// back to every paired client; nothing is applied locally here.
#[cfg(target_os = "linux")]
async fn send_desktop_command(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    command: DesktopCommand,
) -> Result<()> {
    let payload = match command {
        DesktopCommand::SetMuted { muted } => json!({
            "event": "set_muted",
            "muted": muted,
        }),
        DesktopCommand::SetVolume { volume_percent } => json!({
            "event": "set_volume",
            "volume_percent": volume_percent,
        }),
    };
    ws.send(Message::Text(payload.to_string().into())).await?;
    Ok(())
}

/// Off-Linux `DesktopCommand` is uninhabited, so this can never be reached;
/// the empty match proves it to the compiler and keeps the pair loop cfg-free.
#[cfg(not(target_os = "linux"))]
fn send_desktop_command(
    _ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    command: DesktopCommand,
) -> std::future::Ready<Result<()>> {
    match command {}
}

fn set_stream_url(stream_url: &Mutex<String>, stream_generation: &AtomicU64, url: &str) -> bool {
    let mut current = stream_url
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if current.as_str() != url {
        *current = url.to_string();
        stream_generation.fetch_add(1, Ordering::SeqCst);
        return true;
    }
    false
}

async fn send_voice_state(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    voice: &VoiceRuntimeState,
) -> Result<()> {
    let payload = json!({
        "event": "voice_state",
        "joined": voice.joined,
        "room": voice.room,
        "muted": voice.muted,
        "deafened": voice.deafened,
        "speaking": voice.speaking,
    });
    ws.send(Message::Text(payload.to_string().into())).await?;
    Ok(())
}

fn apply_audio_pair_control(
    control: PairControlMessage,
    muted: &AtomicBool,
    volume_percent: &AtomicU8,
) {
    match control {
        PairControlMessage::ToggleMute => {
            let now_muted = muted.fetch_xor(true, Ordering::Relaxed) ^ true;
            info!(muted = now_muted, "applied paired mute toggle");
        }
        PairControlMessage::VolumeUp => {
            let new_volume = bump_volume(volume_percent, 5);
            info!(volume_percent = new_volume, "applied paired volume up");
        }
        PairControlMessage::VolumeDown => {
            let new_volume = bump_volume(volume_percent, -5);
            info!(volume_percent = new_volume, "applied paired volume down");
        }
        PairControlMessage::SetMuted { muted: new_muted } => {
            muted.store(new_muted, Ordering::Relaxed);
            info!(muted = new_muted, "applied paired mute set");
        }
        PairControlMessage::SetVolume {
            volume_percent: new_volume,
        } => {
            volume_percent.store(new_volume, Ordering::Relaxed);
            // A slider dragged off zero is a widget's only way back from
            // pause, so a non-zero volume also unmutes; the webview helper
            // applies the same rule to its own fan-out copy.
            if new_volume > 0 {
                muted.store(false, Ordering::Relaxed);
            }
            info!(volume_percent = new_volume, "applied paired volume set");
        }
        PairControlMessage::SetPlaybackSource { .. }
        | PairControlMessage::QueueUpdate { .. }
        | PairControlMessage::NowPlayingUpdate { .. }
        | PairControlMessage::RadioMetaUpdate { .. }
        | PairControlMessage::RequestClipboardImage { .. }
        | PairControlMessage::VoiceJoin { .. }
        | PairControlMessage::VoiceLeave
        | PairControlMessage::VoiceSetMuted { .. }
        | PairControlMessage::VoiceSetDeafened { .. }
        | PairControlMessage::OpenUrl { .. } => {}
    }
}

async fn send_clipboard_image(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    request_id: Option<u64>,
) -> Result<()> {
    let image_result = tokio::task::spawn_blocking(clipboard::image_png_bytes)
        .await
        .map_err(|err| anyhow::anyhow!("clipboard image task failed: {err}"))?;
    let payload = match image_result {
        Ok(bytes) => json!({
            "event": "clipboard_image",
            "data_base64": STANDARD.encode(bytes),
            "request_id": request_id,
        }),
        Err(err) => json!({
            "event": "clipboard_image_failed",
            "message": err.to_string(),
            "request_id": request_id,
        }),
    };
    ws.send(Message::Text(payload.to_string().into())).await?;
    Ok(())
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

pub(super) const fn client_platform_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "android")]
    {
        "android"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "android",
        target_os = "linux"
    )))]
    {
        "unknown"
    }
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

#[cfg(test)]
#[path = "ws_test.rs"]
mod ws_test;
