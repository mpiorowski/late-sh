use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::{
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
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const CLIENT_CAPABILITIES: &[&str] = &["clipboard_image"];

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const CLIENT_CAPABILITIES: &[&str] = &[];

pub(super) async fn run_viz_ws(
    api_base_url: &str,
    token: &str,
    client: &PairClientInfo,
    frames: &mut broadcast::Receiver<VizSample>,
    playback: &PlaybackState<'_>,
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
            apply_force_mute(muted, mute);
            Ok(true)
        }
        PairControlMessage::RequestClipboardImage => {
            send_clipboard_image(ws).await?;
            Ok(false)
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
