//! End-to-end smoke tests for the bastion-facing `/tunnel` listener.
//!
//! Mirrors the manual websocat recipe used during Phase 2c bring-up:
//! happy-path WebSocket upgrade → at least one binary frame flows back,
//! plus the three handshake-rejection branches (bad secret, missing
//! header, untrusted peer) that are cheap to assert at the HTTP layer.

mod helpers;

use futures_util::{SinkExt, StreamExt};
use helpers::{new_test_db, test_app_state, test_config};
use ipnet::IpNet;
use late_core::MutexRecover;
use late_core::shutdown::CancellationToken;
use late_core::tunnel_protocol::ControlFrame;
use late_ssh::config::Config;
use late_ssh::state::State;
use late_ssh::tunnel::{
    HEADER_COLS, HEADER_FINGERPRINT, HEADER_PEER_IP, HEADER_ROWS, HEADER_SECRET, HEADER_TERM,
    HEADER_USERNAME, run_tunnel_server_with_listener,
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const TEST_SECRET: &str = "test-secret";

fn loopback_cidr() -> Vec<IpNet> {
    vec!["127.0.0.0/8".parse().expect("cidr")]
}

type SpawnedTunnel = (
    SocketAddr,
    State,
    CancellationToken,
    tokio::task::JoinHandle<()>,
);

async fn spawn_tunnel(trusted: Vec<IpNet>) -> SpawnedTunnel {
    spawn_tunnel_with(trusted, |_| {}).await
}

async fn spawn_tunnel_with(trusted: Vec<IpNet>, tweak: impl FnOnce(&mut Config)) -> SpawnedTunnel {
    let test_db = new_test_db().await;
    let mut config = test_config(test_db.db.config().clone());
    config.tunnel_trusted_cidrs = trusted;
    config.tunnel_shared_secret = TEST_SECRET.to_string();
    tweak(&mut config);
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let shutdown = CancellationToken::new();

    // Move the TestDb guard into the task so the Postgres container
    // outlives the server.
    let state_for_server = state.clone();
    let shutdown_for_server = shutdown.clone();
    let task = tokio::spawn(async move {
        let _guard = test_db;
        let _ =
            run_tunnel_server_with_listener(listener, state_for_server, Some(shutdown_for_server))
                .await;
    });
    (addr, state, shutdown, task)
}

/// Build a tungstenite request with the standard set of valid handshake
/// headers. Tests that need to flip a single header (bad secret, missing
/// fingerprint, …) start from this and mutate.
fn make_request(
    addr: SocketAddr,
    username: &str,
) -> tokio_tungstenite::tungstenite::http::Request<()> {
    let url = format!("ws://{addr}/tunnel");
    let mut req = url.into_client_request().expect("client request");
    let h = req.headers_mut();
    h.insert(HEADER_SECRET, HeaderValue::from_static(TEST_SECRET));
    h.insert(
        HEADER_FINGERPRINT,
        HeaderValue::from_static("SHA256:smoke-fp"),
    );
    h.insert(HEADER_USERNAME, HeaderValue::from_str(username).unwrap());
    h.insert(HEADER_PEER_IP, HeaderValue::from_static("127.0.0.1"));
    h.insert(HEADER_TERM, HeaderValue::from_static("xterm-256color"));
    h.insert(HEADER_COLS, HeaderValue::from_static("80"));
    h.insert(HEADER_ROWS, HeaderValue::from_static("24"));
    req
}

#[tokio::test]
async fn tunnel_happy_path_yields_initial_frame_and_accepts_resize() {
    let (addr, _state, _shutdown, server) = spawn_tunnel(loopback_cidr()).await;

    let req = make_request(addr, "smoke-user");
    let (mut ws, response) = timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(req),
    )
    .await
    .expect("connect_async timeout")
    .expect("connect_async");

    assert_eq!(response.status().as_u16(), 101);

    // Wait for at least one binary frame; the render loop pushes the
    // initial alt-screen + paint immediately on session start.
    let first = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("frame timeout")
        .expect("stream ended")
        .expect("ws error");
    match first {
        Message::Binary(bytes) => {
            assert!(!bytes.is_empty(), "first binary frame should be non-empty")
        }
        other => panic!("expected Binary, got {other:?}"),
    }

    // Send a resize control frame; server should accept it without
    // closing the connection. We don't assert a particular response —
    // the contract is that the session keeps rendering.
    let resize = ControlFrame::Resize {
        cols: 100,
        rows: 30,
    }
    .to_json()
    .expect("encode resize");
    ws.send(Message::Text(resize.into()))
        .await
        .expect("send resize");

    // Drain a couple more frames to confirm the loop is still alive
    // post-resize. Two attempts, each with its own timeout, lets us tell
    // "render still working" from "session torn down".
    for _ in 0..2 {
        let msg = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("post-resize frame timeout")
            .expect("post-resize stream ended")
            .expect("post-resize ws error");
        assert!(matches!(msg, Message::Binary(_)));
    }

    let _ = ws.close(None).await;
    server.abort();
}

#[tokio::test]
async fn tunnel_rejects_wrong_secret_with_401() {
    let (addr, _state, _shutdown, server) = spawn_tunnel(loopback_cidr()).await;
    let status = raw_upgrade_status(addr, &[(HEADER_SECRET, "not-the-secret")]).await;
    assert_eq!(status, 401);
    server.abort();
}

#[tokio::test]
async fn tunnel_rejects_missing_required_header_with_400() {
    let (addr, _state, _shutdown, server) = spawn_tunnel(loopback_cidr()).await;
    let status = raw_upgrade_status_omit(addr, HEADER_FINGERPRINT).await;
    assert_eq!(status, 400);
    server.abort();
}

#[tokio::test]
async fn tunnel_rejects_untrusted_peer_with_403() {
    let trusted: Vec<IpNet> = vec!["192.0.2.0/24".parse().expect("cidr")];
    let (addr, _state, _shutdown, server) = spawn_tunnel(trusted).await;
    let status = raw_upgrade_status(addr, &[]).await;
    assert_eq!(status, 403);
    server.abort();
}

#[tokio::test]
async fn tunnel_session_registers_active_user_and_unregisters_on_close() {
    let (addr, state, _shutdown, server) = spawn_tunnel(loopback_cidr()).await;

    let req = make_request(addr, "active-user");
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("connect");

    // Wait for the first frame. By the time bytes arrive, the registration
    // block in `tunnel_handler` has already run — it's synchronous before
    // `ws.on_upgrade`.
    let _ = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("first frame timeout");

    {
        let active = state.active_users.lock_recover();
        let usernames: Vec<_> = active.values().map(|a| a.username.clone()).collect();
        assert!(
            usernames.iter().any(|u| u == "active-user"),
            "expected active-user registered, got {usernames:?}"
        );
    }

    let _ = ws.close(None).await;
    drop(ws);

    // Server-side teardown is async (render task drains, then the guard
    // drops). Poll briefly for active_users to empty.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        {
            let active = state.active_users.lock_recover();
            if active.is_empty() {
                break;
            }
        }
        if std::time::Instant::now() >= deadline {
            let active = state.active_users.lock_recover();
            panic!("active_users did not drain: {:?}", *active);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    server.abort();
}

#[tokio::test]
async fn tunnel_session_emits_joined_activity_event() {
    let (addr, state, _shutdown, server) = spawn_tunnel(loopback_cidr()).await;

    // Subscribe BEFORE the dial — broadcast only delivers messages sent
    // after subscription.
    let mut activity_rx = state.activity_feed.subscribe();

    let req = make_request(addr, "activity-user");
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("connect");

    let event = timeout(Duration::from_secs(5), activity_rx.recv())
        .await
        .expect("activity timeout")
        .expect("activity recv");
    assert_eq!(event.username, "activity-user");
    assert_eq!(event.action, "joined");

    let _ = ws.close(None).await;
    server.abort();
}

#[tokio::test]
async fn tunnel_returns_503_when_global_conn_limit_reached() {
    let (addr, _state, _shutdown, server) = spawn_tunnel_with(loopback_cidr(), |c| {
        c.max_conns_global = 1;
    })
    .await;

    // Open the first session and hold it. Wait for the first frame so we
    // know the global permit has been acquired before issuing the second
    // attempt.
    let req1 = make_request(addr, "global-1");
    let (mut ws1, _) = tokio_tungstenite::connect_async(req1)
        .await
        .expect("connect 1");
    let _ = timeout(Duration::from_secs(5), ws1.next())
        .await
        .expect("first frame timeout");

    let status = raw_upgrade_status(addr, &[]).await;
    assert_eq!(status, 503, "second attempt should hit global cap");

    // Drop only after the assertion so the permit stays held.
    drop(ws1);
    server.abort();
}

#[tokio::test]
async fn tunnel_returns_429_when_per_ip_conn_limit_reached() {
    let (addr, _state, _shutdown, server) = spawn_tunnel_with(loopback_cidr(), |c| {
        c.max_conns_per_ip = 1;
    })
    .await;

    let req1 = make_request(addr, "perip-1");
    let (mut ws1, _) = tokio_tungstenite::connect_async(req1)
        .await
        .expect("connect 1");
    let _ = timeout(Duration::from_secs(5), ws1.next())
        .await
        .expect("first frame timeout");

    let status = raw_upgrade_status(addr, &[]).await;
    assert_eq!(
        status, 429,
        "second attempt from same IP should hit per-IP cap"
    );

    drop(ws1);
    server.abort();
}

#[tokio::test]
async fn tunnel_drain_emits_close_1000() {
    let (addr, _state, shutdown, server) = spawn_tunnel(loopback_cidr()).await;

    let req = make_request(addr, "drain-user");
    let (mut ws, response) = tokio_tungstenite::connect_async(req)
        .await
        .expect("connect_async");
    assert_eq!(response.status().as_u16(), 101);

    // Drain the first frame to confirm the session is live before we
    // trigger shutdown — otherwise we could race the cancel against
    // upgrade completion.
    let _ = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("first frame timeout")
        .expect("stream ended")
        .expect("ws error");

    shutdown.cancel();

    // The receive loop's biased cancellation arm queues a 1000 Close;
    // the writer task forwards it. Anything queued behind us in the
    // mpsc (residual render frames) arrives first, so drain Binary
    // until we see the Close.
    let close_frame = loop {
        let msg = timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("close timeout")
            .expect("stream ended")
            .expect("ws error");
        match msg {
            Message::Binary(_) => continue,
            Message::Close(Some(frame)) => break frame,
            Message::Close(None) => panic!("expected Close with code, got Close(None)"),
            other => panic!("expected Binary or Close, got {other:?}"),
        }
    };
    assert_eq!(close_frame.code, CloseCode::Normal);

    // Server task should wind down on its own once the cancellation
    // propagates through axum's graceful shutdown.
    let _ = timeout(Duration::from_secs(5), server)
        .await
        .expect("server didn't exit after drain");
}

/// Send a handcrafted HTTP/1.1 Upgrade request and return the numeric
/// status. Useful for exercising rejection paths where we don't need a
/// real WebSocket — and where tungstenite would refuse to surface the
/// HTTP status.
async fn raw_upgrade_status(addr: SocketAddr, header_overrides: &[(&str, &str)]) -> u16 {
    let mut headers = vec![
        (HEADER_SECRET, TEST_SECRET),
        (HEADER_FINGERPRINT, "SHA256:smoke-fp"),
        (HEADER_USERNAME, "smoke-user"),
        (HEADER_PEER_IP, "127.0.0.1"),
        (HEADER_TERM, "xterm-256color"),
        (HEADER_COLS, "80"),
        (HEADER_ROWS, "24"),
    ];
    for (name, value) in header_overrides {
        if let Some(slot) = headers.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value;
        } else {
            headers.push((name, value));
        }
    }
    raw_upgrade_with(addr, &headers).await
}

async fn raw_upgrade_status_omit(addr: SocketAddr, omit: &str) -> u16 {
    let headers: Vec<(&str, &str)> = [
        (HEADER_SECRET, TEST_SECRET),
        (HEADER_FINGERPRINT, "SHA256:smoke-fp"),
        (HEADER_USERNAME, "smoke-user"),
        (HEADER_PEER_IP, "127.0.0.1"),
        (HEADER_TERM, "xterm-256color"),
        (HEADER_COLS, "80"),
        (HEADER_ROWS, "24"),
    ]
    .into_iter()
    .filter(|(n, _)| *n != omit)
    .collect();
    raw_upgrade_with(addr, &headers).await
}

async fn raw_upgrade_with(addr: SocketAddr, headers: &[(&str, &str)]) -> u16 {
    let mut stream = TcpStream::connect(addr).await.expect("tcp connect");
    let mut req = String::new();
    req.push_str("GET /tunnel HTTP/1.1\r\n");
    req.push_str(&format!("Host: {addr}\r\n"));
    req.push_str("Upgrade: websocket\r\n");
    req.push_str("Connection: Upgrade\r\n");
    req.push_str("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n");
    req.push_str("Sec-WebSocket-Version: 13\r\n");
    for (name, value) in headers {
        req.push_str(&format!("{name}: {value}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.expect("read");
    let response = String::from_utf8_lossy(&buf[..n]);
    let first = response.lines().next().unwrap_or_default();
    first
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0)
}
