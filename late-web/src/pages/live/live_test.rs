use super::valid_capability_id;

/// Regression: `send_traced` folds upstream non-2xx into `Err`
/// (`error_for_status`), and the proxies used to surface that as a late-web
/// 500. The watch/go-live pages key "stream is gone" off a 404, so the
/// upstream status must survive the hop.
#[tokio::test]
async fn proxies_forward_upstream_404_instead_of_500() {
    use axum::http::StatusCode;
    use late_core::db::{Db, DbConfig};

    // Upstream that answers 404 to everything, like late-ssh does for a
    // dead capability id.
    let upstream = axum::Router::new().fallback(|| async { StatusCode::NOT_FOUND });
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(upstream_listener, upstream).await;
    });

    let state = crate::AppState {
        config: crate::config::Config {
            port: 0,
            ssh_internal_url: format!("http://{upstream_addr}"),
            audio_base_url: "http://127.0.0.1:9".to_string(),
        },
        db: Db::new(&DbConfig::default()).expect("lazy db"),
        http_client: reqwest::Client::new(),
    };
    let app = crate::app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let client = reqwest::Client::new();
    for path in [
        "/live/deadbeef/state",
        "/live/deadbeef/grant",
        "/golive/deadbeef/grant",
    ] {
        let response = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .expect("proxy get");
        assert_eq!(response.status(), 404, "GET {path}");
    }
    let response = client
        .post(format!("http://{addr}/golive/deadbeef/state"))
        .json(&serde_json::json!({ "publishing": true, "mic_live": false }))
        .send()
        .await
        .expect("proxy post");
    assert_eq!(response.status(), 404, "POST publish state");
}

#[test]
fn capability_ids_are_hex_dash_tokens_only() {
    assert!(valid_capability_id("0198a2f4c3f07f4e8a7bde12ab34cd56"));
    assert!(valid_capability_id("abc-123"));

    // Anything that could reshape the proxied internal path is rejected.
    assert!(!valid_capability_id(""));
    assert!(!valid_capability_id(".."));
    assert!(!valid_capability_id("a/b"));
    assert!(!valid_capability_id("a?b=1"));
    assert!(!valid_capability_id("a%2e%2e"));
    assert!(!valid_capability_id(&"a".repeat(65)));
}

#[test]
fn watch_and_golive_pages_render_with_the_id_embedded() {
    use askama::Template;

    let watch = super::WatchPage {
        stream_id: "abc123",
    }
    .render()
    .expect("watch page renders");
    assert!(watch.contains("abc123"));
    assert!(
        watch.contains("muted"),
        "the watch page must be born silent"
    );

    let golive = super::GoLivePage {
        publish_token: "tok456",
    }
    .render()
    .expect("golive page renders");
    assert!(golive.contains("tok456"));
    assert!(
        golive.contains("mic: off"),
        "the go-live page mic starts off"
    );
}
