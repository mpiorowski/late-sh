use crate::{AppState, app, config::Config};
use axum::{Json, http::StatusCode, routing::get};
use late_core::db::{Db, DbConfig};
use tokio::sync::oneshot;

// The listen routes touch no DB, so the inert lazy pool is the sanctioned
// stand-in here (see the test-layout rules in the root CONTEXT.md).
fn test_state(ssh_internal_url: String) -> AppState {
    let config = Config {
        env: crate::config::Env::Dev,
        port: 0,
        ssh_internal_url,
        audio_base_url: "http://127.0.0.1:9".to_string(),
        db: DbConfig::default(),
    };
    AppState {
        config,
        db: Db::new(&DbConfig::default()).expect("lazy db"),
        http_client: reqwest::Client::new(),
    }
}

async fn spawn_app(ssh_internal_url: String) -> (String, oneshot::Sender<()>) {
    let app = app(test_state(ssh_internal_url));
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

/// Stands in for late-ssh's `/api/listen`.
async fn spawn_upstream() -> (String, oneshot::Sender<()>) {
    let app = axum::Router::new().route(
        "/api/listen",
        get(|| async {
            Json(serde_json::json!({
                "listeners": 7,
                "audio_mode": "youtube",
                "streams": { "chill": { "artist": "Ketsa", "title": "Blue Dream" } },
                "stations": {
                    "chillsynth": {
                        "artist": "Waveshaper",
                        "title": "Client",
                        "stream_url": "https://stream.nightride.fm/chillsynth.mp3"
                    }
                },
                "youtube": { "current": null, "queue": [] }
            }))
        }),
    );

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
async fn listen_page_renders_without_a_token() {
    let (base_url, shutdown_tx) = spawn_app("http://127.0.0.1:9".to_string()).await;

    let response = reqwest::Client::new()
        .get(format!("{}/listen", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    // The page must keep steering people at the terminal first.
    assert!(body.contains("ssh late.sh"), "listen page: {body}");

    // Product rule: the house stream sits last. Guest stations and the
    // community queue lead, our own playlist is the fallback option.
    let nightride = body.find("nightride fm").expect("nightride section");
    let community = body.find("community queue").expect("community section");
    let house = body.find("house radio").expect("house section");
    assert!(nightride < house, "house radio must not lead the page");
    assert!(community < house, "house radio must sit below the queue");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn listen_state_forwards_the_upstream_body_verbatim() {
    let (upstream_base_url, upstream_shutdown_tx) = spawn_upstream().await;
    let (base_url, shutdown_tx) = spawn_app(upstream_base_url).await;

    let response = reqwest::Client::new()
        .get(format!("{}/listen/state", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();

    assert_eq!(body["listeners"], 7);
    assert_eq!(body["streams"]["chill"]["title"], "Blue Dream");
    assert_eq!(
        body["stations"]["chillsynth"]["stream_url"],
        "https://stream.nightride.fm/chillsynth.mp3"
    );
    assert!(body["youtube"]["queue"].is_array());

    let _ = shutdown_tx.send(());
    let _ = upstream_shutdown_tx.send(());
}
