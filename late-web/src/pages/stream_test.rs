use crate::{AppState, app, config::Config};
use axum::{body::Body, http::StatusCode, response::IntoResponse, routing::get};
use late_core::db::{Db, DbConfig};
use std::time::Duration;
use tokio::sync::oneshot;

fn test_state(audio_base_url: String) -> AppState {
    let config = Config {
        port: 0,
        ssh_internal_url: "http://127.0.0.1:9".to_string(),
        ssh_public_url: "localhost:4000".to_string(),
        audio_base_url,
        web_tunnel_token: "test-web-tunnel-token".to_string(),
    };
    AppState {
        config,
        db: Db::new(&DbConfig::default()).expect("lazy db"),
        http_client: reqwest::Client::new(),
    }
}

async fn spawn_app(audio_base_url: String) -> (String, oneshot::Sender<()>) {
    let app = app(test_state(audio_base_url));
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

async fn spawn_upstream(body: &'static [u8]) -> (String, oneshot::Sender<()>) {
    async fn stream_handler() -> impl IntoResponse {
        (StatusCode::OK, Body::from("upstream-audio"))
    }

    let app = if body == b"upstream-audio" {
        axum::Router::new().route("/chill", get(stream_handler))
    } else {
        let bytes = body.to_vec();
        axum::Router::new().route(
            "/chill",
            get(move || {
                let bytes = bytes.clone();
                async move { (StatusCode::OK, Body::from(bytes)) }
            }),
        )
    };

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

#[tokio::test]
async fn stream_proxy_passthroughs_upstream_audio() {
    let client = reqwest::Client::new();
    let (upstream_base_url, upstream_shutdown_tx) = spawn_upstream(b"upstream-audio").await;
    let (base_url, shutdown_tx) = spawn_app(upstream_base_url).await;

    let response = client
        .get(format!("{}/stream", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let mut response = response;
    let first = tokio::time::timeout(Duration::from_secs(2), response.chunk())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(first.as_ref(), b"upstream-audio");

    let _ = shutdown_tx.send(());
    let _ = upstream_shutdown_tx.send(());
}

#[tokio::test]
async fn stream_proxy_falls_back_to_silence_when_upstream_is_down() {
    let client = reqwest::Client::new();
    let (base_url, shutdown_tx) = spawn_app("http://127.0.0.1:9".to_string()).await;

    let response = client
        .get(format!("{}/stream", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let mut response = response;

    // The fallback loops the silence asset forever; read one full cycle so a
    // truncated response cannot pass on a short prefix match.
    let silence = include_bytes!("../../assets/silence.mp3");
    let mut collected = Vec::new();
    while collected.len() < silence.len() {
        let chunk = tokio::time::timeout(Duration::from_secs(2), response.chunk())
            .await
            .expect("chunk timeout")
            .expect("chunk read")
            .expect("stream ended before a full silence cycle");
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(&collected[..silence.len()], silence.as_slice());

    let _ = shutdown_tx.send(());
}
