use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use late_core::{
    db::{Db, DbConfig},
    tunnel_protocol::{
        HEADER_COLS, HEADER_FINGERPRINT, HEADER_ROWS, HEADER_SECRET, HEADER_USERNAME,
        HEADER_VIEW_ONLY,
    },
};
use late_web::{AppState, app, config::Config};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;

#[derive(Debug)]
struct HandshakeSnapshot {
    secret: String,
    username: String,
    fingerprint: String,
    view_only: String,
    cols: String,
    rows: String,
}

#[derive(Clone)]
struct FakeTunnelState {
    handshake_tx: mpsc::Sender<HandshakeSnapshot>,
    text_tx: mpsc::Sender<String>,
}

fn test_state(tunnel_url: String) -> AppState {
    let config = Config {
        port: 0,
        ssh_internal_url: "http://127.0.0.1:9".to_string(),
        ssh_public_url: "localhost:4000".to_string(),
        audio_base_url: "http://localhost:8000".to_string(),
        tunnel_url,
        tunnel_shared_secret: "test-secret".to_string(),
        spectator_username: "spectator".to_string(),
        spectator_fingerprint: "web-spectator:v1".to_string(),
        spectator_default_cols: 120,
        spectator_default_rows: 40,
        spectator_max_cols: 240,
        spectator_max_rows: 90,
    };
    AppState {
        config,
        db: Db::new(&DbConfig::default()).expect("lazy db"),
        http_client: reqwest::Client::new(),
    }
}

async fn spawn_app(tunnel_url: String) -> (String, oneshot::Sender<()>) {
    let app = app(test_state(tunnel_url));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        let _ = server.await;
    });

    (base_url, shutdown_tx)
}

async fn spawn_fake_tunnel() -> (
    String,
    mpsc::Receiver<HandshakeSnapshot>,
    mpsc::Receiver<String>,
    oneshot::Sender<()>,
) {
    async fn tunnel_handler(
        ws: WebSocketUpgrade,
        headers: HeaderMap,
        State(state): State<FakeTunnelState>,
    ) -> impl IntoResponse {
        let snapshot = HandshakeSnapshot {
            secret: header(&headers, HEADER_SECRET),
            username: header(&headers, HEADER_USERNAME),
            fingerprint: header(&headers, HEADER_FINGERPRINT),
            view_only: header(&headers, HEADER_VIEW_ONLY),
            cols: header(&headers, HEADER_COLS),
            rows: header(&headers, HEADER_ROWS),
        };
        let _ = state.handshake_tx.send(snapshot).await;
        ws.on_upgrade(move |socket| fake_tunnel_session(socket, state.text_tx))
    }

    let (handshake_tx, handshake_rx) = mpsc::channel(1);
    let (text_tx, text_rx) = mpsc::channel(1);
    let state = FakeTunnelState {
        handshake_tx,
        text_tx,
    };
    let app = Router::new()
        .route("/tunnel", get(tunnel_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let tunnel_url = format!("ws://{}/tunnel", addr);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        let _ = server.await;
    });

    (tunnel_url, handshake_rx, text_rx, shutdown_tx)
}

fn header(headers: &HeaderMap, name: &'static str) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

async fn fake_tunnel_session(mut socket: WebSocket, text_tx: mpsc::Sender<String>) {
    let _ = socket
        .send(Message::Binary(bytes::Bytes::from_static(
            b"\x1b[Hhello spectator",
        )))
        .await;

    while let Some(msg) = socket.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let _ = text_tx.send(text.to_string()).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
}

#[tokio::test]
async fn spectate_ws_proxies_with_view_only_handshake() {
    let (tunnel_url, mut handshake_rx, mut text_rx, tunnel_shutdown_tx) = spawn_fake_tunnel().await;
    let (base_url, app_shutdown_tx) = spawn_app(tunnel_url).await;
    let ws_url = format!(
        "{}/ws/spectate?cols=999&rows=0",
        base_url.replace("http://", "ws://")
    );

    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .expect("spectate websocket should connect");

    let handshake = timeout(Duration::from_secs(2), handshake_rx.recv())
        .await
        .expect("handshake captured")
        .expect("handshake sent");
    assert_eq!(handshake.secret, "test-secret");
    assert_eq!(handshake.username, "spectator");
    assert_eq!(handshake.fingerprint, "web-spectator:v1");
    assert_eq!(handshake.view_only, "1");
    assert_eq!(handshake.cols, "240");
    assert_eq!(handshake.rows, "1");

    let frame = timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("frame proxied")
        .expect("frame present")
        .expect("frame ok");
    assert_eq!(
        frame,
        TungsteniteMessage::Binary(bytes::Bytes::from_static(b"\x1b[Hhello spectator"))
    );

    ws.send(TungsteniteMessage::Text(
        r#"{"t":"resize","cols":100,"rows":30}"#.into(),
    ))
    .await
    .expect("resize should send");
    let text = timeout(Duration::from_secs(2), text_rx.recv())
        .await
        .expect("resize forwarded")
        .expect("resize present");
    assert_eq!(text, r#"{"t":"resize","cols":100,"rows":30}"#);

    let _ = ws.close(None).await;
    let _ = app_shutdown_tx.send(());
    let _ = tunnel_shutdown_tx.send(());
}
