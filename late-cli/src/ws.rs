use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs::OpenOptions,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    time::Duration,
};
use tokio::{sync::broadcast, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::{audio::VizSample, clipboard};

pub(super) struct PairClientInfo {
    pub(super) ssh_mode: &'static str,
    pub(super) platform: &'static str,
}

pub(super) struct PlaybackState<'a> {
    pub(super) played_samples: &'a AtomicU64,
    pub(super) sample_rate: u32,
    pub(super) muted: &'a AtomicBool,
    pub(super) volume_percent: &'a AtomicU8,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    RequestClipboardImage,
    ForceMute { mute: bool },
    SetPlaybackSource { source: String },
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const CLIENT_CAPABILITIES: &[&str] = &["clipboard_image", "youtube"];

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const CLIENT_CAPABILITIES: &[&str] = &[];

pub(super) struct WebviewPlaybackController {
    api_base_url: String,
    token: String,
    child: Option<Child>,
    wants_youtube: bool,
}

impl WebviewPlaybackController {
    pub(super) fn new(api_base_url: String, token: String) -> Self {
        Self {
            api_base_url,
            token,
            child: None,
            wants_youtube: false,
        }
    }

    fn wants_youtube(&self) -> bool {
        self.wants_youtube
    }

    fn apply_playback_source(&mut self, source: &str, muted: &AtomicBool) -> Result<bool> {
        match source {
            "youtube" => self.enter_youtube(muted),
            "icecast" => self.enter_icecast(muted),
            other => {
                warn!(source = %other, "ignoring unknown playback source");
                Ok(false)
            }
        }
    }

    fn enter_youtube(&mut self, muted: &AtomicBool) -> Result<bool> {
        self.wants_youtube = true;
        let was_muted = muted.swap(true, Ordering::Relaxed);
        let muted_changed = !was_muted;
        if self.helper_is_running() {
            return Ok(muted_changed);
        }

        let exe = std::env::current_exe().context("failed to locate current late executable")?;
        let stderr = webview_helper_stderr()?;
        let child = match Command::new(exe)
            .arg("webview-pair")
            .arg(&self.token)
            .env("LATE_API_BASE_URL", &self.api_base_url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                muted.store(was_muted, Ordering::Relaxed);
                return Err(err).context("failed to spawn embedded YouTube webview helper");
            }
        };
        self.child = Some(child);
        info!("started embedded YouTube webview helper");
        Ok(true)
    }

    fn enter_icecast(&mut self, muted: &AtomicBool) -> Result<bool> {
        self.wants_youtube = false;
        self.stop_helper();
        let muted_changed = muted.swap(false, Ordering::Relaxed);
        if muted_changed {
            info!("resumed native Icecast playback");
        }
        Ok(muted_changed)
    }

    fn helper_is_running(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                warn!(?status, "embedded YouTube webview helper exited");
                self.child = None;
                false
            }
            Ok(None) => true,
            Err(err) => {
                warn!(error = %err, "failed to inspect embedded YouTube webview helper");
                self.child = None;
                false
            }
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

fn webview_helper_stderr() -> Result<Stdio> {
    let path = std::env::temp_dir().join("late-webview.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open webview helper log at {}", path.display()))?;
    Ok(Stdio::from(file))
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
) -> Result<()> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!(%ws_url, "connecting pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .with_context(|| format!("timed out connecting to pair websocket at {ws_url}"))?
        .with_context(|| format!("failed to connect to pair websocket at {ws_url}"))?;
    info!("pair websocket established");
    let mut heartbeat = interval(Duration::from_secs(1));
    send_client_state(&mut ws, client, playback).await?;

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
                let payload = json!({
                    "event": "heartbeat",
                    "position_ms": playback_position_ms(playback.played_samples, playback.sample_rate),
                });
                ws.send(Message::Text(payload.to_string().into())).await?;
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
                            webview,
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
    webview: &mut WebviewPlaybackController,
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
        PairControlMessage::ForceMute { mute } => {
            if !mute && webview.wants_youtube() {
                debug!("ignoring force-unmute while YouTube webview is selected");
                return Ok(false);
            }
            apply_force_mute(muted, mute);
            Ok(true)
        }
        PairControlMessage::RequestClipboardImage => {
            send_clipboard_image(ws).await?;
            Ok(false)
        }
        PairControlMessage::SetPlaybackSource { source } => {
            webview.apply_playback_source(&source, muted)
        }
    }
}

fn apply_force_mute(muted: &AtomicBool, mute: bool) {
    let previous = muted.swap(mute, Ordering::Relaxed);
    if previous != mute {
        info!(muted = mute, "applied server-forced mute");
    }
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
        PairControlMessage::ForceMute { .. } | PairControlMessage::RequestClipboardImage => {}
        PairControlMessage::SetPlaybackSource { .. } => {}
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
