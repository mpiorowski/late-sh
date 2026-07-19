use crate::{AppState, app, config::Config};
use axum::{Json, routing::get};
use late_core::db::{Db, DbConfig};
use serde_json::json;
use std::sync::Once;
use tokio::sync::oneshot;
use tokio::time::{Duration, sleep};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

fn init_test_tracing() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let _ = tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .try_init();
    });
}

fn test_state(ssh_internal_url: String) -> AppState {
    let config = Config {
        port: 0,
        ssh_internal_url,
        ssh_public_url: "localhost:3000".to_string(),
        audio_base_url: "http://localhost:8000".to_string(),
        web_tunnel_token: "test-web-tunnel-token".to_string(),
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

async fn spawn_now_playing_server() -> (String, oneshot::Sender<()>) {
    async fn now_playing() -> Json<serde_json::Value> {
        Json(json!({
            "listeners_count": 42
        }))
    }

    let app = axum::Router::new().route("/api/now-playing", get(now_playing));
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

async fn wait_for_ok(client: &reqwest::Client, url: &str) {
    for _ in 0..20 {
        if let Ok(resp) = client.get(url).send().await
            && resp.status().is_success()
        {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become ready at {url}");
}

#[tokio::test]
async fn dashboard_page_renders_expected_fields() {
    init_test_tracing();
    let client = reqwest::Client::new();
    let (now_playing_url, now_playing_shutdown_tx) = spawn_now_playing_server().await;
    let (base_url, shutdown_tx) = spawn_app(now_playing_url).await;
    let url = format!("{}/dashboard", base_url);

    wait_for_ok(&client, &url).await;

    let body = client.get(url).send().await.unwrap().text().await.unwrap();

    assert!(body.contains("Dashboard"));
    assert!(body.contains("Mat's Stream"));
    assert!(body.contains("42"));
    assert!(body.contains("LIVE"));

    let _ = shutdown_tx.send(());
    let _ = now_playing_shutdown_tx.send(());
}

#[tokio::test]
async fn status_partial_renders_percent_metrics() {
    init_test_tracing();
    let client = reqwest::Client::new();
    let (base_url, shutdown_tx) = spawn_app("http://127.0.0.1:9".to_string()).await;
    let url = format!("{}/dashboard/status", base_url);

    wait_for_ok(&client, &url).await;

    let body = client.get(url).send().await.unwrap().text().await.unwrap();

    assert!(body.contains("System Metrics"));

    // The metric values are generated, so only the shape is contractual:
    // the partial shows at least four percentages and they are percentages.
    let percents = extract_percent_values(&body);
    assert!(percents.len() >= 4);
    assert!(percents.iter().all(|value| *value <= 100));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn now_playing_partial_renders_live_listener_data() {
    init_test_tracing();
    let client = reqwest::Client::new();
    let (now_playing_url, now_playing_shutdown_tx) = spawn_now_playing_server().await;
    let (base_url, shutdown_tx) = spawn_app(now_playing_url).await;
    let url = format!("{}/dashboard/now-playing", base_url);

    wait_for_ok(&client, &url).await;

    let body = client.get(url).send().await.unwrap().text().await.unwrap();

    assert!(body.contains("42"));
    assert!(body.contains("LIVE"));

    let _ = shutdown_tx.send(());
    let _ = now_playing_shutdown_tx.send(());
}

fn extract_percent_values(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut values = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len()
                && bytes[i] == b'%'
                && let Ok(value) = input[start..i].parse::<u8>()
            {
                values.push(value);
            }
        } else {
            i += 1;
        }
    }
    values
}
