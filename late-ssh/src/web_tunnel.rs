use anyhow::{Context, Result};
use axum::{
    extract::{
        ConnectInfo, Query, State as AxumState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use late_core::{
    MutexRecover,
    models::{
        server_ban::ServerBan,
        user::{User, UserParams},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex as TokioMutex, Notify, OwnedSemaphorePermit, mpsc},
    time::{MissedTickBehavior, timeout},
};

use crate::{
    app::state::App,
    metrics,
    session_bootstrap::{SessionBootstrapInputs, build_session_config},
    state::{ActiveSession, ActiveUser, ActivityEvent, State},
};

const INPUT_QUEUE_CAP: usize = 256;
const WS_OUT_BUFFER: usize = 8;
const WORLD_TICK_INTERVAL: Duration = Duration::from_millis(66);
const MIN_RENDER_GAP: Duration = Duration::from_millis(15);
const EXIT_MESSAGE: &str = "\r\nStay late. Code safe. ✨\r\n";

static FRAME_DROP_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
pub struct TunnelParams {
    token: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t")]
enum ControlFrame {
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
}

enum InputEvent {
    Bytes(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

struct RenderSignal {
    dirty: AtomicBool,
    notify: Notify,
}

impl RenderSignal {
    fn new() -> Self {
        Self {
            dirty: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }
}

struct WebTunnelGuard {
    state: State,
    peer_ip: IpAddr,
    user_id: uuid::Uuid,
    session_token: String,
    per_ip_incremented: bool,
    active_user_incremented: bool,
    _conn_permit: OwnedSemaphorePermit,
}

struct WebTunnelSession {
    socket: WebSocket,
    state: State,
    user: User,
    is_new_user: bool,
    session_token: String,
    cols: u16,
    rows: u16,
    _guard: WebTunnelGuard,
}

impl Drop for WebTunnelGuard {
    fn drop(&mut self) {
        if self.active_user_incremented {
            metrics::add_ssh_session(-1);
            let mut active_users = self.state.active_users.lock_recover();
            if let Some(active) = active_users.get_mut(&self.user_id) {
                active
                    .sessions
                    .retain(|session| session.token != self.session_token);
                if active.connection_count <= 1 {
                    active_users.remove(&self.user_id);
                } else {
                    active.connection_count -= 1;
                }
            }
        }

        if self.per_ip_incremented {
            let mut counts = self.state.conn_counts.lock_recover();
            if let Some(count) = counts.get_mut(&self.peer_ip) {
                if *count <= 1 {
                    counts.remove(&self.peer_ip);
                } else {
                    *count -= 1;
                }
            }
        }
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<TunnelParams>,
    AxumState(state): AxumState<State>,
    headers: HeaderMap,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
) -> axum::response::Response {
    let peer_ip = effective_client_ip(&headers, peer_addr, &state);
    if !state.ws_pair_limiter.allow(peer_ip) {
        tracing::warn!(peer_ip = %peer_ip, "web tunnel rate limit exceeded");
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    let expected_token = state.config.web_tunnel.token.as_str();
    let presented_token = match params.token.as_deref() {
        Some(token) if constant_time_eq(token.as_bytes(), expected_token.as_bytes()) => token,
        _ => {
            tracing::warn!(peer_ip = %peer_ip, "web tunnel rejected: bad token");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let token_hint = token_hint(presented_token);

    let permit = match state.conn_limit.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    {
        let mut counts = state.conn_counts.lock_recover();
        let count = counts.entry(peer_ip).or_insert(0);
        if *count >= state.config.max_conns_per_ip {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        *count += 1;
    }

    let (user, is_new_user) = match ensure_web_tunnel_user(&state, peer_ip).await {
        Ok(pair) => pair,
        Err(err) => {
            decrement_ip_count(&state, peer_ip);
            tracing::warn!(peer_ip = %peer_ip, error = ?err, "web tunnel admission failed");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let session_token = crate::session::new_session_token();
    let mut guard = WebTunnelGuard {
        state: state.clone(),
        peer_ip,
        user_id: user.id,
        session_token: session_token.clone(),
        per_ip_incremented: true,
        active_user_incremented: false,
        _conn_permit: permit,
    };
    track_active_user(&state, &user, peer_ip, &session_token);
    guard.active_user_incremented = true;

    let _ = state.activity_feed.send(ActivityEvent {
        username: user.username.clone(),
        action: "joined".to_string(),
        at: Instant::now(),
    });

    tracing::info!(
        peer_ip = %peer_ip,
        username = %user.username,
        token_hint = %token_hint,
        "web tunnel accepted"
    );

    let cols = params.cols.unwrap_or(120).clamp(20, 240);
    let rows = params.rows.unwrap_or(36).clamp(10, 80);
    ws.on_upgrade(move |socket| {
        handle_socket(WebTunnelSession {
            socket,
            state,
            user,
            is_new_user,
            session_token,
            cols,
            rows,
            _guard: guard,
        })
    })
}

async fn handle_socket(session: WebTunnelSession) {
    let WebTunnelSession {
        socket,
        state,
        user,
        is_new_user,
        session_token,
        cols,
        rows,
        _guard,
    } = session;

    let session_config = build_session_config(
        &state,
        SessionBootstrapInputs {
            user,
            is_new_user,
            cols,
            rows,
            session_token,
            session_rx: None,
            activity_feed_rx: Some(state.activity_feed.subscribe()),
        },
    )
    .await;

    let app = match App::new(session_config) {
        Ok(app) => Arc::new(TokioMutex::new(app)),
        Err(err) => {
            tracing::error!(error = ?err, "failed to initialize web tunnel app");
            return;
        }
    };

    let (mut ws_sink, mut ws_stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(WS_OUT_BUFFER);
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let was_close = matches!(msg, Message::Close(_));
            if let Err(err) = ws_sink.send(msg).await {
                tracing::debug!(error = ?err, "web tunnel send failed");
                break;
            }
            if was_close {
                break;
            }
        }
        let _ = ws_sink.close().await;
    });

    let _ = out_tx
        .send(Message::Binary(App::enter_alt_screen().into()))
        .await;

    let (input_tx, input_rx) = mpsc::channel(INPUT_QUEUE_CAP);
    let signal = Arc::new(RenderSignal::new());
    signal.dirty.store(true, Ordering::Release);
    signal.notify.notify_one();
    let render = tokio::spawn(run_render_loop(
        Arc::clone(&app),
        input_rx,
        out_tx.clone(),
        state.config.frame_drop_log_every,
        Arc::clone(&signal),
    ));

    while let Some(msg) = ws_stream.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                tracing::debug!(error = ?err, "web tunnel recv failed");
                break;
            }
        };

        match msg {
            Message::Binary(bytes) => {
                if let Some(event) = readonly_input_event(&bytes) {
                    enqueue_input(&input_tx, &signal, event, "bytes");
                }
            }
            Message::Text(text) => match serde_json::from_str::<ControlFrame>(&text) {
                Ok(ControlFrame::Resize { cols, rows }) => enqueue_input(
                    &input_tx,
                    &signal,
                    InputEvent::Resize { cols, rows },
                    "resize",
                ),
                Err(err) => {
                    tracing::warn!(error = ?err, "web tunnel bad control frame");
                    let _ = out_tx.send(Message::Close(None)).await;
                    break;
                }
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    {
        let mut app = app.lock().await;
        app.running = false;
    }
    signal.notify.notify_one();
    let _ = render.await;
    let _ = writer.await;
}

fn readonly_input_event(data: &[u8]) -> Option<InputEvent> {
    match data {
        [b'1'..=b'5'] | [b'\t'] | [0x1b] => Some(InputEvent::Bytes(data.to_vec())),
        b"\x1b[Z" => Some(InputEvent::Bytes(data.to_vec())),
        _ => None,
    }
}

fn enqueue_input(
    input_tx: &mpsc::Sender<InputEvent>,
    signal: &RenderSignal,
    event: InputEvent,
    label: &'static str,
) {
    match input_tx.try_reserve() {
        Ok(permit) => {
            permit.send(event);
            signal.dirty.store(true, Ordering::Release);
            signal.notify.notify_one();
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!(
                queue_cap = INPUT_QUEUE_CAP,
                label,
                "web tunnel input queue full"
            );
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            tracing::debug!(label, "web tunnel input queue closed");
        }
    }
}

async fn run_render_loop(
    app: Arc<TokioMutex<App>>,
    mut input_rx: mpsc::Receiver<InputEvent>,
    out_tx: mpsc::Sender<Message>,
    frame_drop_log_every: u64,
    signal: Arc<RenderSignal>,
) {
    let mut world_tick = tokio::time::interval(WORLD_TICK_INTERVAL);
    world_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut previous_render: Option<Instant> = None;
    let mut input_pending = false;

    loop {
        let advance_world = match next_render_action(
            &mut world_tick,
            &signal,
            &mut input_pending,
            previous_render,
        )
        .await
        {
            RenderAction::AdvanceWorld => true,
            RenderAction::Render => false,
            RenderAction::Skip => continue,
        };

        match render_once(
            &app,
            &mut input_rx,
            &out_tx,
            frame_drop_log_every,
            advance_world,
            &signal,
        )
        .await
        {
            Ok(should_quit) => {
                previous_render = Some(Instant::now());
                if should_quit {
                    clean_disconnect(&out_tx).await;
                    break;
                }
            }
            Err(err) => {
                tracing::debug!(error = ?err, "web tunnel render failed");
                let _ = out_tx
                    .send(Message::Binary(App::leave_alt_screen().into()))
                    .await;
                let _ = out_tx.send(Message::Close(None)).await;
                break;
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RenderAction {
    AdvanceWorld,
    Render,
    Skip,
}

async fn next_render_action(
    world_tick: &mut tokio::time::Interval,
    signal: &RenderSignal,
    input_pending: &mut bool,
    previous_render: Option<Instant>,
) -> RenderAction {
    tokio::select! {
        biased;
        _ = world_tick.tick() => {
            *input_pending = false;
            RenderAction::AdvanceWorld
        }
        _ = tokio::time::sleep_until(
            previous_render
                .map(|t| t + MIN_RENDER_GAP)
                .unwrap_or_else(Instant::now)
                .into(),
        ), if *input_pending => {
            *input_pending = false;
            RenderAction::Render
        }
        _ = signal.notify.notified(), if !*input_pending => {
            if signal.dirty.load(Ordering::Acquire) {
                *input_pending = true;
            }
            RenderAction::Skip
        }
    }
}

async fn render_once(
    app: &Arc<TokioMutex<App>>,
    input_rx: &mut mpsc::Receiver<InputEvent>,
    out_tx: &mpsc::Sender<Message>,
    frame_drop_log_every: u64,
    advance_world: bool,
    signal: &RenderSignal,
) -> Result<bool> {
    let (frame, terminal_commands) = {
        let mut app = app.lock().await;
        if !app.running {
            return Ok(true);
        }
        signal.dirty.store(false, Ordering::Release);
        while let Ok(event) = input_rx.try_recv() {
            match event {
                InputEvent::Bytes(data) => app.handle_input(&data),
                InputEvent::Resize { cols, rows } => {
                    if let Err(err) = app.resize(cols, rows) {
                        tracing::warn!(error = ?err, cols, rows, "web tunnel resize failed");
                    }
                }
            }
            if !app.running {
                return Ok(true);
            }
        }
        if advance_world {
            app.tick();
        }
        let frame = app.render().context("rendering frame")?;
        let terminal_commands = std::mem::take(&mut app.pending_terminal_commands);
        (frame, terminal_commands)
    };

    if !send_frame(out_tx, frame).await? {
        let mut app = app.lock().await;
        app.force_full_repaint();
        if !signal.dirty.swap(true, Ordering::AcqRel) {
            signal.notify.notify_one();
        }
    }

    for command in terminal_commands {
        if !send_frame(out_tx, command).await? {
            let drops = FRAME_DROP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            metrics::record_render_frame_drop();
            if drops.is_multiple_of(frame_drop_log_every) {
                tracing::debug!(drops, "web tunnel frame drops");
            }
        }
    }

    Ok(false)
}

async fn send_frame(out_tx: &mpsc::Sender<Message>, frame: Vec<u8>) -> Result<bool> {
    match timeout(
        Duration::from_millis(50),
        out_tx.send(Message::Binary(frame.into())),
    )
    .await
    {
        Ok(Ok(())) => Ok(true),
        Ok(Err(_)) => Err(anyhow::anyhow!("web tunnel output channel closed")),
        Err(_) => Ok(false),
    }
}

async fn clean_disconnect(out_tx: &mpsc::Sender<Message>) {
    let _ = out_tx
        .send(Message::Binary(App::leave_alt_screen().into()))
        .await;
    let _ = out_tx
        .send(Message::Binary(EXIT_MESSAGE.as_bytes().to_vec().into()))
        .await;
    let _ = out_tx.send(Message::Close(None)).await;
}

async fn ensure_web_tunnel_user(state: &State, peer_ip: IpAddr) -> Result<(User, bool)> {
    let fingerprint = &state.config.web_tunnel.fingerprint;
    let client = state.db.get().await?;

    if ServerBan::find_active_for_fingerprint(&client, fingerprint)
        .await?
        .is_some()
        || ServerBan::find_active_for_ip_address(&client, &peer_ip.to_string())
            .await?
            .is_some()
    {
        anyhow::bail!("active server ban");
    }

    if let Some(mut user) = User::find_by_fingerprint(&client, fingerprint).await? {
        if ServerBan::find_active_for_user_id(&client, user.id)
            .await?
            .is_some()
        {
            anyhow::bail!("active server ban");
        }
        if let Err(err) = User::update_last_seen(&mut user, &client).await {
            tracing::warn!(error = ?err, "failed to update web tunnel user last_seen");
        }
        return Ok((user, false));
    }

    let username =
        User::next_available_username(&client, &state.config.web_tunnel.username).await?;
    let user = User::create(
        &client,
        UserParams {
            fingerprint: fingerprint.clone(),
            username,
            settings: serde_json::json!({}),
        },
    )
    .await?;
    if let Err(err) = state.chat_service.auto_join_public_rooms(user.id).await {
        tracing::warn!(user_id = %user.id, error = ?err, "failed to seed web tunnel chat rooms");
    }
    Ok((user, true))
}

fn track_active_user(state: &State, user: &User, peer_ip: IpAddr, session_token: &str) {
    let mut active_users = state.active_users.lock_recover();
    let session = ActiveSession {
        token: session_token.to_string(),
        fingerprint: Some(user.fingerprint.clone()),
        peer_ip: Some(peer_ip),
    };

    if let Some(active) = active_users.get_mut(&user.id) {
        active.connection_count += 1;
        active.username = user.username.clone();
        active.fingerprint = Some(user.fingerprint.clone());
        active.peer_ip = Some(peer_ip);
        active.last_login_at = Instant::now();
        active.sessions.push(session);
    } else {
        active_users.insert(
            user.id,
            ActiveUser {
                username: user.username.clone(),
                fingerprint: Some(user.fingerprint.clone()),
                peer_ip: Some(peer_ip),
                sessions: vec![session],
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
    }
    metrics::add_ssh_session(1);
}

fn decrement_ip_count(state: &State, peer_ip: IpAddr) {
    let mut counts = state.conn_counts.lock_recover();
    if let Some(count) = counts.get_mut(&peer_ip) {
        if *count <= 1 {
            counts.remove(&peer_ip);
        } else {
            *count -= 1;
        }
    }
}

fn effective_client_ip(headers: &HeaderMap, peer_addr: SocketAddr, state: &State) -> IpAddr {
    if state
        .config
        .ssh_proxy_trusted_cidrs
        .iter()
        .any(|cidr| cidr.contains(&peer_addr.ip()))
        && let Some(ip) = forwarded_for_ip(headers)
    {
        return ip;
    }
    peer_addr.ip()
}

fn forwarded_for_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = value.split(',').next()?.trim();
    first.parse().ok()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_resize_parses() {
        let parsed: ControlFrame =
            serde_json::from_str(r#"{"t":"resize","cols":120,"rows":40}"#).unwrap();
        assert_eq!(
            parsed,
            ControlFrame::Resize {
                cols: 120,
                rows: 40
            }
        );
    }

    #[test]
    fn constant_time_eq_basic_cases() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn readonly_input_allows_only_page_navigation() {
        for input in [b"1".as_slice(), b"5", b"\t", b"\x1b", b"\x1b[Z"] {
            assert!(matches!(
                readonly_input_event(input),
                Some(InputEvent::Bytes(_))
            ));
        }
    }

    #[test]
    fn readonly_input_blocks_mutating_and_mouse_input() {
        for input in [
            b"q".as_slice(),
            b"\r",
            b"hello",
            b"\x1b[A",
            b"\x1b[<0;10;10M",
            b"\x1b[200~paste\x1b[201~",
        ] {
            assert!(readonly_input_event(input).is_none());
        }
    }
}
