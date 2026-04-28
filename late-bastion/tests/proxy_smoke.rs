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
//! No reconnect-loop / close-code coverage here — that's Phase 4.

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use getrandom::SysRng;
use late_bastion::config::Config;
use late_bastion::ssh::{Server, load_or_generate_key};
use late_core::shutdown::CancellationToken;
use late_core::tunnel_protocol::{
    ControlFrame, HEADER_COLS, HEADER_FINGERPRINT, HEADER_PEER_IP, HEADER_ROWS, HEADER_SECRET,
    HEADER_SESSION_ID, HEADER_TERM,
};
use russh::client::{self as russh_client, Handler as ClientHandler};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, signature::rand_core::UnwrapErr};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderMap;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

const TEST_SECRET: &str = "test-secret";

/// Channel a test pushes onto to send a binary frame to the user; mock
/// backend forwards it onto the actual WS sink.
type BackendSendTx = mpsc::Sender<WsMessage>;

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
            let captured_for_callback = captured.clone();

            let ws = match tokio_tungstenite::accept_hdr_async(
                tcp,
                |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                    *captured_for_callback.lock().unwrap() = Some(req.headers().clone());
                    Ok(resp)
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
            proxy_protocol: false,
            proxy_trusted_cidrs: vec![],
        });

        let host_key = load_or_generate_key(&config.host_key_path)?;
        let russh_config = Arc::new(russh::server::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            auth_rejection_time: Duration::from_secs(1),
            keys: vec![host_key],
            ..Default::default()
        });

        let server = Server::new(config.clone());
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        let (tcp, peer) = match accept {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        let cfg = russh_config.clone();
                        let mut s = server.clone();
                        tokio::spawn(async move {
                            let handler = russh::server::Server::new_client(&mut s, Some(peer));
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
        .request_pty(false, "xterm-256color", 100, 30, 0, 0, &[])
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
    assert_eq!(headers.get(HEADER_TERM).unwrap(), "xterm-256color");
    assert_eq!(headers.get(HEADER_COLS).unwrap(), "100");
    assert_eq!(headers.get(HEADER_ROWS).unwrap(), "30");
    // Loopback: peer_ip captured by bastion is 127.0.0.1 (no PROXY v1).
    assert_eq!(headers.get(HEADER_PEER_IP).unwrap(), "127.0.0.1");
    // session_id is a UUID — just assert the header is present and non-empty.
    let sid = headers
        .get(HEADER_SESSION_ID)
        .expect("session id header")
        .to_str()
        .unwrap();
    assert!(!sid.is_empty(), "session id non-empty");

    // Take the channel as a stream and exchange bytes.
    let stream = channel.into_stream();
    let (mut user_reader, mut user_writer) = tokio::io::split(stream);

    // User → backend.
    user_writer.write_all(b"abc").await.expect("user write");
    user_writer.flush().await.ok();

    let frame = backend.next_frame().await.expect("backend frame");
    match frame {
        WsMessage::Binary(bytes) => assert_eq!(bytes.as_ref(), b"abc"),
        other => panic!("expected user bytes as Binary, got {other:?}"),
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
        .request_pty(false, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request_pty");
    channel.request_shell(true).await.expect("request_shell");

    // Drain handshake.
    let _ = backend.wait_for_handshake().await;

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
