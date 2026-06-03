use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio::{sync::broadcast, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::{audio::VizSample, clipboard, voice::VoiceRuntimeState};

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
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    RequestClipboardImage,
    SetPlaybackSource {
        source: PairAudioSource,
        #[serde(default = "default_embedded_webview_enabled")]
        embedded_webview_enabled: bool,
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
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum PairAudioSource {
    Icecast,
    Youtube,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const CLIENT_CAPABILITIES: &[&str] = &["clipboard_image", "youtube", "voice"];

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const CLIENT_CAPABILITIES: &[&str] = &[];

const fn default_embedded_webview_enabled() -> bool {
    true
}

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
        embedded_webview_enabled: bool,
    ) -> Result<()> {
        match (source, embedded_webview_enabled) {
            (PairAudioSource::Youtube, true) => self.enter_youtube(),
            (PairAudioSource::Youtube, false) => self.enter_browser_youtube(),
            (PairAudioSource::Icecast, _) => self.enter_icecast(),
        }
    }

    fn enter_youtube(&mut self) -> Result<()> {
        self.wants_youtube = true;
        if self.helper_is_running() {
            return Ok(());
        }
        if self.helper_backoff_active() {
            return Ok(());
        }

        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(err) => {
                warn!(error = %err, "failed to locate current late executable for webview helper");
                self.record_helper_start_failure();
                return Ok(());
            }
        };
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
        let mut command = Command::new(exe);
        command
            .arg("webview-pair")
            .env("LATE_API_BASE_URL", &self.api_base_url)
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

        let child = match command.spawn() {
            Ok(child) => child,
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

    fn enter_browser_youtube(&mut self) -> Result<()> {
        if !self.wants_youtube && self.child.is_none() {
            return Ok(());
        }
        self.wants_youtube = false;
        self.stop_helper();
        info!("using paired browser for YouTube playback");
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
            warn!(
                crash_count = self.crash_count,
                backoff_secs = WEBVIEW_CRASH_BACKOFF.as_secs(),
                log_path = ?self.helper_log_path.as_deref(),
                "{message}; temporarily disabling embedded YouTube fallback"
            );
        }
    }

    fn stop_helper(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Err(err) = child.kill() {
            warn!(error = %err, "failed to stop embedded YouTube webview helper");
            return;
        }
        let _ = child.wait();
        info!("stopped embedded YouTube webview helper");
    }
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

pub(super) async fn run_viz_ws(
    api_base_url: &str,
    token: &str,
    client: &PairClientInfo,
    frames: &mut broadcast::Receiver<VizSample>,
    playback: &PlaybackState<'_>,
    webview: &mut WebviewPlaybackController,
    voice: &mut VoiceRuntimeState,
) -> Result<()> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!("connecting pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .context("timed out connecting to pair websocket")?
        .context("failed to connect to pair websocket")?;
    info!("pair websocket established");
    let mut heartbeat = interval(Duration::from_secs(1));
    let mut voice_state_heartbeat = interval(Duration::from_secs(15));
    send_client_state(&mut ws, client, playback).await?;
    if voice.joined {
        send_voice_state(&mut ws, voice).await?;
    }

    loop {
        tokio::select! {
            recv = frames.recv() => {
                let frame = match recv {
                    Ok(frame) => frame,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let position_ms =
                    playback_position_ms(playback.played_samples, playback.sample_rate);
                let payload = json!({
                    "event": "viz",
                    "position_ms": position_ms,
                    "bands": frame.bands,
                    "rms": frame.rms,
                });
                ws.send(Message::Text(payload.to_string().into())).await?;
            }
            _ = heartbeat.tick() => {
                if voice.joined && voice.media_disconnected() {
                    warn!("voice media disconnected; leaving voice state");
                    voice.leave().await;
                    send_voice_state(&mut ws, voice).await?;
                }
                let payload = json!({
                    "event": "heartbeat",
                    "position_ms": playback_position_ms(playback.played_samples, playback.sample_rate),
                });
                ws.send(Message::Text(payload.to_string().into())).await?;
            }
            _ = voice_state_heartbeat.tick(), if voice.joined => {
                send_voice_state(&mut ws, voice).await?;
            }
            maybe_msg = ws.next() => {
                let Some(msg) = maybe_msg else {
                    break;
                };
                match msg? {
                    Message::Text(text) => {
                        let should_send_state = handle_pair_control(
                            &text,
                            &mut ws,
                            playback.muted,
                            playback.volume_percent,
                            playback.source_is_icecast,
                            webview,
                            voice,
                        )
                        .await?;
                        if should_send_state {
                            send_client_state(&mut ws, client, playback).await?;
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
    muted: &AtomicBool,
    volume_percent: &AtomicU8,
    source_is_icecast: &AtomicBool,
    webview: &mut WebviewPlaybackController,
    voice: &mut VoiceRuntimeState,
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
        | PairControlMessage::VolumeDown) => {
            apply_audio_pair_control(audio_control, muted, volume_percent);
            Ok(true)
        }
        PairControlMessage::SetPlaybackSource {
            source,
            embedded_webview_enabled,
        } => {
            let is_icecast = matches!(source, PairAudioSource::Icecast);
            let previous = source_is_icecast.swap(is_icecast, Ordering::Relaxed);
            if previous != is_icecast {
                info!(
                    source = ?source,
                    "applied playback source change"
                );
            }
            webview.apply_playback_source(source, embedded_webview_enabled)?;
            Ok(false)
        }
        PairControlMessage::RequestClipboardImage => {
            send_clipboard_image(ws).await?;
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
    }
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
        PairControlMessage::SetPlaybackSource { .. }
        | PairControlMessage::RequestClipboardImage
        | PairControlMessage::VoiceJoin { .. }
        | PairControlMessage::VoiceLeave
        | PairControlMessage::VoiceSetMuted { .. }
        | PairControlMessage::VoiceSetDeafened { .. } => {}
    }
}

async fn send_clipboard_image(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<()> {
    let image_result = tokio::task::spawn_blocking(clipboard::image_png_bytes)
        .await
        .map_err(|err| anyhow::anyhow!("clipboard image task failed: {err}"))?;
    let payload = match image_result {
        Ok(bytes) => json!({
            "event": "clipboard_image",
            "data_base64": STANDARD.encode(bytes),
        }),
        Err(err) => json!({
            "event": "clipboard_image_failed",
            "message": err.to_string(),
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
mod tests {
    use super::*;

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

        apply_audio_pair_control(PairControlMessage::ToggleMute, &muted, &volume_percent);
        assert!(muted.load(Ordering::Relaxed));

        apply_audio_pair_control(PairControlMessage::ToggleMute, &muted, &volume_percent);
        assert!(!muted.load(Ordering::Relaxed));
    }

    #[test]
    fn apply_pair_control_adjusts_volume() {
        let muted = AtomicBool::new(false);
        let volume_percent = AtomicU8::new(50);

        apply_audio_pair_control(PairControlMessage::VolumeUp, &muted, &volume_percent);
        assert_eq!(volume_percent.load(Ordering::Relaxed), 55);

        apply_audio_pair_control(PairControlMessage::VolumeDown, &muted, &volume_percent);
        assert_eq!(volume_percent.load(Ordering::Relaxed), 50);
    }
}
