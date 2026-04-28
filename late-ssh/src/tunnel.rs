//! `/tunnel` listener — bastion-only WebSocket entry point.
//!
//! Phase 2a scope: bind the private listener, validate handshake (IP
//! allowlist, pre-shared secret, required headers), accept the upgrade,
//! and close immediately with WS code 1000. **No proxy logic, no
//! `App::new` wiring** — those land in Phase 2c.
//!
//! The `:4001` listener is intentionally separate from the public `:4000`
//! API listener (per `PERSISTENT-CONNECTION-GATEWAY.md` §3): mixing trust
//! domains on one socket is a known footgun, and a separate bind gives
//! kernel-level isolation in addition to the in-app checks below.

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{
        ConnectInfo, State as AxumState, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket, close_code},
    },
    http::{HeaderMap, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
};
use ipnet::IpNet;
use late_core::telemetry::http_telemetry_middleware;
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;

use crate::state::State;

/// Required client headers on the `/tunnel` upgrade. Captured here so the
/// handler and tests stay in lockstep.
pub const HEADER_SECRET: &str = "x-late-secret";
pub const HEADER_FINGERPRINT: &str = "x-late-fingerprint";
pub const HEADER_USERNAME: &str = "x-late-username";
pub const HEADER_PEER_IP: &str = "x-late-peer-ip";
pub const HEADER_TERM: &str = "x-late-term";
pub const HEADER_COLS: &str = "x-late-cols";
pub const HEADER_ROWS: &str = "x-late-rows";
pub const HEADER_RECONNECT: &str = "x-late-reconnect";
pub const HEADER_SESSION_ID: &str = "x-late-session-id";

/// Validated handshake. Phase 2c will hand this to the session bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelHandshake {
    pub fingerprint: String,
    pub username: String,
    pub peer_ip: IpAddr,
    pub term: String,
    pub cols: u16,
    pub rows: u16,
    pub reconnect: bool,
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

    let presented_secret = header_str(headers, HEADER_SECRET).unwrap_or("");
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

    let reconnect = matches!(header_str(headers, HEADER_RECONNECT), Some("1"));
    let session_id = header_str(headers, HEADER_SESSION_ID).map(str::to_string);

    Ok(TunnelHandshake {
        fingerprint,
        username,
        peer_ip: peer_ip_asserted,
        term,
        cols,
        rows,
        reconnect,
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
    shutdown: Option<late_core::shutdown::CancellationToken>,
) -> Result<()> {
    let app = Router::new()
        .route("/tunnel", get(tunnel_handler))
        .layer(middleware::from_fn(http_telemetry_middleware))
        .with_state(state);

    let shutdown = shutdown.unwrap_or_default();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown.cancelled().await;
    })
    .await
    .context("tunnel server failed")?;

    Ok(())
}

async fn tunnel_handler(
    ws: WebSocketUpgrade,
    AxumState(state): AxumState<State>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
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

    tracing::info!(
        peer_ip = %peer_addr.ip(),
        asserted_ip = %handshake.peer_ip,
        username = %handshake.username,
        fingerprint = %handshake.fingerprint,
        term = %handshake.term,
        cols = handshake.cols,
        rows = handshake.rows,
        reconnect = handshake.reconnect,
        session_id = ?handshake.session_id,
        "tunnel handshake accepted (stub — closing 1000)"
    );

    ws.on_upgrade(move |socket| handle_stub(socket, handshake))
}

async fn handle_stub(mut socket: WebSocket, handshake: TunnelHandshake) {
    let frame = CloseFrame {
        code: close_code::NORMAL,
        reason: "tunnel stub: phase 2c not yet wired".into(),
    };
    if let Err(err) = socket.send(Message::Close(Some(frame))).await {
        tracing::debug!(error = ?err, "stub close send failed");
    }
    tracing::debug!(
        username = %handshake.username,
        "tunnel stub closed cleanly"
    );
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
        assert!(!result.reconnect);
        assert!(result.session_id.is_none());
    }

    #[test]
    fn parses_optional_reconnect_and_session_id() {
        let mut h = full_headers();
        h.insert(HEADER_RECONNECT, HeaderValue::from_static("1"));
        h.insert(
            HEADER_SESSION_ID,
            HeaderValue::from_static("01HX7Q4N4S2NS9X9"),
        );
        let trusted = cidrs(&["10.42.0.0/16"]);
        let result =
            validate_handshake(&h, "10.42.0.5".parse().unwrap(), &trusted, "hunter2").unwrap();
        assert!(result.reconnect);
        assert_eq!(result.session_id.as_deref(), Some("01HX7Q4N4S2NS9X9"));
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
