//! Pair-WS relay task used by the `late-webview` helper binary (Linux) and
//! the in-process `late webview-pair` subcommand (Windows/macOS).
//!
//! Connects to /api/ws/pair?token=..., registers as `client_kind = "browser"`
//! with `ssh_mode = "webview"`, relays inbound `load_video` / `source_changed`
//! server messages to the webview, and forwards `player_state` events back to
//! the server.
//!
//! The relay reconnects on WebSocket drops instead of exiting: the helper
//! process (and with it the window position and mute/volume state) must
//! survive server redeploys and network blips. Mute/volume are seeded from
//! `LATE_WEBVIEW_INITIAL_MUTED` / `LATE_WEBVIEW_INITIAL_VOLUME`, set by the
//! parent CLI at spawn, so a respawned helper inherits the session's current
//! state instead of booting unmuted.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tao::event_loop::EventLoopProxy;
use tokio::{sync::mpsc, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::commands::{WebviewCommand, WebviewEvent};
use crate::client_platform_label;

/// Tag the webview sends on the wire. Server-side still treats the helper as a
/// browser, but distinguishes it from a real browser through `ssh_mode`.
const CLIENT_KIND: &str = "browser";
const DEFAULT_VOLUME_PERCENT: u8 = 30;

/// Reconnect policy, mirroring the parent CLI's pair-WS loop: retry with a
/// short delay, give up after this many consecutive failures. A connection
/// that stayed up for `STABLE_CONNECTION` resets the counter so one long
/// session's worth of rare drops never accumulates into a give-up.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const MAX_CONSECUTIVE_FAILURES: u32 = 10;
const STABLE_CONNECTION: Duration = Duration::from_secs(60);

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

#[derive(Debug, Deserialize, Clone)]
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

impl AudioSettings {
    /// Seed mute/volume from the env the parent CLI sets at helper spawn, so
    /// a respawned helper inherits the session's current state. Absent or
    /// invalid values keep the defaults (old parents, spike mode).
    fn from_env() -> Self {
        initial_audio_settings(
            std::env::var("LATE_WEBVIEW_INITIAL_MUTED").ok().as_deref(),
            std::env::var("LATE_WEBVIEW_INITIAL_VOLUME").ok().as_deref(),
        )
    }
}

fn initial_audio_settings(muted: Option<&str>, volume_percent: Option<&str>) -> AudioSettings {
    let defaults = AudioSettings::default();
    AudioSettings {
        muted: match muted {
            Some("1") => true,
            Some("0") => false,
            _ => defaults.muted,
        },
        volume_percent: volume_percent
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|value| *value <= 100)
            .unwrap_or(defaults.volume_percent),
    }
}

/// How one relay session over an established (or attempted) connection ended.
enum SessionEnd {
    /// The server closed the socket cleanly; reconnect.
    ServerClosed,
    /// The webview event loop side is gone; exit for good.
    IpcClosed,
}

pub async fn run(
    api_base_url: &str,
    token: &str,
    proxy: EventLoopProxy<WebviewCommand>,
    mut ipc_rx: mpsc::UnboundedReceiver<WebviewEvent>,
) -> Result<()> {
    let mut audio_settings = AudioSettings::from_env();
    let mut consecutive_failures: u32 = 0;
    let result = loop {
        let attempt_started = Instant::now();
        match run_session(
            api_base_url,
            token,
            &proxy,
            &mut ipc_rx,
            &mut audio_settings,
        )
        .await
        {
            Ok(SessionEnd::IpcClosed) => break Ok(()),
            Ok(SessionEnd::ServerClosed) => {
                info!("server closed webview pair websocket; reconnecting");
            }
            Err(err) => {
                warn!(error = %err, "webview pair session failed; reconnecting");
            }
        }
        if attempt_started.elapsed() >= STABLE_CONNECTION {
            consecutive_failures = 0;
        }
        consecutive_failures += 1;
        if consecutive_failures > MAX_CONSECUTIVE_FAILURES {
            break Err(anyhow::anyhow!(
                "webview pair websocket failed {MAX_CONSECUTIVE_FAILURES} consecutive times; giving up"
            ));
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    };
    let _ = proxy.send_event(WebviewCommand::Shutdown);
    result
}

async fn run_session(
    api_base_url: &str,
    token: &str,
    proxy: &EventLoopProxy<WebviewCommand>,
    ipc_rx: &mut mpsc::UnboundedReceiver<WebviewEvent>,
    audio_settings: &mut AudioSettings,
) -> Result<SessionEnd> {
    let ws_url = pair_ws_url(api_base_url, token)?;
    debug!("connecting webview pair websocket");
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(&ws_url))
        .await
        .context("timed out connecting to pair websocket")?
        .context("failed to connect to pair websocket")?;
    info!("webview pair websocket established");

    send_client_state(&mut ws, *audio_settings).await?;
    let mut heartbeat = interval(Duration::from_secs(1));
    heartbeat.tick().await;

    let mut current_item: Option<CurrentItem> = None;
    // Per-connection: the server resends its catch-up burst (queue_update +
    // load_video) on reconnect, so the one-shot live-position seek applies to
    // the first load of every connection, not just the process's first.
    let mut initial_sync = InitialYoutubeSync::new();
    // Latest server snapshot for the playing item. Unlike the one-shot initial
    // sync, this is kept current across track changes so unmute can resume at
    // the live server position (started_at_ms is the single source of truth).
    let mut current_snapshot: Option<InitialSyncItem> = None;

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
                    return Ok(SessionEnd::IpcClosed);
                };
                if matches!(event, WebviewEvent::Ready) {
                    // The page just finished loading. Seed it with the
                    // session's current mute/volume before the first
                    // load_video plays, so a muted session's respawned
                    // helper never blasts audio from a fresh page.
                    send_audio_settings(proxy, *audio_settings);
                }
                if let Err(err) =
                    handle_webview_event(&mut ws, event, current_item.as_ref()).await
                {
                    warn!(error = %err, "failed to forward webview event");
                }
            }
            inbound = ws.next() => {
                let Some(inbound) = inbound else { return Ok(SessionEnd::ServerClosed); };
                match inbound? {
                    Message::Text(text) => {
                        let result = handle_server_text(
                            text.as_str(),
                            proxy,
                            audio_settings,
                            &mut initial_sync,
                            &mut current_snapshot,
                            current_item.as_ref(),
                        );
                        if result.send_client_state {
                            send_client_state(&mut ws, *audio_settings).await?;
                        }
                        if let Some(item) = result.current_item {
                            current_item = Some(item);
                        }
                    }
                    Message::Close(_) => {
                        return Ok(SessionEnd::ServerClosed);
                    }
                    _ => {}
                }
            }
        }
    }
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

fn client_state_only() -> ServerTextResult {
    ServerTextResult {
        send_client_state: true,
        ..ServerTextResult::default()
    }
}

/// On unmute, re-load the current track at its live server position and still
/// flag a client-state update for the new mute flag.
fn unmute_resume_result(
    proxy: &EventLoopProxy<WebviewCommand>,
    current_item: Option<&CurrentItem>,
    current_snapshot: Option<&InitialSyncItem>,
) -> ServerTextResult {
    let mut result = client_state_only();
    if let Some(command) = resume_command(current_item, current_snapshot) {
        result.current_item = dispatch_load_video(proxy, command).current_item;
    }
    result
}

/// Build the `load_video` used to resume the current track on unmute, seeking
/// to the live server position derived from `started_at_ms`. Falls back to a
/// from-start load when no matching server snapshot is available (fallback
/// stream, missing `started_at_ms`, or a snapshot for a different item).
fn resume_command(
    current_item: Option<&CurrentItem>,
    snapshot: Option<&InitialSyncItem>,
) -> Option<LoadVideoCommand> {
    resume_command_at(current_item, snapshot, unix_epoch_ms())
}

fn resume_command_at(
    current_item: Option<&CurrentItem>,
    snapshot: Option<&InitialSyncItem>,
    now_ms: Option<i64>,
) -> Option<LoadVideoCommand> {
    let item = current_item?;
    let matched =
        snapshot.filter(|snap| snap.item_id == item.item_id && snap.video_id == item.video_id);
    let load = PendingLoadVideo {
        item_id: item.item_id.clone(),
        video_id: item.video_id.clone(),
        is_stream: matched.map(|snap| snap.is_stream).unwrap_or(false),
    };
    let start_seconds = matched
        .zip(now_ms)
        .and_then(|(snap, now_ms)| start_seconds_for_load(&load, snap, now_ms));
    Some(load.into_command(start_seconds))
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

fn handle_server_text(
    text: &str,
    proxy: &EventLoopProxy<WebviewCommand>,
    audio_settings: &mut AudioSettings,
    initial_sync: &mut InitialYoutubeSync,
    current_snapshot: &mut Option<InitialSyncItem>,
    current_item: Option<&CurrentItem>,
) -> ServerTextResult {
    let Ok(message) = serde_json::from_str::<ServerMessage>(text) else {
        debug!(payload = %text, "ignoring unrecognized pair ws message");
        return ServerTextResult::default();
    };
    match message {
        ServerMessage::ToggleMute => {
            let was_muted = audio_settings.muted;
            audio_settings.muted = !was_muted;
            send_audio_settings(proxy, *audio_settings);
            // The webview stops downloading while muted, so a mute→unmute
            // transition must re-load the current track at its live server
            // position rather than leaving it silent until the next 10s
            // heartbeat (which loads from 0).
            if was_muted {
                unmute_resume_result(proxy, current_item, current_snapshot.as_ref())
            } else {
                client_state_only()
            }
        }
        ServerMessage::VolumeUp => {
            let was_muted = audio_settings.muted;
            audio_settings.volume_percent = bump_volume(audio_settings.volume_percent, 5);
            audio_settings.muted = false;
            send_audio_settings(proxy, *audio_settings);
            // Volume-up also clears mute, so resume playback the same way.
            if was_muted {
                unmute_resume_result(proxy, current_item, current_snapshot.as_ref())
            } else {
                client_state_only()
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
            // Keep the live snapshot fresh for every track, not just the first.
            *current_snapshot = current.clone().and_then(InitialSyncItem::from_snapshot);
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
#[path = "pair_test.rs"]
mod pair_test;
