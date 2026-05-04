//! End-to-end smoke test for the bastion's `/tunnel` proxy.
//!
//! Spins up:
//!   1. A mock backend WS server (raw `tokio_tungstenite::accept_hdr_async`,
//!      not axum — the bastion only needs the wire shape, not the route).
//!   2. A real `late-bastion` SSH listener pointed at (1).
//!   3. A `russh::client` driving (2) the same way a user's `ssh` would.
//!
//! Asserts that:
//!   - The handshake headers reach the backend with the right pubkey
//!     fingerprint, peer IP, term, and PTY dimensions.
//!   - User-side bytes round-trip into a backend-side `Binary` frame.
//!   - Backend-side `Binary` frames land on the user's SSH stream.
//!   - SSH `window-change` shows up as a `ControlFrame::Resize` text
//!     frame on the backend.
//!
//! Phase 4 additions (`ScriptedBackend`):
//!   - On retryable WS close (4100), the bastion redials with the same
//!     `X-Late-Session-Id` and `X-Late-Reconnect-Reason: 4100`.
//!   - On terminal WS close (4001), the bastion ends the session
//!     without further dial attempts.
//!   - On HTTP 4xx upgrade rejection (401), same: terminal, no redial.

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use getrandom::SysRng;
use late_bastion::config::Config;
use late_bastion::ssh::{Server, load_or_generate_key};
use late_core::shutdown::CancellationToken;
use late_core::tunnel_protocol::{
    ControlFrame, HEADER_FINGERPRINT, HEADER_PEER_IP, HEADER_RECONNECT_REASON, HEADER_SECRET,
    HEADER_SESSION_ID,
};
use russh::client::{self as russh_client, Handler as ClientHandler};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, signature::rand_core::UnwrapErr};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::{HeaderMap, StatusCode};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const TEST_SECRET: &str = "test-secret";

/// Channel a test pushes onto to send a binary frame to the user; mock
/// backend forwards it onto the actual WS sink.
type BackendSendTx = mpsc::Sender<WsMessage>;

#[derive(Clone)]
struct CaptureHeaders {
    captured: Arc<std::sync::Mutex<Option<HeaderMap>>>,
}

impl Callback for CaptureHeaders {
    #[allow(clippy::result_large_err)]
    fn on_request(self, req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
        *self.captured.lock().unwrap() = Some(req.headers().clone());
        Ok(resp)
    }
}

struct RejectWithStatus {
    captured: Arc<std::sync::Mutex<Option<HeaderMap>>>,
    status: StatusCode,
}

impl Callback for RejectWithStatus {
    #[allow(clippy::result_large_err)]
    fn on_request(self, req: &Request, _resp: Response) -> Result<Response, ErrorResponse> {
        *self.captured.lock().unwrap() = Some(req.headers().clone());
        let mut err: ErrorResponse = ErrorResponse::new(None);
        *err.status_mut() = self.status;
        Err(err)
    }
}

struct MockBackend {
    addr: SocketAddr,
    /// Headers from the upgrade request, populated when a client dials.
    headers_rx: oneshot::Receiver<HeaderMap>,
    /// Frames the bastion forwarded to us.
    received_rx: mpsc::Receiver<WsMessage>,
    /// Channel to push frames toward the user.
    send_tx: AsyncMutex<Option<BackendSendTx>>,
    /// Becomes ready when the mock task exits.
    _join: tokio::task::JoinHandle<()>,
}

impl MockBackend {
    async fn spawn() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let (headers_tx, headers_rx) = oneshot::channel::<HeaderMap>();
        let (received_tx, received_rx) = mpsc::channel::<WsMessage>(64);
        let (send_tx, mut send_rx) = mpsc::channel::<WsMessage>(64);

        let join = tokio::spawn(async move {
            let (tcp, _peer) = match listener.accept().await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("mock backend accept failed: {e:?}");
                    return;
                }
            };

            let captured: Arc<std::sync::Mutex<Option<HeaderMap>>> = Default::default();

            let ws = match tokio_tungstenite::accept_hdr_async(
                tcp,
                CaptureHeaders {
                    captured: captured.clone(),
                },
            )
            .await
            {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!("mock backend ws upgrade failed: {e:?}");
                    return;
                }
            };

            if let Some(h) = captured.lock().unwrap().take() {
                let _ = headers_tx.send(h);
            }

            let (mut sink, mut stream) = ws.split();

            // Pump any test-driven outbound frames concurrently with
            // capturing inbound frames.
            let pump_out = async {
                while let Some(msg) = send_rx.recv().await {
                    let stop = matches!(msg, WsMessage::Close(_));
                    if sink.send(msg).await.is_err() {
                        break;
                    }
                    if stop {
                        break;
                    }
                }
            };

            let pump_in = async {
                while let Some(Ok(msg)) = stream.next().await {
                    let stop = matches!(msg, WsMessage::Close(_));
                    let _ = received_tx.send(msg).await;
                    if stop {
                        break;
                    }
                }
            };

            tokio::join!(pump_out, pump_in);
        });

        Ok(Self {
            addr,
            headers_rx,
            received_rx,
            send_tx: AsyncMutex::new(Some(send_tx)),
            _join: join,
        })
    }

    fn ws_url(&self) -> String {
        format!("ws://{}/tunnel", self.addr)
    }

    async fn wait_for_handshake(&mut self) -> HeaderMap {
        timeout(Duration::from_secs(5), &mut self.headers_rx)
            .await
            .expect("handshake timeout")
            .expect("handshake closed without sending")
    }

    /// Receive the next frame the bastion forwarded to us. Filters out
    /// nothing — caller asserts on the variant.
    async fn next_frame(&mut self) -> Option<WsMessage> {
        timeout(Duration::from_secs(5), self.received_rx.recv())
            .await
            .expect("frame recv timeout")
    }

    async fn expect_setup_frames(&mut self, term: &str, cols: u16, rows: u16) {
        let pty = self.next_frame().await.expect("pty frame");
        match pty {
            WsMessage::Text(text) => {
                let parsed = ControlFrame::from_json(text.as_str()).expect("parse pty");
                assert_eq!(
                    parsed,
                    ControlFrame::Pty {
                        term: term.to_string(),
                        cols,
                        rows
                    }
                );
            }
            other => panic!("expected pty Text frame, got {other:?}"),
        }

        let shell_start = self.next_frame().await.expect("shell_start frame");
        match shell_start {
            WsMessage::Text(text) => {
                let parsed = ControlFrame::from_json(text.as_str()).expect("parse shell_start");
                assert_eq!(parsed, ControlFrame::ShellStart);
            }
            other => panic!("expected shell_start Text frame, got {other:?}"),
        }
    }

    async fn send(&self, msg: WsMessage) {
        let guard = self.send_tx.lock().await;
        guard
            .as_ref()
            .expect("send channel still open")
            .send(msg)
            .await
            .expect("backend send");
    }
}

struct TestBastion {
    addr: SocketAddr,
    cancel: CancellationToken,
    _host_key_path: PathBuf,
}

impl TestBastion {
    async fn spawn(backend_url: String) -> Result<Self> {
        Self::spawn_with(backend_url, false, vec![]).await
    }

    /// Spawn variant that lets a test enable PROXY v1 parsing and
    /// configure the trusted CIDR list. The accept loop mirrors the
    /// production `ssh::run` path so PROXY v1 behavior under test is
    /// the same code that runs in the binary.
    async fn spawn_with(
        backend_url: String,
        proxy_protocol: bool,
        proxy_trusted_cidrs: Vec<ipnet::IpNet>,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let host_key_path =
            std::env::temp_dir().join(format!("late-bastion-it-key-{}", uuid::Uuid::now_v7()));
        let config = Arc::new(Config {
            ssh_port: 0,
            host_key_path: host_key_path.clone(),
            ssh_idle_timeout: 60,
            backend_tunnel_url: backend_url,
            backend_shared_secret: TEST_SECRET.to_string(),
            max_conns_global: 16,
            proxy_protocol,
            proxy_trusted_cidrs,
        });

        let host_key = load_or_generate_key(&config.host_key_path)?;
        let russh_config = Arc::new(russh::server::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            auth_rejection_time: Duration::from_secs(1),
            keys: vec![host_key],
            ..Default::default()
        });

        let cancel = CancellationToken::new();
        let server = Server::new(config.clone(), cancel.clone());
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        let (mut tcp, transport_peer_addr) = match accept {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        let cfg = russh_config.clone();
                        let server = server.clone();
                        let config = Arc::clone(&config);
                        tokio::spawn(async move {
                            let proxied_addr = match late_bastion::ssh::resolve_proxied_client_addr(
                                &config,
                                &mut tcp,
                                transport_peer_addr,
                            )
                            .await
                            {
                                Ok(a) => a,
                                Err(_) => return,
                            };
                            let handler = server.new_client_with_addrs(
                                Some(transport_peer_addr),
                                proxied_addr,
                            );
                            if let Ok(sess) = russh::server::run_stream(cfg, tcp, handler).await {
                                let _ = sess.await;
                            }
                        });
                    }
                    _ = cancel_clone.cancelled() => break,
                }
            }
        });

        Ok(Self {
            addr,
            cancel,
            _host_key_path: host_key_path,
        })
    }

    fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// `russh::client` handler that accepts any server key.
struct AnyHostKey;
impl ClientHandler for AnyHostKey {
    type Error = anyhow::Error;
    async fn check_server_key(
        &mut self,
        _key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[tokio::test]
async fn bastion_proxies_ssh_to_tunnel_with_full_handshake_and_byte_flow() {
    let mut backend = MockBackend::spawn().await.expect("backend");
    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    // Generate a user keypair and connect.
    let user_key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)
        .expect("user key");
    let user_fingerprint = user_key
        .public_key()
        .fingerprint(HashAlg::Sha256)
        .to_string();

    let client_config = Arc::new(russh_client::Config::default());
    let mut session = russh_client::connect(client_config, bastion.addr, AnyHostKey)
        .await
        .expect("client connect");

    let auth_res = session
        .authenticate_publickey(
            "alice",
            PrivateKeyWithHashAlg::new(Arc::new(user_key), None),
        )
        .await
        .expect("authenticate_publickey");
    assert!(auth_res.success(), "auth not accepted");

    let channel = session
        .channel_open_session()
        .await
        .expect("channel_open_session");

    channel
        .request_pty(true, "xterm-256color", 100, 30, 0, 0, &[])
        .await
        .expect("request_pty");
    channel.request_shell(true).await.expect("request_shell");

    // Wait for the bastion to dial the backend and present the handshake.
    let headers = backend.wait_for_handshake().await;
    assert_eq!(
        headers.get(HEADER_SECRET).unwrap(),
        TEST_SECRET,
        "shared secret"
    );
    assert_eq!(
        headers.get(HEADER_FINGERPRINT).unwrap(),
        &user_fingerprint,
        "fingerprint mismatch"
    );
    // Loopback: peer_ip captured by bastion is 127.0.0.1 (no PROXY v1).
    assert_eq!(headers.get(HEADER_PEER_IP).unwrap(), "127.0.0.1");
    // session_id is a UUID — just assert the header is present and non-empty.
    let sid = headers
        .get(HEADER_SESSION_ID)
        .expect("session id header")
        .to_str()
        .unwrap();
    assert!(!sid.is_empty(), "session id non-empty");
    backend.expect_setup_frames("xterm-256color", 100, 30).await;

    // Take the channel as a stream and exchange bytes.
    let stream = channel.into_stream();
    let (mut user_reader, mut user_writer) = tokio::io::split(stream);

    // User → backend.
    user_writer.write_all(b"abc").await.expect("user write");
    user_writer.flush().await.ok();

    loop {
        let frame = backend.next_frame().await.expect("backend frame");
        match frame {
            WsMessage::Binary(bytes) => {
                assert_eq!(bytes.as_ref(), b"abc");
                break;
            }
            WsMessage::Ping(_) | WsMessage::Pong(_) => {}
            other => panic!("expected user bytes as Binary, got {other:?}"),
        }
    }

    // Backend → user.
    backend
        .send(WsMessage::Binary(b"DEF".to_vec().into()))
        .await;
    let mut buf = [0u8; 16];
    let n = timeout(Duration::from_secs(5), user_reader.read(&mut buf))
        .await
        .expect("user read timeout")
        .expect("user read");
    assert_eq!(&buf[..n], b"DEF");

    // Resize forwarding is covered in
    // `bastion_forwards_window_change_as_resize_text_frame` below;
    // here we've already consumed the channel into a stream, so
    // calling `Channel::window_change` is no longer available.
    drop(user_reader);
    drop(user_writer);
    bastion.shutdown();
}

#[tokio::test]
async fn bastion_reuses_exec_tunnel_for_shell_setup() {
    let mut backend = MockBackend::spawn().await.expect("backend");
    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    let user_key =
        PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519).expect("key");
    let client_config = Arc::new(russh_client::Config::default());
    let mut session = russh_client::connect(client_config, bastion.addr, AnyHostKey)
        .await
        .expect("client connect");
    let auth = session
        .authenticate_publickey(
            "alice",
            PrivateKeyWithHashAlg::new(Arc::new(user_key), None),
        )
        .await
        .expect("authenticate_publickey");
    assert!(auth.success(), "auth not accepted");

    let mut channel = session
        .channel_open_session()
        .await
        .expect("channel_open_session");
    channel
        .exec(true, "late-cli-token-v1")
        .await
        .expect("exec request");

    let headers = backend.wait_for_handshake().await;
    let sid = headers
        .get(HEADER_SESSION_ID)
        .expect("session id header")
        .to_str()
        .unwrap();
    assert!(!sid.is_empty(), "session id non-empty");

    let exec_id = match backend.next_frame().await.expect("exec frame") {
        WsMessage::Text(text) => {
            match ControlFrame::from_json(text.as_str()).expect("parse exec") {
                ControlFrame::ExecRequest { id, command } => {
                    assert_eq!(command, "late-cli-token-v1");
                    id
                }
                other => panic!("expected exec_request, got {other:?}"),
            }
        }
        other => panic!("expected exec_request Text frame, got {other:?}"),
    };

    let response = ControlFrame::ExecResponse {
        id: exec_id,
        stdout: r#"{"session_token":"tok"}"#.to_string(),
        stderr: String::new(),
        exit_status: 0,
    }
    .to_json()
    .expect("encode exec response");
    backend.send(WsMessage::Text(response.into())).await;

    let mut stdout = Vec::new();
    let mut exit_status = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => stdout.extend_from_slice(data.as_ref()),
            russh::ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
            russh::ChannelMsg::Close => break,
            _ => {}
        }
    }

    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        r#"{"session_token":"tok"}"#
    );
    assert_eq!(exit_status, Some(0));

    let shell = session
        .channel_open_session()
        .await
        .expect("shell channel_open_session");
    shell
        .request_pty(true, "xterm-256color", 120, 40, 0, 0, &[])
        .await
        .expect("shell request_pty");
    shell
        .request_shell(true)
        .await
        .expect("shell request_shell");

    backend.expect_setup_frames("xterm-256color", 120, 40).await;

    let stream = shell.into_stream();
    let (_user_reader, mut user_writer) = tokio::io::split(stream);
    user_writer
        .write_all(b"after-exec")
        .await
        .expect("user write");
    user_writer.flush().await.ok();

    loop {
        let frame = backend.next_frame().await.expect("backend frame");
        match frame {
            WsMessage::Binary(bytes) => {
                assert_eq!(bytes.as_ref(), b"after-exec");
                break;
            }
            WsMessage::Ping(_) | WsMessage::Pong(_) => {}
            other => panic!("expected post-shell user bytes as Binary, got {other:?}"),
        }
    }

    bastion.shutdown();
}

#[tokio::test]
async fn bastion_uses_proxy_v1_source_ip_for_peer_ip_header() {
    let mut backend = MockBackend::spawn().await.expect("backend");
    // Trust loopback so the bastion accepts the PROXY v1 header from
    // our test client.
    let trusted: Vec<ipnet::IpNet> = vec!["127.0.0.0/8".parse().unwrap()];
    let bastion = TestBastion::spawn_with(backend.ws_url(), true, trusted)
        .await
        .expect("bastion");

    // Open a raw TCP stream so we can write the PROXY v1 line *before*
    // russh's SSH version handshake.
    let mut tcp = TcpStream::connect(bastion.addr).await.expect("tcp connect");
    tcp.write_all(b"PROXY TCP4 198.51.100.42 203.0.113.7 54321 5222\r\n")
        .await
        .expect("write proxy v1");

    let user_key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)
        .expect("user key");
    let client_config = Arc::new(russh_client::Config::default());
    let mut session = russh_client::connect_stream(client_config, tcp, AnyHostKey)
        .await
        .expect("connect_stream");

    let auth_res = session
        .authenticate_publickey(
            "alice",
            PrivateKeyWithHashAlg::new(Arc::new(user_key), None),
        )
        .await
        .expect("authenticate_publickey");
    assert!(auth_res.success(), "auth not accepted");

    let channel = session
        .channel_open_session()
        .await
        .expect("channel_open_session");
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request_pty");
    channel.request_shell(true).await.expect("request_shell");

    let headers = backend.wait_for_handshake().await;
    assert_eq!(
        headers.get(HEADER_PEER_IP).unwrap(),
        "198.51.100.42",
        "X-Late-Peer-IP should reflect the PROXY v1 source IP, not the transport peer"
    );

    bastion.shutdown();
}

#[tokio::test]
async fn bastion_forwards_window_change_as_resize_text_frame() {
    let mut backend = MockBackend::spawn().await.expect("backend");
    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    let user_key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)
        .expect("user key");
    let client_config = Arc::new(russh_client::Config::default());
    let mut session = russh_client::connect(client_config, bastion.addr, AnyHostKey)
        .await
        .expect("client connect");
    let auth_res = session
        .authenticate_publickey(
            "alice",
            PrivateKeyWithHashAlg::new(Arc::new(user_key), None),
        )
        .await
        .expect("authenticate_publickey");
    assert!(auth_res.success());

    let channel = session
        .channel_open_session()
        .await
        .expect("channel_open_session");
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request_pty");
    channel.request_shell(true).await.expect("request_shell");

    // Drain handshake.
    let _ = backend.wait_for_handshake().await;
    backend.expect_setup_frames("xterm-256color", 80, 24).await;

    // Trigger a resize. The Channel<Msg> handle is still in scope here
    // — we haven't consumed it via into_stream — so we can call
    // window_change directly.
    channel
        .window_change(132, 50, 0, 0)
        .await
        .expect("window_change");

    // Read frames until we see the resize text frame; ignore any
    // intervening binary frames the user side might have sent (none
    // here, but defensive).
    let mut saw_resize = false;
    for _ in 0..4 {
        let Some(frame) = backend.next_frame().await else {
            break;
        };
        if let WsMessage::Text(text) = frame {
            let parsed = ControlFrame::from_json(text.as_str()).expect("parse resize");
            assert_eq!(
                parsed,
                ControlFrame::Resize {
                    cols: 132,
                    rows: 50,
                }
            );
            saw_resize = true;
            break;
        }
    }
    assert!(saw_resize, "expected a Resize text frame from the bastion");

    bastion.shutdown();
}

/// SSH wire order: data, window-change, data. The proxy MUST emit
/// the corresponding WS frames in that exact order — Binary, Text,
/// Binary — so coordinate-sensitive backends (mouse SGR, paste, the
/// artboard) see the resize at the right moment in the byte stream.
///
/// This test guards against regression of the bastion's prior
/// design, which routed inbound bytes through `channel.into_stream`
/// (a separate queue) and resizes through `resize_tx` (another
/// queue), then `tokio::select!`'d between them — `select!` has no
/// fairness or arrival-order guarantee, so a [A, R, B] wire sequence
/// could surface as [R, AB] or [AB, R].
#[tokio::test]
async fn bastion_preserves_data_resize_data_order() {
    let mut backend = MockBackend::spawn().await.expect("backend");
    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    let (_session, channel) = open_user_session(bastion.addr).await;
    let _headers = backend.wait_for_handshake().await;
    backend.expect_setup_frames("xterm-256color", 80, 24).await;

    // Issue the three operations serially from one task. russh's
    // per-connection task dispatches the resulting SSH messages in
    // call order, so this models a real client doing data-resize-data.
    channel.data(&b"A"[..]).await.expect("data A");
    channel
        .window_change(132, 50, 0, 0)
        .await
        .expect("window_change");
    channel.data(&b"B"[..]).await.expect("data B");

    // Collect the three meaningful frames the bastion emits, skipping
    // any pings/pongs that ride along.
    let mut meaningful: Vec<WsMessage> = Vec::with_capacity(3);
    while meaningful.len() < 3 {
        let Some(frame) = backend.next_frame().await else {
            panic!("backend stream ended early; got {meaningful:?}");
        };
        match &frame {
            WsMessage::Binary(_) | WsMessage::Text(_) => meaningful.push(frame),
            _ => {}
        }
    }

    match &meaningful[0] {
        WsMessage::Binary(bytes) => assert_eq!(
            bytes.as_ref(),
            b"A",
            "first frame should be the 'A' Binary; got {:?}",
            bytes.as_ref()
        ),
        other => panic!("first frame should be Binary('A'); got {other:?}"),
    }

    match &meaningful[1] {
        WsMessage::Text(text) => {
            let parsed = ControlFrame::from_json(text.as_str()).expect("parse resize");
            assert_eq!(
                parsed,
                ControlFrame::Resize {
                    cols: 132,
                    rows: 50
                },
                "second frame should be the resize Text"
            );
        }
        other => panic!("second frame should be Text(resize); got {other:?}"),
    }

    match &meaningful[2] {
        WsMessage::Binary(bytes) => assert_eq!(
            bytes.as_ref(),
            b"B",
            "third frame should be the 'B' Binary; got {:?}",
            bytes.as_ref()
        ),
        other => panic!("third frame should be Binary('B'); got {other:?}"),
    }

    bastion.shutdown();
}

/// One accepted-or-rejected connection's behavior, used by ScriptedBackend
/// to drive multi-attempt reconnect tests.
#[derive(Clone, Debug)]
enum Behavior {
    /// Complete the WS upgrade, then immediately send a Close frame with
    /// the given code and exit. Used to exercise both retryable
    /// (4100-4199) and terminal dispatch paths.
    CloseWithCode(u16, &'static str),
    /// Complete the WS upgrade, then drop the transport without sending a
    /// WebSocket Close frame. Used to exercise the 1006-equivalent path.
    DropTransport,
    /// Reject the WS upgrade with the given HTTP status. Used to
    /// exercise the dial-error classifier (4xx → terminal, 5xx → retry).
    HttpReject(StatusCode),
    /// Complete the upgrade and hold the connection open (drain inbound,
    /// no outbound). Used as the "second attempt succeeds" terminator
    /// for retry-and-recover tests.
    AcceptAndHold,
    /// Complete the upgrade, then stop polling the WebSocket entirely
    /// for the given duration. Bastion-sent Pings accumulate in the
    /// TCP buffer with no auto-pong reply, so the bastion's silence
    /// detector should trip and treat the connection as dead.
    AcceptThenStall(Duration),
}

/// Mock backend that handles a *sequence* of connection attempts. Each
/// accepted TCP gets the next `Behavior` from the script; once the
/// script is exhausted, additional accepts (which we don't want to see
/// in terminal-close tests) get a default terminal close so the bastion
/// would-loop is observable without hanging.
///
/// All captured handshake headers go onto a single mpsc in accept order,
/// so a test can both (a) verify the second handshake's reconnect reason
/// header and (b) detect unwanted extra dials via `try_recv` timeout.
struct ScriptedBackend {
    addr: SocketAddr,
    handshakes_rx: mpsc::Receiver<HeaderMap>,
    cancel: CancellationToken,
    _join: tokio::task::JoinHandle<()>,
}

impl ScriptedBackend {
    async fn spawn(script: Vec<Behavior>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (handshakes_tx, handshakes_rx) = mpsc::channel::<HeaderMap>(16);
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let join = tokio::spawn(async move {
            let mut script = script;
            loop {
                tokio::select! {
                    _ = cancel_for_task.cancelled() => return,
                    accept = listener.accept() => {
                        let (tcp, _) = match accept {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        let behavior = if script.is_empty() {
                            Behavior::CloseWithCode(1000, "post-script")
                        } else {
                            script.remove(0)
                        };
                        let tx = handshakes_tx.clone();
                        tokio::spawn(handle_scripted(tcp, behavior, tx));
                    }
                }
            }
        });

        Ok(Self {
            addr,
            handshakes_rx,
            cancel,
            _join: join,
        })
    }

    fn ws_url(&self) -> String {
        format!("ws://{}/tunnel", self.addr)
    }

    async fn next_handshake(&mut self) -> Option<HeaderMap> {
        self.next_handshake_within(Duration::from_secs(5)).await
    }

    /// Like `next_handshake` but with a caller-chosen timeout, for
    /// scenarios where the bastion takes longer than the default 5s
    /// to redial (e.g. silence-detection paths that wait out a
    /// SILENCE_THRESHOLD-sized stall before retrying).
    async fn next_handshake_within(&mut self, dur: Duration) -> Option<HeaderMap> {
        timeout(dur, self.handshakes_rx.recv()).await.ok().flatten()
    }

    /// Assert no further handshake arrives within `wait`. Used by tests
    /// that expect the bastion to have given up reconnecting.
    async fn assert_no_redial(&mut self, wait: Duration) {
        let result = timeout(wait, self.handshakes_rx.recv()).await;
        if let Ok(Some(h)) = result {
            panic!("unexpected redial attempt; headers: {h:?}");
        }
    }

    fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Complete a WS upgrade, capture the request headers onto `tx`, and
/// return the WebSocketStream. Returns None on upgrade failure.
async fn accept_capture(
    tcp: TcpStream,
    tx: &mpsc::Sender<HeaderMap>,
) -> Option<tokio_tungstenite::WebSocketStream<TcpStream>> {
    let captured: Arc<std::sync::Mutex<Option<HeaderMap>>> = Default::default();
    let ws = tokio_tungstenite::accept_hdr_async(
        tcp,
        CaptureHeaders {
            captured: captured.clone(),
        },
    )
    .await
    .ok()?;
    let header_map = captured.lock().unwrap().take();
    if let Some(h) = header_map {
        let _ = tx.send(h).await;
    }
    Some(ws)
}

async fn handle_scripted(tcp: TcpStream, behavior: Behavior, tx: mpsc::Sender<HeaderMap>) {
    match behavior {
        Behavior::HttpReject(status) => {
            // accept_hdr_async lets the callback reject with an HTTP response.
            // The handshake headers ARE visible at callback time even when
            // we reject — capture them so the test can correlate the
            // attempt with this Behavior.
            let captured: Arc<std::sync::Mutex<Option<HeaderMap>>> = Default::default();
            let _ = tokio_tungstenite::accept_hdr_async(
                tcp,
                RejectWithStatus {
                    captured: captured.clone(),
                    status,
                },
            )
            .await;
            let header_map = captured.lock().unwrap().take();
            if let Some(h) = header_map {
                let _ = tx.send(h).await;
            }
        }
        Behavior::CloseWithCode(code, reason) => {
            let Some(ws) = accept_capture(tcp, &tx).await else {
                return;
            };
            let (mut sink, _stream) = ws.split();
            let frame = CloseFrame {
                code: CloseCode::from(code),
                reason: reason.into(),
            };
            let _ = sink.send(WsMessage::Close(Some(frame))).await;
            let _ = sink.close().await;
        }
        Behavior::DropTransport => {
            let Some(ws) = accept_capture(tcp, &tx).await else {
                return;
            };
            drop(ws);
        }
        Behavior::AcceptAndHold => {
            let Some(ws) = accept_capture(tcp, &tx).await else {
                return;
            };
            let (mut sink, mut stream) = ws.split();
            // Drain inbound until peer closes; don't send anything.
            while let Some(Ok(msg)) = stream.next().await {
                if matches!(msg, WsMessage::Close(_)) {
                    break;
                }
            }
            let _ = sink.close().await;
        }
        Behavior::AcceptThenStall(stall_for) => {
            let Some(ws) = accept_capture(tcp, &tx).await else {
                return;
            };
            // Hold the upgraded WebSocketStream alive but never poll
            // it. Bastion-sent Pings sit in the TCP recv buffer; no
            // pong is ever produced. The bastion's silence threshold
            // should trip, breaking its pump and triggering a redial.
            tokio::time::sleep(stall_for).await;
            drop(ws);
        }
    }
}

/// Open an SSH session against the bastion and advance through
/// handshake → pty → shell. Returns the live channel so the test can
/// hold it open while inspecting backend state.
async fn open_user_session(
    bastion_addr: SocketAddr,
) -> (
    russh_client::Handle<AnyHostKey>,
    russh::Channel<russh::client::Msg>,
) {
    let user_key =
        PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519).expect("key");
    let client_config = Arc::new(russh_client::Config::default());
    let mut session = russh_client::connect(client_config, bastion_addr, AnyHostKey)
        .await
        .expect("client connect");
    let auth = session
        .authenticate_publickey(
            "alice",
            PrivateKeyWithHashAlg::new(Arc::new(user_key), None),
        )
        .await
        .expect("authenticate_publickey");
    assert!(auth.success(), "auth not accepted");
    let channel = session
        .channel_open_session()
        .await
        .expect("channel_open_session");
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request_pty");
    channel.request_shell(true).await.expect("request_shell");
    (session, channel)
}

#[tokio::test]
async fn bastion_redials_after_retryable_close_with_reconnect_reason_header() {
    // First connection: backend closes 4100 → bastion should retry.
    // Second connection: backend holds → bastion proceeds normally.
    let mut backend = ScriptedBackend::spawn(vec![
        Behavior::CloseWithCode(4100, "upgrade requested"),
        Behavior::AcceptAndHold,
    ])
    .await
    .expect("backend");

    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    let (_session, _channel) = open_user_session(bastion.addr).await;

    // First handshake: fresh dial, no reconnect reason header.
    let h1 = backend
        .next_handshake()
        .await
        .expect("first handshake never arrived");
    let sid1 = h1
        .get(HEADER_SESSION_ID)
        .expect("session id on first dial")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        h1.get(HEADER_RECONNECT_REASON).is_none(),
        "first dial should not carry X-Late-Reconnect-Reason"
    );

    // Second handshake: same session_id, X-Late-Reconnect-Reason=4100.
    let h2 = backend
        .next_handshake()
        .await
        .expect("redial never arrived");
    assert_eq!(
        h2.get(HEADER_RECONNECT_REASON)
            .expect("reconnect reason header on second dial"),
        "4100",
        "redial should set X-Late-Reconnect-Reason=4100"
    );
    assert_eq!(
        h2.get(HEADER_SESSION_ID).unwrap().to_str().unwrap(),
        sid1,
        "session id should be stable across reconnects"
    );

    backend.shutdown();
    bastion.shutdown();
}

#[tokio::test]
async fn bastion_does_not_redial_after_terminal_close() {
    // Terminal close (4001 = kicked) should end the user's session
    // without further dial attempts.
    let mut backend = ScriptedBackend::spawn(vec![Behavior::CloseWithCode(4001, "kicked")])
        .await
        .expect("backend");

    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    let (_session, _channel) = open_user_session(bastion.addr).await;

    let _h1 = backend
        .next_handshake()
        .await
        .expect("first handshake never arrived");

    // Generous window: the backoff initial delay is 100ms, so a single
    // unwanted retry would land well within 2s.
    backend.assert_no_redial(Duration::from_secs(2)).await;

    backend.shutdown();
    bastion.shutdown();
}

#[tokio::test]
async fn bastion_redials_when_backend_goes_silent() {
    // First connection: backend accepts the upgrade then stalls. With
    // no auto-pong reply, the bastion's silence detector trips at
    // SILENCE_THRESHOLD (5s) and breaks its pump. The reconnect loop
    // then redials, hitting the AcceptAndHold terminator.
    let mut backend = ScriptedBackend::spawn(vec![
        Behavior::AcceptThenStall(Duration::from_secs(15)),
        Behavior::AcceptAndHold,
    ])
    .await
    .expect("backend");

    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");
    let (_session, _channel) = open_user_session(bastion.addr).await;

    // First handshake: fresh dial.
    let h1 = backend
        .next_handshake()
        .await
        .expect("first handshake never arrived");
    assert!(
        h1.get(HEADER_RECONNECT_REASON).is_none(),
        "first dial should not carry X-Late-Reconnect-Reason"
    );

    // After silence trip + redial, expect a second handshake within a
    // generous window (silence threshold ≈ 5s + redial fastpath).
    let h2 = backend
        .next_handshake_within(Duration::from_secs(8))
        .await
        .expect("redial after silence trip never arrived");
    assert_eq!(
        h2.get(HEADER_RECONNECT_REASON)
            .expect("reconnect reason header on redial"),
        "1006",
        "silence-triggered redial should set X-Late-Reconnect-Reason=1006"
    );

    backend.shutdown();
    bastion.shutdown();
}

#[tokio::test]
async fn bastion_writes_reconnect_message_during_long_outage() {
    // Trajectory:
    //   1. First WS upgrade succeeds; backend drops the transport.
    //      Pump fails fast; bastion enters its second 'session iter
    //      (is_redial = true).
    //   2. Mock backend rejects the next four dials with HTTP 503;
    //      backoff sleeps cumulate to ~700ms, crossing the 500ms
    //      visibility threshold. Bastion writes the plain-text
    //      reconnect message to the user's SSH stream.
    //   3. Fifth dial accepts and holds; recovery complete.
    let backend = ScriptedBackend::spawn(vec![
        Behavior::DropTransport,
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::AcceptAndHold,
    ])
    .await
    .expect("backend");

    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");
    let (_session, channel) = open_user_session(bastion.addr).await;

    // Read user-side bytes until we see the reconnect needle. The
    // mock backend doesn't emit any binary frames, so the only bytes
    // arriving here are the bastion's own reconnect-message write.
    let stream = channel.into_stream();
    let (mut reader, _writer) = tokio::io::split(stream);
    let mut accumulated: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let needle = "reconnecting to late.sh";
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            panic!(
                "timeout waiting for reconnect message; got: {:?}",
                String::from_utf8_lossy(&accumulated)
            );
        }
        let n = timeout(deadline - now, reader.read(&mut chunk))
            .await
            .expect("read deadline")
            .expect("read");
        if n == 0 {
            panic!(
                "ssh stream EOF before message; got: {:?}",
                String::from_utf8_lossy(&accumulated)
            );
        }
        accumulated.extend_from_slice(&chunk[..n]);
        if String::from_utf8_lossy(&accumulated).contains(needle) {
            break;
        }
    }

    let text = String::from_utf8_lossy(&accumulated);
    // Terminal reset must precede the message so the user's terminal
    // exits the previous TUI's alt-screen and clears formatting.
    assert!(
        text.contains("\x1b[?1049l"),
        "expected alt-screen exit; got: {text:?}"
    );
    assert!(
        text.contains("\x1b[2J"),
        "expected screen clear; got: {text:?}"
    );

    bastion.shutdown();
}

#[tokio::test]
async fn bastion_writes_update_wait_message_for_user_requested_redial() {
    let backend = ScriptedBackend::spawn(vec![
        Behavior::CloseWithCode(4100, "upgrade requested"),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::HttpReject(StatusCode::SERVICE_UNAVAILABLE),
        Behavior::AcceptAndHold,
    ])
    .await
    .expect("backend");

    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");
    let (_session, channel) = open_user_session(bastion.addr).await;

    let stream = channel.into_stream();
    let (mut reader, _writer) = tokio::io::split(stream);
    let mut accumulated: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let needle = "waiting for updated late.sh";
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            panic!(
                "timeout waiting for update wait message; got: {:?}",
                String::from_utf8_lossy(&accumulated)
            );
        }
        let n = timeout(deadline - now, reader.read(&mut chunk))
            .await
            .expect("read deadline")
            .expect("read");
        if n == 0 {
            panic!(
                "ssh stream EOF before update wait message; got: {:?}",
                String::from_utf8_lossy(&accumulated)
            );
        }
        accumulated.extend_from_slice(&chunk[..n]);
        if String::from_utf8_lossy(&accumulated).contains(needle) {
            break;
        }
    }

    let text = String::from_utf8_lossy(&accumulated);
    assert!(
        text.contains("\x1b[?1049l"),
        "expected alt-screen exit; got: {text:?}"
    );
    assert!(
        text.contains("\x1b[2J"),
        "expected screen clear; got: {text:?}"
    );

    bastion.shutdown();
}

#[tokio::test]
async fn bastion_does_not_redial_after_http_4xx_rejection() {
    // HTTP 401 on the upgrade should be classified as terminal.
    let mut backend = ScriptedBackend::spawn(vec![Behavior::HttpReject(StatusCode::UNAUTHORIZED)])
        .await
        .expect("backend");

    let bastion = TestBastion::spawn(backend.ws_url()).await.expect("bastion");

    let (_session, _channel) = open_user_session(bastion.addr).await;

    let _h1 = backend
        .next_handshake()
        .await
        .expect("first handshake never arrived");

    backend.assert_no_redial(Duration::from_secs(2)).await;

    backend.shutdown();
    bastion.shutdown();
}
