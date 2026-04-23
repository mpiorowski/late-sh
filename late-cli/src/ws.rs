use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    time::Duration,
};
use tokio::{sync::broadcast, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info};

use super::audio::VizSample;

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
}

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
                    Message::Text(text)
                        if apply_pair_control(&text, playback.muted, playback.volume_percent)? =>
                    {
                        send_client_state(&mut ws, client, playback).await?;
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
        "muted": playback.muted.load(Ordering::Relaxed),
        "volume_percent": playback.volume_percent.load(Ordering::Relaxed),
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
}
