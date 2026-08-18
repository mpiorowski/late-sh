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
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: format!("http://{upstream_addr}"),
            audio_base_url: "http://127.0.0.1:9".to_string(),
            db: DbConfig::default(),
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
        "/live/deadbeef/grant?watcher_id=abc-123",
        "/golive/deadbeef/grant",
    ] {
        let response = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .expect("proxy get");
        assert_eq!(response.status(), 404, "GET {path}");
    }

    // The watcher id is interpolated into the internal query string and
    // becomes a LiveKit identity upstream, so an absent or off-shape one is
    // refused here instead of being forwarded.
    for path in [
        "/live/deadbeef/grant",
        "/live/deadbeef/grant?watcher_id=",
        "/live/deadbeef/grant?watcher_id=a%2Fb",
    ] {
        let response = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .expect("proxy get");
        assert_eq!(response.status(), 400, "GET {path}");
    }
    let response = client
        .post(format!("http://{addr}/golive/deadbeef/state"))
        .json(&serde_json::json!({ "publishing": true, "mic_live": false }))
        .send()
        .await
        .expect("proxy post");
    assert_eq!(response.status(), 404, "POST publish state");
}

/// The claim-once cookie exchange: the upstream's claim header becomes an
/// HttpOnly path-scoped cookie on the console's browser, and the browser's
/// cookie rides back upstream as the claim header.
#[tokio::test]
async fn golive_grant_proxy_exchanges_the_claim_cookie() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use late_core::db::{Db, DbConfig};

    // Upstream echoes the presented claim and mints one when absent, like
    // the late-ssh grant endpoint.
    let upstream = axum::Router::new().route(
        "/api/stream/publish/{token}",
        axum::routing::get(|headers: axum::http::HeaderMap| async move {
            let presented = headers
                .get("x-late-publish-claim")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            match presented {
                Some(claim) if claim == "secret-1" => {
                    (StatusCode::OK, axum::Json(serde_json::json!({"ok": true}))).into_response()
                }
                Some(_) => StatusCode::FORBIDDEN.into_response(),
                None => {
                    let mut response =
                        (StatusCode::OK, axum::Json(serde_json::json!({"ok": true})))
                            .into_response();
                    response.headers_mut().insert(
                        "x-late-publish-claim",
                        axum::http::HeaderValue::from_static("secret-1"),
                    );
                    response
                }
            }
        }),
    );
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(upstream_listener, upstream).await;
    });

    let state = crate::AppState {
        config: crate::config::Config {
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: format!("http://{upstream_addr}"),
            audio_base_url: "http://127.0.0.1:9".to_string(),
            db: DbConfig::default(),
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
    // First (claiming) fetch: no cookie in, HttpOnly path-scoped cookie out.
    let response = client
        .get(format!("http://{addr}/golive/tok123/grant"))
        .send()
        .await
        .expect("claiming grant");
    assert_eq!(response.status(), 200);
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("set-cookie on the claiming fetch");
    assert!(set_cookie.contains("golive_claim=secret-1"), "{set_cookie}");
    assert!(set_cookie.contains("Path=/golive/tok123"), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");

    // The cookie rides back upstream as the claim header.
    let response = client
        .get(format!("http://{addr}/golive/tok123/grant"))
        .header("cookie", "golive_claim=secret-1")
        .send()
        .await
        .expect("claimed grant");
    assert_eq!(response.status(), 200);
    assert!(response.headers().get("set-cookie").is_none());

    // A wrong cookie is forwarded and the upstream 403 survives the hop.
    let response = client
        .get(format!("http://{addr}/golive/tok123/grant"))
        .header("cookie", "golive_claim=stolen")
        .send()
        .await
        .expect("denied grant");
    assert_eq!(response.status(), 403);
}

#[test]
fn capability_ids_are_base64url_tokens_only() {
    // What late-ssh actually mints: base64url over 16 random bytes. Both
    // non-alphanumeric characters of that alphabet have to pass, or a stream
    // whose id happens to contain one 404s on its own watch page.
    assert!(valid_capability_id("HdRl3AJfRhWc7-BdvUEFrQ"));
    assert!(valid_capability_id("HdRl3AJfRhWc7_BdvUEFrQ"));
    // A browser `randomUUID` watcher id, and the hex form minted before.
    assert!(valid_capability_id("0198a2f4c3f07f4e8a7bde12ab34cd56"));
    assert!(valid_capability_id("abc-123"));

    // Anything that could reshape the proxied internal path is rejected.
    assert!(!valid_capability_id(""));
    assert!(!valid_capability_id(".."));
    assert!(!valid_capability_id("a/b"));
    assert!(!valid_capability_id("a?b=1"));
    assert!(!valid_capability_id("a%2e%2e"));
    // Base64 padding is not part of the no-pad alphabet, and `+`/`/` are the
    // standard-alphabet characters this deliberately does not accept.
    assert!(!valid_capability_id("HdRl3AJfRhWc7+BdvUEFrQ=="));
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
    // Audio defaults ON (autoplay permitting), voices separately togglable
    // and also on; the button labels are what the viewer sees.
    assert!(
        watch.contains("mute room audio"),
        "room audio starts on, so the button offers mute"
    );
    assert!(watch.contains("voices: on"), "voices toggle starts on");
    assert!(watch.contains("id=\"volume\""), "stream volume slider");
    assert!(
        watch.contains("id=\"fullscreen-btn\""),
        "fullscreen control"
    );
    // The grant fetch carries the page's watcher id, so every retry from
    // one viewer joins LiveKit under the same identity.
    assert!(
        watch.contains("grant?watcher_id="),
        "the grant fetch carries the watcher id"
    );
    // A dropped connection must be recoverable in place: telling the viewer
    // to reload was a dead end the page could never leave on its own.
    assert!(
        !watch.contains("reload to retry"),
        "a lost connection reconnects instead of demanding a reload"
    );

    let golive = super::GoLivePage {
        publish_token: "tok456",
    }
    .render()
    .expect("golive page renders");
    assert!(golive.contains("tok456"));
    // Voice is CLI-only with no exceptions: the console page has no
    // browser mic at all, and publishes nothing until the share click.
    assert!(
        !golive.contains("mic-btn") && !golive.contains("setMicrophoneEnabled"),
        "the go-live page must have no browser mic"
    );
    assert!(
        golive.contains("room audio: muted"),
        "console room audio starts muted"
    );
    // The SFU re-sends the publisher's bitrate once per viewer, so the
    // publish ceiling is the fan-out bill. Left unset it is the SDK's
    // 2.5 Mbps screen-share default, which nothing in our stack caps.
    assert!(
        golive.contains("maxBitrate: 1500000"),
        "the screen share publishes under an explicit bitrate cap"
    );
}
