//! Pair-WS relay task used by `late webview-pair`.
//!
//! Connects to /api/ws/pair?token=..., registers as `client_kind = "browser"`
//! with `ssh_mode = "webview"`, relays inbound `load_video` / `source_changed`
//! server messages to the webview, and forwards `player_state` events back to
//! the server.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tao::event_loop::EventLoopProxy;
use tokio::{sync::mpsc, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::commands::{WebviewCommand, WebviewEvent};
use crate::ws::client_platform_label;

/// Tag the webview sends on the wire. Server-side still treats the helper as a
/// browser, but distinguishes it from a real browser through `ssh_mode`.
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
    QueueUpdate {
        #[serde(default)]
        current: Option<QueueItemSnapshot>,
    },
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
    Radio,
}

#[derive(Debug, Clone, Copy)]
struct AudioSettings {
    muted: bool,
    volume_percent: u8,
}

#[derive(Debug, Deserialize)]
struct QueueItemSnapshot {
    id: String,
    video_id: String,
    #[serde(default)]
    started_at_ms: Option<i64>,
    #[serde(default)]
    duration_ms: Option<i64>,
    #[serde(default)]
    is_stream: bool,
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
    let result = run_inner(api_base_url, token, &proxy, &mut ipc_rx).await;
    let _ = proxy.send_event(WebviewCommand::Shutdown);
    result
}

async fn run_inner(
    api_base_url: &str,
    token: &str,
    proxy: &EventLoopProxy<WebviewCommand>,
    ipc_rx: &mut mpsc::UnboundedReceiver<WebviewEvent>,
) -> Result<()> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!("connecting webview pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .context("timed out connecting to pair websocket")?
        .context("failed to connect to pair websocket")?;
    info!("webview pair websocket established");

    let mut audio_settings = AudioSettings::default();
    send_client_state(&mut ws, audio_settings).await?;
    let mut heartbeat = interval(Duration::from_secs(1));
    heartbeat.tick().await;

    let mut current_item: Option<CurrentItem> = None;
    let mut initial_sync = InitialYoutubeSync::new();

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
                if let Err(err) =
                    handle_webview_event(&mut ws, event, current_item.as_ref()).await
                {
                    warn!(error = %err, "failed to forward webview event");
                }
            }
            inbound = ws.next() => {
                let Some(inbound) = inbound else { break; };
                match inbound? {
                    Message::Text(text) => {
                        let result = handle_server_text(
                            text.as_str(),
                            proxy,
                            &mut audio_settings,
                            &mut initial_sync,
                        ).await;
                        if result.send_client_state {
                            send_client_state(&mut ws, audio_settings).await?;
                        }
                        if let Some(item) = result.current_item {
                            current_item = Some(item);
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

    Ok(())
}

#[derive(Default)]
struct ServerTextResult {
    current_item: Option<CurrentItem>,
    send_client_state: bool,
}

#[derive(Debug, Clone)]
struct CurrentItem {
    item_id: String,
    video_id: String,
}

struct InitialYoutubeSync {
    state: InitialYoutubeSyncState,
}

enum InitialYoutubeSyncState {
    WaitingForSnapshot {
        buffered_load: Option<PendingLoadVideo>,
    },
    Ready {
        current: Option<InitialSyncItem>,
    },
    Consumed,
}

#[derive(Debug, Clone)]
struct InitialSyncItem {
    item_id: String,
    video_id: String,
    started_at_ms: i64,
    duration_ms: Option<i64>,
    is_stream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingLoadVideo {
    item_id: String,
    video_id: String,
    is_stream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadVideoCommand {
    item_id: String,
    video_id: String,
    is_stream: bool,
    start_seconds: Option<u64>,
}

enum LoadVideoDecision {
    Dispatch(LoadVideoCommand),
    Buffered,
}

impl InitialYoutubeSync {
    fn new() -> Self {
        Self {
            state: InitialYoutubeSyncState::WaitingForSnapshot {
                buffered_load: None,
            },
        }
    }

    fn observe_queue_update(
        &mut self,
        current: Option<QueueItemSnapshot>,
    ) -> Option<LoadVideoCommand> {
        let now_ms = unix_epoch_ms().unwrap_or_default();
        self.observe_queue_update_at(current, now_ms)
    }

    fn observe_queue_update_at(
        &mut self,
        current: Option<QueueItemSnapshot>,
        now_ms: i64,
    ) -> Option<LoadVideoCommand> {
        let current = current.and_then(InitialSyncItem::from_snapshot);
        match std::mem::replace(&mut self.state, InitialYoutubeSyncState::Consumed) {
            InitialYoutubeSyncState::WaitingForSnapshot { buffered_load } => {
                if let Some(load) = buffered_load {
                    Some(command_for_load(load, current.as_ref(), now_ms))
                } else if current.is_some() {
                    self.state = InitialYoutubeSyncState::Ready { current };
                    None
                } else {
                    None
                }
            }
            state @ (InitialYoutubeSyncState::Ready { .. } | InitialYoutubeSyncState::Consumed) => {
                self.state = state;
                None
            }
        }
    }

    fn handle_load(
        &mut self,
        item_id: String,
        video_id: String,
        is_stream: bool,
    ) -> LoadVideoDecision {
        let now_ms = unix_epoch_ms();
        self.handle_load_at(item_id, video_id, is_stream, now_ms)
    }

    fn handle_load_at(
        &mut self,
        item_id: String,
        video_id: String,
        is_stream: bool,
        now_ms: Option<i64>,
    ) -> LoadVideoDecision {
        let load = PendingLoadVideo {
            item_id,
            video_id,
            is_stream,
        };

        match std::mem::replace(&mut self.state, InitialYoutubeSyncState::Consumed) {
            InitialYoutubeSyncState::WaitingForSnapshot { .. } => {
                self.state = InitialYoutubeSyncState::WaitingForSnapshot {
                    buffered_load: Some(load),
                };
                LoadVideoDecision::Buffered
            }
            InitialYoutubeSyncState::Ready { current } => {
                let command = match now_ms {
                    Some(now_ms) => command_for_load(load, current.as_ref(), now_ms),
                    None => load.into_command(None),
                };
                LoadVideoDecision::Dispatch(command)
            }
            InitialYoutubeSyncState::Consumed => {
                LoadVideoDecision::Dispatch(load.into_command(None))
            }
        }
    }
}

impl InitialSyncItem {
    fn from_snapshot(snapshot: QueueItemSnapshot) -> Option<Self> {
        Some(Self {
            item_id: snapshot.id,
            video_id: snapshot.video_id,
            started_at_ms: snapshot.started_at_ms?,
            duration_ms: snapshot.duration_ms,
            is_stream: snapshot.is_stream,
        })
    }
}

impl PendingLoadVideo {
    fn into_command(self, start_seconds: Option<u64>) -> LoadVideoCommand {
        LoadVideoCommand {
            item_id: self.item_id,
            video_id: self.video_id,
            is_stream: self.is_stream,
            start_seconds,
        }
    }
}

fn command_for_load(
    load: PendingLoadVideo,
    current: Option<&InitialSyncItem>,
    now_ms: i64,
) -> LoadVideoCommand {
    let start_seconds = current.and_then(|current| start_seconds_for_load(&load, current, now_ms));
    load.into_command(start_seconds)
}

fn start_seconds_for_load(
    load: &PendingLoadVideo,
    current: &InitialSyncItem,
    now_ms: i64,
) -> Option<u64> {
    if current.item_id != load.item_id || current.video_id != load.video_id {
        return None;
    }
    if load.is_stream || current.is_stream {
        return None;
    }
    let mut elapsed_ms = now_ms.checked_sub(current.started_at_ms)?;
    if elapsed_ms <= 0 {
        return None;
    }
    if let Some(duration_ms) = current.duration_ms.filter(|duration| *duration > 0) {
        elapsed_ms = elapsed_ms.min(duration_ms.saturating_sub(1_000));
    }
    let start_seconds = (elapsed_ms / 1_000) as u64;
    (start_seconds > 0).then_some(start_seconds)
}

async fn handle_server_text(
    text: &str,
    proxy: &EventLoopProxy<WebviewCommand>,
    audio_settings: &mut AudioSettings,
    initial_sync: &mut InitialYoutubeSync,
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
        } => match initial_sync.handle_load(item_id, video_id, is_stream) {
            LoadVideoDecision::Dispatch(command) => dispatch_load_video(proxy, command),
            LoadVideoDecision::Buffered => ServerTextResult::default(),
        },
        ServerMessage::SourceChanged { audio_mode } => {
            debug!(%audio_mode, "dispatching source_changed to embedded webview");
            if let Err(err) = proxy.send_event(WebviewCommand::SourceChanged { audio_mode }) {
                warn!(error = %err, "event loop closed while sending source_changed");
            }
            ServerTextResult::default()
        }
        ServerMessage::QueueUpdate { current } => {
            if let Some(command) = initial_sync.observe_queue_update(current) {
                dispatch_load_video(proxy, command)
            } else {
                ServerTextResult::default()
            }
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

fn dispatch_load_video(
    proxy: &EventLoopProxy<WebviewCommand>,
    command: LoadVideoCommand,
) -> ServerTextResult {
    let LoadVideoCommand {
        item_id,
        video_id,
        is_stream,
        start_seconds,
    } = command;
    debug!(
        %item_id,
        %video_id,
        is_stream,
        ?start_seconds,
        "dispatching load_video to embedded webview"
    );
    let current_item = CurrentItem {
        item_id: item_id.clone(),
        video_id: video_id.clone(),
    };
    if let Err(err) = proxy.send_event(WebviewCommand::LoadVideo {
        item_id,
        video_id,
        is_stream,
        start_seconds,
    }) {
        warn!(error = %err, "event loop closed while sending load_video");
        return ServerTextResult::default();
    }
    ServerTextResult {
        current_item: Some(current_item),
        ..ServerTextResult::default()
    }
}

fn unix_epoch_ms() -> Option<i64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    i64::try_from(millis).ok()
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
    current_item: Option<&CurrentItem>,
) -> Result<()> {
    let payload = match event {
        WebviewEvent::State {
            item_id,
            state,
            position_ms,
            duration_ms,
            autoplay_blocked,
        } => {
            let resolved = item_id.or_else(|| current_item.map(|item| item.item_id.clone()));
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
        WebviewEvent::Error {
            item_id,
            video_id,
            code,
        } => {
            let resolved = item_id.or_else(|| current_item.map(|item| item.item_id.clone()));
            let resolved_video_id =
                video_id.or_else(|| current_item.map(|item| item.video_id.clone()));
            warn!(
                item_id = ?resolved,
                video_id = ?resolved_video_id,
                error_code = %code,
                "embedded YouTube player reported playback error"
            );
            if is_embed_rejection(&code) {
                warn!(
                    item_id = ?resolved,
                    video_id = ?resolved_video_id,
                    error_code = %code,
                    "embedded YouTube playback rejected; staying on controlled helper page"
                );
            }
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
            let resolved = item_id.or_else(|| current_item.map(|item| item.item_id.clone()));
            warn!(
                item_id = ?resolved,
                "embedded YouTube player appears autoplay-blocked"
            );
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

fn is_embed_rejection(code: &str) -> bool {
    matches!(code, "101" | "150" | "153")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: &str, video_id: &str, started_at_ms: Option<i64>) -> QueueItemSnapshot {
        QueueItemSnapshot {
            id: id.to_string(),
            video_id: video_id.to_string(),
            started_at_ms,
            duration_ms: Some(180_000),
            is_stream: false,
        }
    }

    fn load(sync: &mut InitialYoutubeSync, item_id: &str, video_id: &str) -> LoadVideoDecision {
        sync.handle_load_at(
            item_id.to_string(),
            video_id.to_string(),
            false,
            Some(25_500),
        )
    }

    fn dispatched(decision: LoadVideoDecision) -> LoadVideoCommand {
        match decision {
            LoadVideoDecision::Dispatch(command) => command,
            LoadVideoDecision::Buffered => panic!("expected load_video dispatch"),
        }
    }

    fn assert_buffered(decision: LoadVideoDecision) {
        match decision {
            LoadVideoDecision::Buffered => {}
            LoadVideoDecision::Dispatch(command) => {
                panic!("expected buffered load_video, got {command:?}")
            }
        }
    }

    #[test]
    fn initial_sync_uses_snapshot_once_for_first_matching_load() {
        let mut sync = InitialYoutubeSync::new();
        assert!(
            sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_500)
                .is_none()
        );

        assert_eq!(
            dispatched(load(&mut sync, "item-1", "video-1")),
            LoadVideoCommand {
                item_id: "item-1".to_string(),
                video_id: "video-1".to_string(),
                is_stream: false,
                start_seconds: Some(15),
            }
        );
        assert_eq!(
            dispatched(load(&mut sync, "item-1", "video-1")),
            LoadVideoCommand {
                item_id: "item-1".to_string(),
                video_id: "video-1".to_string(),
                is_stream: false,
                start_seconds: None,
            }
        );
    }

    #[test]
    fn initial_sync_buffers_load_until_snapshot_arrives() {
        let mut sync = InitialYoutubeSync::new();
        assert_buffered(load(&mut sync, "item-1", "video-1"));

        assert_eq!(
            sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_500)
                .unwrap(),
            LoadVideoCommand {
                item_id: "item-1".to_string(),
                video_id: "video-1".to_string(),
                is_stream: false,
                start_seconds: Some(15),
            }
        );
    }

    #[test]
    fn initial_sync_does_not_arm_later_track_switches() {
        let mut sync = InitialYoutubeSync::new();
        sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_000);

        assert_eq!(
            dispatched(sync.handle_load_at(
                "item-1".to_string(),
                "video-1".to_string(),
                false,
                Some(25_000),
            ))
            .start_seconds,
            Some(15)
        );

        sync.observe_queue_update_at(Some(snapshot("item-2", "video-2", Some(30_000))), 45_000);
        assert_eq!(
            dispatched(sync.handle_load_at(
                "item-2".to_string(),
                "video-2".to_string(),
                false,
                Some(45_000),
            ))
            .start_seconds,
            None
        );
    }

    #[test]
    fn initial_sync_dispatches_buffered_load_without_seek_if_it_does_not_match_snapshot() {
        let mut sync = InitialYoutubeSync::new();
        assert_buffered(sync.handle_load_at(
            "fallback".to_string(),
            "fallback-video".to_string(),
            true,
            Some(25_000),
        ));

        assert_eq!(
            sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 25_000)
                .unwrap(),
            LoadVideoCommand {
                item_id: "fallback".to_string(),
                video_id: "fallback-video".to_string(),
                is_stream: true,
                start_seconds: None,
            }
        );
    }

    #[test]
    fn initial_sync_disables_when_initial_snapshot_has_no_current_track() {
        let mut sync = InitialYoutubeSync::new();
        assert!(sync.observe_queue_update_at(None, 20_000).is_none());
        sync.observe_queue_update_at(Some(snapshot("item-1", "video-1", Some(10_000))), 30_000);

        assert_eq!(
            dispatched(sync.handle_load_at(
                "item-1".to_string(),
                "video-1".to_string(),
                false,
                Some(30_000),
            ))
            .start_seconds,
            None
        );
    }
}
