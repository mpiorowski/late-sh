//! `/tunnel` listener — bastion-only WebSocket entry point.
//!
//! Phase 2c: bind the private listener, validate handshake (IP allowlist,
//! pre-shared secret, required headers), look up the user, build a
//! `SessionConfig`, and run the same `App::new` + `run_session` render
//! loop that the russh path uses. The transport difference is confined
//! to the `WsFrameSink` (output) and the receive loop below (input +
//! resize control frames).
//!
//! The `:4001` listener is intentionally separate from the public `:4000`
//! API listener (per `devdocs/LATE-CONNECTION-BASTION.md` §3): mixing trust
//! domains on one socket is a known footgun, and a separate bind gives
//! kernel-level isolation in addition to the in-app checks below.

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{
        ConnectInfo, State as AxumState, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use ipnet::IpNet;
use late_core::MutexRecover;
use late_core::models::user::User;
use late_core::shutdown::CancellationToken;
use late_core::telemetry::http_telemetry_middleware;
use late_core::tunnel_protocol::{
    ControlFrame, SshInputEvent, TUNNEL_CLOSE_BANNED, TUNNEL_CLOSE_PROTOCOL_ERROR,
};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::{Mutex as TokioMutex, OwnedSemaphorePermit, mpsc};
use uuid::Uuid;

use crate::metrics;
use crate::session_bootstrap::{SessionBootstrapInputs, build_session_config};
use crate::session_io::WsFrameSink;
use crate::ssh::{
    AdmissionReject, INPUT_QUEUE_CAP, RenderSignal, check_ssh_admission, ensure_user, run_session,
};
use crate::state::{ActiveSession, ActiveUser, ActivityEvent, State, TunnelSessionPermit};

/// Bound on the writer-task mpsc that feeds `WsFrameSink`. Backpressure
/// past this is surfaced to the render loop as `Ok(false)` (drop +
/// repaint), matching the russh path's per-frame send timeout.
const WS_OUT_BUFFER: usize = 8;

// Header names live in `late_core::tunnel_protocol` so the bastion and
// backend reference the same constants. Re-exported here so existing
// imports (`late_ssh::tunnel::HEADER_*`) keep working.
pub use late_core::tunnel_protocol::{
    HEADER_COLS, HEADER_FINGERPRINT, HEADER_PEER_IP, HEADER_RECONNECT_REASON, HEADER_ROWS,
    HEADER_SECRET, HEADER_SESSION_ID, HEADER_TERM, HEADER_USERNAME, HEADER_VIA,
};

/// Validated handshake. Phase 2c will hand this to the session bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelHandshake {
    pub fingerprint: String,
    pub username: String,
    pub peer_ip: IpAddr,
    pub term: String,
    pub cols: u16,
    pub rows: u16,
    pub reconnect_reason: Option<u16>,
    pub session_id: Option<String>,
}

/// Why a handshake was rejected. Maps directly onto an HTTP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeReject {
    /// Transport peer is not in the configured CIDR allowlist.
    UntrustedPeer,
    /// `X-Late-Secret` missing or did not match.
    BadSecret,
    /// A required header is missing or unparseable.
    BadHeader(&'static str),
}

impl HandshakeReject {
    pub fn status(&self) -> StatusCode {
        match self {
            HandshakeReject::UntrustedPeer => StatusCode::FORBIDDEN,
            HandshakeReject::BadSecret => StatusCode::UNAUTHORIZED,
            HandshakeReject::BadHeader(_) => StatusCode::BAD_REQUEST,
        }
    }

    pub fn log_label(&self) -> &'static str {
        match self {
            HandshakeReject::UntrustedPeer => "untrusted_peer",
            HandshakeReject::BadSecret => "bad_secret",
            HandshakeReject::BadHeader(_) => "bad_header",
        }
    }
}

/// Pure-logic handshake validation. Tested below.
pub fn validate_handshake(
    headers: &HeaderMap,
    peer_ip: IpAddr,
    trusted_cidrs: &[IpNet],
    expected_secret: &str,
) -> Result<TunnelHandshake, HandshakeReject> {
    if !trusted_cidrs.iter().any(|cidr| cidr.contains(&peer_ip)) {
        return Err(HandshakeReject::UntrustedPeer);
    }

    if expected_secret.trim().is_empty() {
        return Err(HandshakeReject::BadSecret);
    }

    let presented_secret = header_str(headers, HEADER_SECRET).ok_or(HandshakeReject::BadSecret)?;
    if !constant_time_eq(presented_secret.as_bytes(), expected_secret.as_bytes()) {
        return Err(HandshakeReject::BadSecret);
    }

    let fingerprint = header_str(headers, HEADER_FINGERPRINT)
        .ok_or(HandshakeReject::BadHeader(HEADER_FINGERPRINT))?
        .to_string();
    let username = header_str(headers, HEADER_USERNAME)
        .ok_or(HandshakeReject::BadHeader(HEADER_USERNAME))?
        .to_string();
    let peer_ip_asserted: IpAddr = header_str(headers, HEADER_PEER_IP)
        .ok_or(HandshakeReject::BadHeader(HEADER_PEER_IP))?
        .parse()
        .map_err(|_| HandshakeReject::BadHeader(HEADER_PEER_IP))?;
    let term = header_str(headers, HEADER_TERM)
        .ok_or(HandshakeReject::BadHeader(HEADER_TERM))?
        .to_string();
    let cols: u16 = header_str(headers, HEADER_COLS)
        .ok_or(HandshakeReject::BadHeader(HEADER_COLS))?
        .parse()
        .map_err(|_| HandshakeReject::BadHeader(HEADER_COLS))?;
    let rows: u16 = header_str(headers, HEADER_ROWS)
        .ok_or(HandshakeReject::BadHeader(HEADER_ROWS))?
        .parse()
        .map_err(|_| HandshakeReject::BadHeader(HEADER_ROWS))?;

    let reconnect_reason = header_str(headers, HEADER_RECONNECT_REASON)
        .map(str::parse::<u16>)
        .transpose()
        .map_err(|_| HandshakeReject::BadHeader(HEADER_RECONNECT_REASON))?;
    let session_id = header_str(headers, HEADER_SESSION_ID).map(str::to_string);

    Ok(TunnelHandshake {
        fingerprint,
        username,
        peer_ip: peer_ip_asserted,
        term,
        cols,
        rows,
        reconnect_reason,
        session_id,
    })
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn run_tunnel_server(
    port: u16,
    state: State,
    shutdown: Option<late_core::shutdown::CancellationToken>,
) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .context("failed to bind tunnel server")?;
    tracing::info!(address = %addr, "tunnel server listening");

    run_tunnel_server_with_listener(listener, state, shutdown).await
}

pub async fn run_tunnel_server_with_listener(
    listener: TcpListener,
    state: State,
    shutdown: Option<CancellationToken>,
) -> Result<()> {
    let shutdown = shutdown.unwrap_or_default();

    let app = Router::new()
        .route("/tunnel", get(tunnel_handler))
        .layer(middleware::from_fn(http_telemetry_middleware))
        .with_state(state);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown({
        let shutdown = shutdown.clone();
        async move {
            shutdown.cancelled().await;
        }
    })
    .await
    .context("tunnel server failed")?;

    Ok(())
}

/// Owns the session-scoped accounting that `shell_request` keeps in
/// `ClientHandler`: the global conn-limit permit, the per-IP count
/// increment, the `active_users` increment, and the active-session
/// metric. Drop reverses them in the opposite order from acquisition.
///
/// Field order matters for Drop semantics: Rust drops fields top-to-
/// bottom, so `_conn_permit` is declared last to release the global
/// slot only after per-IP and per-user state have been cleaned up,
/// matching the russh path.
struct TunnelSessionGuard {
    state: State,
    peer_ip: IpAddr,
    user_id: Option<Uuid>,
    active_session_token: Option<String>,
    per_ip_incremented: bool,
    active_user_incremented: bool,
    _conn_permit: OwnedSemaphorePermit,
}

impl Drop for TunnelSessionGuard {
    fn drop(&mut self) {
        if self.active_user_incremented
            && let Some(user_id) = self.user_id
        {
            metrics::add_ssh_session(-1);
            let mut active_users = self.state.active_users.lock_recover();
            if let Some(active) = active_users.get_mut(&user_id) {
                if let Some(token) = self.active_session_token.as_ref() {
                    active.sessions.retain(|session| session.token != *token);
                }
                if active.connection_count <= 1 {
                    active_users.remove(&user_id);
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

async fn tunnel_handler(
    ws: WebSocketUpgrade,
    AxumState(state): AxumState<State>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if state.is_draining.load(Ordering::Acquire) {
        tracing::info!(
            peer_ip = %peer_addr.ip(),
            "tunnel rejected: backend is draining"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    let handshake = match validate_handshake(
        &headers,
        peer_addr.ip(),
        &state.config.tunnel_trusted_cidrs,
        &state.config.tunnel_shared_secret,
    ) {
        Ok(h) => h,
        Err(reject) => {
            tracing::warn!(
                peer_ip = %peer_addr.ip(),
                reason = reject.log_label(),
                detail = ?reject,
                "tunnel handshake rejected"
            );
            return reject.status().into_response();
        }
    };

    // Past CIDR + secret checks: this is a real connection attempt
    // from a trusted bastion, so count it.
    metrics::record_ssh_connection();

    let is_reconnect = handshake.reconnect_reason.is_some();

    // Per-IP rate limiter, keyed on the bastion-asserted client IP per
    // devdocs/LATE-CONNECTION-BASTION.md §6 (bastion is intentionally
    // ignorant of per-IP state; backend keys on X-Late-Peer-IP instead
    // of the transport peer, which is always the bastion pod).
    //
    // Authenticated bastion redials are exempt: during deploys, many
    // existing sessions can reconnect in the same rate window, and a
    // 429 is terminal from the bastion's perspective.
    if !is_reconnect && !state.ssh_attempt_limiter.allow(handshake.peer_ip) {
        tracing::warn!(
            peer_ip = %handshake.peer_ip,
            max_attempts = state.ssh_attempt_limiter.max_attempts(),
            window_secs = state.ssh_attempt_limiter.window_secs(),
            "tunnel rejected: per-IP rate limit exceeded"
        );
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    match check_ssh_admission(&state, &handshake.fingerprint, Some(handshake.peer_ip)).await {
        Ok(()) => {}
        Err(AdmissionReject::ClosedAccess) => {
            tracing::warn!(
                peer_ip = %handshake.peer_ip,
                fingerprint = %handshake.fingerprint,
                "tunnel rejected: open access disabled"
            );
            return StatusCode::FORBIDDEN.into_response();
        }
        Err(AdmissionReject::Banned) => {
            tracing::warn!(
                peer_ip = %handshake.peer_ip,
                fingerprint = %handshake.fingerprint,
                "tunnel rejected: active server ban"
            );
            return close_after_upgrade(ws, TUNNEL_CLOSE_BANNED, "banned").into_response();
        }
        Err(AdmissionReject::Infrastructure) => {
            tracing::warn!(
                peer_ip = %handshake.peer_ip,
                fingerprint = %handshake.fingerprint,
                "tunnel rejected: admission check failed"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // Global concurrent-session limit. Acquire the permit BEFORE
    // touching per-IP counts so a saturated server fails fast without
    // mutating shared state.
    let permit = match state.conn_limit.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(
                peer_ip = %handshake.peer_ip,
                "tunnel rejected: global connection limit reached"
            );
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };

    // Construct the guard now so per-IP / active_user increments below
    // unwind via Drop on any subsequent error path.
    let mut guard = TunnelSessionGuard {
        state: state.clone(),
        peer_ip: handshake.peer_ip,
        user_id: None,
        active_session_token: None,
        per_ip_incremented: false,
        active_user_incremented: false,
        _conn_permit: permit,
    };

    // Per-IP concurrent-connection cap.
    {
        let mut counts = state.conn_counts.lock_recover();
        let count = counts.entry(handshake.peer_ip).or_insert(0);
        // Authenticated reconnects may briefly exceed the normal per-IP cap
        // during deploy storms or when an old session is still unwinding.
        if !is_reconnect && *count >= state.config.max_conns_per_ip {
            tracing::warn!(
                peer_ip = %handshake.peer_ip,
                limit = state.config.max_conns_per_ip,
                "tunnel rejected: per-IP connection limit reached"
            );
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        *count += 1;
        guard.per_ip_incremented = true;
    }

    // ensure_user only fails on infrastructure errors (DB unreachable, …)
    // today — there is no User.banned column. HTTP 500 is the right
    // semantic: the bastion's reconnect dispatcher will treat 5xx as
    // retryable. When ban support lands, route ban rejections through
    // a post-upgrade WS close 4002 instead.
    let (user, is_new_user) =
        match ensure_user(&state, &handshake.username, &handshake.fingerprint).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(error = ?e, "tunnel ensure_user failed");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let Some(tunnel_permit) = state.tunnel_sessions.enter_if_accepting(&state.is_draining) else {
        tracing::info!(
            peer_ip = %handshake.peer_ip,
            "tunnel rejected: backend began draining during handshake"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    let session_token = handshake
        .session_id
        .clone()
        .unwrap_or_else(crate::session::new_session_token);

    // Register in `active_users` and bump the active-session metric.
    // Mirrors the auth_publickey block in the russh path.
    {
        let mut active_users = state.active_users.lock_recover();
        if let Some(active) = active_users.get_mut(&user.id) {
            active.connection_count += 1;
            active.username = user.username.clone();
            active.fingerprint = Some(handshake.fingerprint.clone());
            active.peer_ip = Some(handshake.peer_ip);
            active.last_login_at = Instant::now();
            if !active
                .sessions
                .iter()
                .any(|session| session.token == session_token)
            {
                active.sessions.push(ActiveSession {
                    token: session_token.clone(),
                    fingerprint: Some(handshake.fingerprint.clone()),
                    peer_ip: Some(handshake.peer_ip),
                });
            }
        } else {
            active_users.insert(
                user.id,
                ActiveUser {
                    username: user.username.clone(),
                    fingerprint: Some(handshake.fingerprint.clone()),
                    peer_ip: Some(handshake.peer_ip),
                    sessions: vec![ActiveSession {
                        token: session_token.clone(),
                        fingerprint: Some(handshake.fingerprint.clone()),
                        peer_ip: Some(handshake.peer_ip),
                    }],
                    connection_count: 1,
                    last_login_at: Instant::now(),
                },
            );
        }
    }
    metrics::add_ssh_session(1);
    guard.user_id = Some(user.id);
    guard.active_session_token = Some(session_token.clone());
    guard.active_user_incremented = true;

    // Broadcast the join. Subscribers attach in their own time; a send
    // failure here just means no one was listening.
    let _ = state.activity_feed.send(ActivityEvent {
        username: user.username.clone(),
        action: "joined".to_string(),
        at: Instant::now(),
    });

    tracing::info!(
        peer_ip = %peer_addr.ip(),
        asserted_ip = %handshake.peer_ip,
        username = %user.username,
        fingerprint = %handshake.fingerprint,
        term = %handshake.term,
        cols = handshake.cols,
        rows = handshake.rows,
        reconnect_reason = ?handshake.reconnect_reason,
        session_id = ?handshake.session_id,
        is_new_user,
        "tunnel handshake accepted; running session"
    );
    let mut handshake = handshake;
    handshake.session_id = Some(session_token.clone());

    ws.on_upgrade(move |socket| {
        handle_session(
            socket,
            handshake,
            user,
            is_new_user,
            state,
            guard,
            tunnel_permit,
        )
    })
}

async fn handle_session(
    socket: WebSocket,
    handshake: TunnelHandshake,
    user: User,
    is_new_user: bool,
    state: State,
    _guard: TunnelSessionGuard,
    _tunnel_permit: TunnelSessionPermit,
) {
    let frame_drop_log_every = state.config.frame_drop_log_every;
    let activity_feed_rx = Some(state.activity_feed.subscribe());
    let session_token = handshake
        .session_id
        .clone()
        .unwrap_or_else(crate::session::new_session_token);

    let (input_tx, input_rx) = mpsc::channel::<SshInputEvent>(INPUT_QUEUE_CAP);

    let session_config = build_session_config(
        &state,
        SessionBootstrapInputs {
            user,
            is_new_user,
            cols: handshake.cols,
            rows: handshake.rows,
            session_token,
            session_rx: None,
            activity_feed_rx,
            supports_reconnect_on_drain: true,
            reconnect_reason: handshake.reconnect_reason,
        },
    )
    .await;

    let app = match crate::app::state::App::new(session_config) {
        Ok(app) => Arc::new(TokioMutex::new(app)),
        Err(err) => {
            tracing::error!(error = ?err, "failed to initialize tunnel app");
            return;
        }
    };

    // Split the WS so the render loop's writer task and the receive loop
    // can run concurrently without holding a single `&mut WebSocket`.
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Writer task: drains an mpsc<Message> into the actual WS sink. The
    // bounded mpsc's capacity is what gives `WsFrameSink::send_frame`
    // its backpressure; capacity-saturated sends time out at 50ms and
    // surface as drops to the render loop.
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(WS_OUT_BUFFER);
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let was_close = matches!(msg, Message::Close(_));
            if let Err(err) = ws_sink.send(msg).await {
                tracing::debug!(error = ?err, "tunnel ws send failed");
                break;
            }
            if was_close {
                break;
            }
        }
        // Best-effort flush.
        let _ = ws_sink.close().await;
    });

    // Initial alt-screen enter, mirroring shell_request's pre-loop write.
    // The russh path explicitly pushes `App::enter_alt_screen()` bytes
    // through the SSH channel before spawning the render loop; the
    // tunnel path needs to do the same, otherwise the TUI's first paint
    // lands in the user's normal scrollback instead of alt-screen.
    // (Just dirtying the render signal isn't enough — ratatui's first
    // paint diffs forward from an empty terminal and never emits the
    // `\x1b[?1049h` toggle on its own.)
    let _ = out_tx
        .send(Message::Binary(
            crate::app::state::App::enter_alt_screen().into(),
        ))
        .await;

    let signal = Arc::new(RenderSignal::new());
    let render = tokio::spawn(run_session(
        Arc::clone(&app),
        input_rx,
        WsFrameSink::new(out_tx.clone()),
        frame_drop_log_every,
        Arc::clone(&signal),
    ));

    // Wake the render loop so its first paint goes out promptly without
    // waiting for input.
    if let Err(err) =
        signal
            .dirty
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
    {
        // Already dirty (e.g. resize fired before we got here); fine.
        let _ = err;
    }
    signal.notify.notify_one();

    // Receive loop: input bytes (binary), resize control frames (text), and
    // clean close from the client. Existing tunnel sessions intentionally
    // keep running after the acceptor begins graceful shutdown.
    loop {
        tokio::select! {
            biased;
            next = ws_stream.next() => {
                let Some(msg) = next else { break; };
                let msg = match msg {
                    Ok(m) => m,
                    Err(err) => {
                        tracing::debug!(error = ?err, "tunnel ws recv error");
                        break;
                    }
                };

                match msg {
                    Message::Binary(bytes) => match input_tx.try_reserve() {
                        Ok(permit) => {
                            permit.send(SshInputEvent::Bytes(bytes.into()));
                            signal.dirty.store(true, Ordering::Release);
                            signal.notify.notify_one();
                        }
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            tracing::warn!(
                                queue_cap = INPUT_QUEUE_CAP,
                                "tunnel input queue full; dropping inbound bytes"
                            );
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            tracing::debug!("tunnel input queue closed; render loop ended");
                            break;
                        }
                    },
                    // Resize is queued on the same FIFO as Bytes so the
                    // render loop applies them in WS-arrival order, not
                    // in app-lock-acquisition order.
                    Message::Text(text) => match ControlFrame::from_json(text.as_str()) {
                        Ok(ControlFrame::Resize { cols, rows }) => match input_tx.try_reserve() {
                            Ok(permit) => {
                                permit.send(SshInputEvent::Resize { cols, rows });
                                signal.dirty.store(true, Ordering::Release);
                                signal.notify.notify_one();
                            }
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                tracing::warn!(
                                    queue_cap = INPUT_QUEUE_CAP,
                                    cols,
                                    rows,
                                    "tunnel input queue full; dropping resize event"
                                );
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                tracing::debug!("tunnel input queue closed; render loop ended");
                                break;
                            }
                        },
                        Err(err) => {
                            let sample: String = text.chars().take(200).collect();
                            tracing::warn!(error = ?err, payload = ?sample, "tunnel: bad control frame");
                            let _ = out_tx
                                .send(Message::Close(Some(CloseFrame {
                                    code: TUNNEL_CLOSE_PROTOCOL_ERROR,
                                    reason: "bad control frame".into(),
                                })))
                                .await;
                            break;
                        }
                    },
                    Message::Close(_) => {
                        tracing::debug!("tunnel: client sent Close");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Tell the render loop to stop, then drain the spawn handles. The
    // render loop calls `clean_disconnect`/`eof_close`, which sends a
    // `Message::Close` down the writer mpsc; the writer task forwards
    // it and exits.
    {
        let mut app_guard = app.lock().await;
        // Peer already closed; no backend close-code signal remains to send.
        app_guard.running = false;
    }
    signal.notify.notify_one();

    let _ = render.await;
    let _ = writer.await;

    tracing::info!(
        peer_ip = %handshake.peer_ip,
        username = %handshake.username,
        "tunnel session ended"
    );
}

fn close_after_upgrade(
    ws: WebSocketUpgrade,
    code: u16,
    reason: &'static str,
) -> axum::response::Response {
    ws.on_upgrade(move |mut socket| async move {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            })))
            .await;
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn cidrs(strs: &[&str]) -> Vec<IpNet> {
        strs.iter().map(|s| s.parse().unwrap()).collect()
    }

    fn full_headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(HEADER_SECRET, HeaderValue::from_static("hunter2"));
        h.insert(HEADER_FINGERPRINT, HeaderValue::from_static("SHA256:abc"));
        h.insert(HEADER_USERNAME, HeaderValue::from_static("alice"));
        h.insert(HEADER_PEER_IP, HeaderValue::from_static("203.0.113.7"));
        h.insert(HEADER_TERM, HeaderValue::from_static("xterm-256color"));
        h.insert(HEADER_COLS, HeaderValue::from_static("120"));
        h.insert(HEADER_ROWS, HeaderValue::from_static("40"));
        h
    }

    #[test]
    fn accepts_well_formed_handshake() {
        let trusted = cidrs(&["10.42.0.0/16"]);
        let result = validate_handshake(
            &full_headers(),
            "10.42.0.5".parse().unwrap(),
            &trusted,
            "hunter2",
        )
        .unwrap();
        assert_eq!(result.fingerprint, "SHA256:abc");
        assert_eq!(result.username, "alice");
        assert_eq!(result.cols, 120);
        assert_eq!(result.rows, 40);
        assert_eq!(result.reconnect_reason, None);
        assert!(result.session_id.is_none());
    }

    #[test]
    fn parses_optional_reconnect_reason_and_session_id() {
        let mut h = full_headers();
        h.insert(HEADER_RECONNECT_REASON, HeaderValue::from_static("4100"));
        h.insert(
            HEADER_SESSION_ID,
            HeaderValue::from_static("01HX7Q4N4S2NS9X9"),
        );
        let trusted = cidrs(&["10.42.0.0/16"]);
        let result =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap();
        assert_eq!(result.reconnect_reason, Some(4100));
        assert_eq!(result.session_id.as_deref(), Some("01HX7Q4N4S2NS9X9"));
    }

    #[test]
    fn invalid_reconnect_reason_is_bad_header() {
        let mut h = full_headers();
        h.insert(HEADER_RECONNECT_REASON, HeaderValue::from_static("again"));
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap_err();
        assert_eq!(err, HandshakeReject::BadHeader(HEADER_RECONNECT_REASON));
    }

    #[test]
    fn untrusted_peer_rejected() {
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err = validate_handshake(
            &full_headers(),
            "192.0.2.5".parse().unwrap(),
            &trusted,
            "hunter2",
        )
        .unwrap_err();
        assert_eq!(err, HandshakeReject::UntrustedPeer);
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn missing_secret_rejected_as_bad_secret() {
        let mut h = full_headers();
        h.remove(HEADER_SECRET);
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap_err();
        assert_eq!(err, HandshakeReject::BadSecret);
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn wrong_secret_rejected() {
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err = validate_handshake(
            &full_headers(),
            "10.42.0.5".parse().unwrap(),
            &trusted,
            "different",
        )
        .unwrap_err();
        assert_eq!(err, HandshakeReject::BadSecret);
    }

    #[test]
    fn empty_expected_secret_rejected() {
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err = validate_handshake(&full_headers(), "10.42.0.5".parse().unwrap(), &trusted, "")
            .unwrap_err();
        assert_eq!(err, HandshakeReject::BadSecret);
    }

    #[test]
    fn missing_required_header_rejected() {
        let mut h = full_headers();
        h.remove(HEADER_FINGERPRINT);
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap_err();
        assert_eq!(err, HandshakeReject::BadHeader(HEADER_FINGERPRINT));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn bad_cols_rejected() {
        let mut h = full_headers();
        h.insert(HEADER_COLS, HeaderValue::from_static("notanumber"));
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap_err();
        assert_eq!(err, HandshakeReject::BadHeader(HEADER_COLS));
    }

    #[test]
    fn bad_peer_ip_rejected() {
        let mut h = full_headers();
        h.insert(HEADER_PEER_IP, HeaderValue::from_static("not-an-ip"));
        let trusted = cidrs(&["10.42.0.0/16"]);
        let err =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap_err();
        assert_eq!(err, HandshakeReject::BadHeader(HEADER_PEER_IP));
    }

    #[test]
    fn constant_time_eq_basic_cases() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"abcdef", b"abcdef"));
        assert!(!constant_time_eq(b"abcdef", b"abcdeg"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
