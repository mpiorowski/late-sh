//! Pair-WS relay task used by `late webview-pair <token>`.
//!
//! Connects to /api/ws/pair?token=..., registers as `client_kind = "browser"`,
//! relays inbound `load_video` / `source_changed` server messages to the
//! webview, and forwards `player_state` events back to the server.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tao::event_loop::EventLoopProxy;
use tokio::{sync::mpsc, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::commands::{WebviewCommand, WebviewEvent};
use crate::ws::client_platform_label;

/// Tag the webview sends on the wire. Server-side recognises `"browser"`
/// today; a future `"embedded_webview"` variant slots in here without
/// touching the protocol.
const CLIENT_KIND: &str = "browser";
const DEFAULT_VOLUME_PERCENT: u8 = 30;

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum ServerMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    LoadVideo {
        item_id: String,
        video_id: String,
        #[serde(default)]
        is_stream: bool,
    },
    SourceChanged {
        audio_mode: String,
    },
    QueueUpdate(serde_json::Value),
    SetPlaybackSource {
        source: PairAudioSource,
        #[serde(default)]
        web_icecast_enabled: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PairAudioSource {
    Icecast,
    Youtube,
}

#[derive(Debug, Clone, Copy)]
struct AudioSettings {
    muted: bool,
    volume_percent: u8,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            muted: false,
            volume_percent: DEFAULT_VOLUME_PERCENT,
        }
    }
}

pub async fn run(
    api_base_url: &str,
    token: &str,
    proxy: EventLoopProxy<WebviewCommand>,
    mut ipc_rx: mpsc::UnboundedReceiver<WebviewEvent>,
) -> Result<()> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!(%ws_url, "connecting webview pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .with_context(|| format!("timed out connecting to pair websocket at {ws_url}"))?
        .with_context(|| format!("failed to connect to pair websocket at {ws_url}"))?;
    info!("webview pair websocket established");

    let mut audio_settings = AudioSettings::default();
    send_client_state(&mut ws, audio_settings).await?;
    let mut heartbeat = interval(Duration::from_secs(1));
    heartbeat.tick().await;

    let mut current_item_id: Option<String> = None;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let payload = json!({ "event": "heartbeat" });
                ws.send(Message::Text(payload.to_string().into())).await
                    .context("failed to send heartbeat")?;
            }
            event = ipc_rx.recv() => {
                let Some(event) = event else {
                    debug!("webview ipc channel closed; stopping pair task");
                    break;
                };
                if let Err(err) = handle_webview_event(&mut ws, event, &current_item_id).await {
                    warn!(error = %err, "failed to forward webview event");
                }
            }
            inbound = ws.next() => {
                let Some(inbound) = inbound else { break; };
                match inbound? {
                    Message::Text(text) => {
                        let result = handle_server_text(text.as_str(), &proxy, &mut audio_settings).await;
                        if result.send_client_state {
                            send_client_state(&mut ws, audio_settings).await?;
                        }
                        if let Some(item_id) = result.current_item_id {
                            current_item_id = Some(item_id);
                        }
                    }
                    Message::Close(_) => {
                        info!("server closed webview pair websocket");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = proxy.send_event(WebviewCommand::Shutdown);
    Ok(())
}

#[derive(Default)]
struct ServerTextResult {
    current_item_id: Option<String>,
    send_client_state: bool,
}

async fn handle_server_text(
    text: &str,
    proxy: &EventLoopProxy<WebviewCommand>,
    audio_settings: &mut AudioSettings,
) -> ServerTextResult {
    let Ok(message) = serde_json::from_str::<ServerMessage>(text) else {
        debug!(payload = %text, "ignoring unrecognized pair ws message");
        return ServerTextResult::default();
    };
    match message {
        ServerMessage::ToggleMute => {
            audio_settings.muted = !audio_settings.muted;
            send_audio_settings(proxy, *audio_settings);
            ServerTextResult {
                send_client_state: true,
                ..ServerTextResult::default()
            }
        }
        ServerMessage::VolumeUp => {
            audio_settings.volume_percent = bump_volume(audio_settings.volume_percent, 5);
            audio_settings.muted = false;
            send_audio_settings(proxy, *audio_settings);
            ServerTextResult {
                send_client_state: true,
                ..ServerTextResult::default()
            }
        }
        ServerMessage::VolumeDown => {
            audio_settings.volume_percent = bump_volume(audio_settings.volume_percent, -5);
            send_audio_settings(proxy, *audio_settings);
            ServerTextResult {
                send_client_state: true,
                ..ServerTextResult::default()
            }
        }
        ServerMessage::LoadVideo {
            item_id,
            video_id,
            is_stream,
        } => {
            debug!(
                %item_id,
                %video_id,
                is_stream,
                "dispatching load_video to embedded webview"
            );
            let id_for_state = item_id.clone();
            if let Err(err) = proxy.send_event(WebviewCommand::LoadVideo {
                item_id,
                video_id,
                is_stream,
            }) {
                warn!(error = %err, "event loop closed while sending load_video");
                return ServerTextResult::default();
            }
            ServerTextResult {
                current_item_id: Some(id_for_state),
                ..ServerTextResult::default()
            }
        }
        ServerMessage::SourceChanged { audio_mode } => {
            debug!(%audio_mode, "dispatching source_changed to embedded webview");
            if let Err(err) = proxy.send_event(WebviewCommand::SourceChanged { audio_mode }) {
                warn!(error = %err, "event loop closed while sending source_changed");
            }
            ServerTextResult::default()
        }
        ServerMessage::QueueUpdate(payload) => {
            let _ = payload;
            ServerTextResult::default()
        }
        ServerMessage::SetPlaybackSource {
            source,
            web_icecast_enabled,
        } => {
            debug!(
                ?source,
                web_icecast_enabled,
                "server requested playback source (ignored by embedded webview)"
            );
            ServerTextResult::default()
        }
    }
}

fn send_audio_settings(proxy: &EventLoopProxy<WebviewCommand>, settings: AudioSettings) {
    debug!(
        muted = settings.muted,
        volume_percent = settings.volume_percent,
        "dispatching audio settings to embedded webview"
    );
    if let Err(err) = proxy.send_event(WebviewCommand::AudioSettings {
        muted: settings.muted,
        volume_percent: settings.volume_percent,
    }) {
        warn!(error = %err, "event loop closed while sending audio settings");
    }
}

fn bump_volume(volume_percent: u8, delta: i16) -> u8 {
    let next = volume_percent as i16 + delta;
    next.clamp(0, 100) as u8
}

async fn handle_webview_event(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    event: WebviewEvent,
    current_item_id: &Option<String>,
) -> Result<()> {
    let payload = match event {
        WebviewEvent::State {
            item_id,
            state,
            position_ms,
            duration_ms,
            autoplay_blocked,
        } => {
            let resolved = item_id.or_else(|| current_item_id.clone());
            json!({
                "event": "player_state",
                "item_id": resolved,
                "state": state,
                "offset_ms": position_ms,
                "duration_ms": duration_ms,
                "autoplay_blocked": autoplay_blocked,
                "error": serde_json::Value::Null,
            })
        }
        WebviewEvent::Error { item_id, code } => {
            let resolved = item_id.or_else(|| current_item_id.clone());
            json!({
                "event": "player_state",
                "item_id": resolved,
                "state": "error",
                "offset_ms": 0,
                "duration_ms": serde_json::Value::Null,
                "autoplay_blocked": false,
                "error": code,
            })
        }
        WebviewEvent::AutoplayBlocked { item_id } => {
            let resolved = item_id.or_else(|| current_item_id.clone());
            json!({
                "event": "player_state",
                "item_id": resolved,
                "state": "buffering",
                "offset_ms": 0,
                "duration_ms": serde_json::Value::Null,
                "autoplay_blocked": true,
                "error": serde_json::Value::Null,
            })
        }
        WebviewEvent::Ready | WebviewEvent::SourceAck { .. } | WebviewEvent::ShutdownAck => {
            debug!(?event, "informational webview event");
            return Ok(());
        }
        WebviewEvent::ApiLoadFailed => {
            warn!("youtube iframe api failed to load in the embedded webview");
            return Ok(());
        }
    };
    ws.send(Message::Text(payload.to_string().into()))
        .await
        .context("failed to send player_state")?;
    Ok(())
}

async fn send_client_state(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    audio_settings: AudioSettings,
) -> Result<()> {
    let payload = json!({
        "event": "client_state",
        "client_kind": CLIENT_KIND,
        "ssh_mode": "webview",
        "platform": client_platform_label(),
        "capabilities": ["youtube"],
        "muted": audio_settings.muted,
        "volume_percent": audio_settings.volume_percent,
    });
    ws.send(Message::Text(payload.to_string().into()))
        .await
        .context("failed to send client_state")?;
    Ok(())
}

fn pair_ws_url(api_base_url: &str, token: &str) -> Result<String> {
    let base = api_base_url.trim_end_matches('/');
    let rewritten = if let Some(rest) = base.strip_prefix("https://") {
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
        rewritten.trim_end_matches('/')
    ))
}
