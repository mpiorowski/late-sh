use crate::api::run_api_server_with_listener;
use crate::test_helpers::{new_test_db, test_app_state, test_config};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

#[tokio::test]
async fn ws_pair_endpoint_rate_limits_repeated_attempts_from_same_ip() {
    let test_db = new_test_db().await;
    let mut config = test_config(test_db.db.config().clone());
    config.ws_pair_max_attempts_per_ip = 1;
    let state = test_app_state(test_db.db.clone(), config);

    let (session_tx_one, _rx_one) = tokio::sync::mpsc::channel(1);
    state
        .session_registry
        .register("tok-one".to_string(), session_tx_one, uuid::Uuid::now_v7())
        .await;
    let (session_tx_two, _rx_two) = tokio::sync::mpsc::channel(1);
    state
        .session_registry
        .register("tok-two".to_string(), session_tx_two, uuid::Uuid::now_v7())
        .await;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let api_task = tokio::spawn(async move {
        let _ = run_api_server_with_listener(listener, state, None).await;
    });

    let first_status = ws_upgrade_status_with_retry(addr, "tok-one", 10)
        .await
        .expect("first ws upgrade");
    assert_eq!(first_status, 101);

    let second_status = ws_upgrade_status_with_retry(addr, "tok-two", 10)
        .await
        .expect("second ws upgrade");
    assert_eq!(second_status, 429);

    api_task.abort();
}

#[tokio::test]
async fn ws_pair_endpoint_rejects_unknown_token() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let api_task = tokio::spawn(async move {
        let _ = run_api_server_with_listener(listener, state, None).await;
    });

    let status = ws_upgrade_status_with_retry(addr, "never-registered", 10)
        .await
        .expect("ws upgrade");
    assert_eq!(status, 404);

    api_task.abort();
}

/// The listen page is unauthenticated, and the response is a published
/// contract: internal snapshot fields (history, skip progress, submitter ids,
/// vote scores) must not ride along just because someone swapped the handler
/// back to serializing `QueueSnapshot` directly.
#[tokio::test]
async fn listen_endpoint_is_public_and_hides_internal_snapshot_fields() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let api_task = tokio::spawn(async move {
        let _ = run_api_server_with_listener(listener, state, None).await;
    });

    let (status, body) = http_get_with_retry(addr, "/api/listen", 10)
        .await
        .expect("listen request");
    assert_eq!(status, 200);

    let json: serde_json::Value = serde_json::from_str(&body).expect("listen json");
    assert!(json.get("audio_mode").is_some());
    assert!(json.get("listeners").is_some());
    assert!(json["streams"].is_object());
    assert!(json["stations"].is_object());
    assert!(json["youtube"].get("current").is_some());
    assert!(json["youtube"]["queue"].is_array());

    for leaked in ["history", "skip_progress", "submitter_id", "vote_score"] {
        assert!(
            !body.contains(leaked),
            "public listen response leaked internal field {leaked}: {body}"
        );
    }

    api_task.abort();
}

#[tokio::test]
async fn stream_endpoints_serve_the_watch_and_publish_flow() {
    let test_db = new_test_db().await;
    let mut config = test_config(test_db.db.config().clone());
    config.voice = crate::app::voice::svc::VoiceConfig::enabled(
        "wss://rtc.test".to_string(),
        "test-key".to_string(),
        "test-secret".to_string(),
        "late-voice".to_string(),
    )
    .expect("voice config");
    let state = test_app_state(test_db.db.clone(), config);

    let client = test_db.db.get().await.expect("db client");
    let user = late_core::models::user::User::create(
        &client,
        late_core::models::user::UserParams {
            fingerprint: "stream-test-fp".to_string(),
            username: "streamer".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user");
    drop(client);
    let mut events = state.stream_service.subscribe_events();
    state.stream_service.go_live_task(
        user.id,
        "streamer".to_string(),
        Some("demo show".to_string()),
    );
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("go live event in time")
        .expect("go live event");
    let (publish_url, watch_url) = match event {
        crate::app::stream::svc::StreamEvent::GoLiveReady {
            publish_url,
            watch_url,
            ..
        } => (publish_url, watch_url),
        other => panic!("expected GoLiveReady, got {other:?}"),
    };
    let publish_token = publish_url.rsplit('/').next().expect("token").to_string();
    let stream_id = watch_url.rsplit('/').next().expect("stream id").to_string();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let api_task = tokio::spawn(async move {
        let _ = run_api_server_with_listener(listener, state, None).await;
    });

    // Pending stream: the watch URL resolves but is not live yet.
    let (status, body) = http_get_with_retry(addr, &format!("/api/stream/watch/{stream_id}"), 10)
        .await
        .expect("watch state");
    assert_eq!(status, 200);
    assert!(body.contains("\"live\":false"), "pending stream: {body}");
    assert!(body.contains("demo show"));
    // No subscribe grant while pending: nobody listens to the room's voice
    // channel before media has actually flowed.
    let (status, _) = http_get_with_retry(addr, &format!("/api/stream/watch/{stream_id}/grant"), 3)
        .await
        .expect("pending watch grant");
    assert_eq!(status, 404);

    // The publisher page fetches its grant; the first fetch claims the
    // token and the minted secret rides back in a header (the late-web
    // proxy turns it into the console's cookie).
    let (status, head, body) =
        http_get_with_header(addr, &format!("/api/stream/publish/{publish_token}"), None)
            .await
            .expect("publish grant");
    assert_eq!(status, 200);
    assert!(body.contains("\"streamer\":\"streamer\""), "grant: {body}");
    assert!(body.contains("\"token\":"));
    let claim = head
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with("x-late-publish-claim:")
        })
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_string())
        .expect("claim header on the first grant fetch");

    // A leaked publish URL replayed without the claim is refused, grant
    // and state report alike: claim-once is the anti-hijack lock.
    let (status, _, _) =
        http_get_with_header(addr, &format!("/api/stream/publish/{publish_token}"), None)
            .await
            .expect("replayed grant");
    assert_eq!(status, 403);
    let (status, _) = http_post_json(
        addr,
        &format!("/api/stream/publish/{publish_token}/state"),
        "{\"publishing\":false,\"mic_live\":false}",
    )
    .await
    .expect("unclaimed state report");
    assert_eq!(status, 403);

    // The claiming console reports media flowing.
    let (status, _) = http_post_json_with_header(
        addr,
        &format!("/api/stream/publish/{publish_token}/state"),
        "{\"publishing\":true,\"mic_live\":false}",
        Some(("x-late-publish-claim", &claim)),
    )
    .await
    .expect("publish state report");
    assert_eq!(status, 204);

    // Watchers now see it live, get a subscribe grant, and count via
    // heartbeats.
    let (status, body) = http_get_with_retry(addr, &format!("/api/stream/watch/{stream_id}"), 3)
        .await
        .expect("watch state");
    assert_eq!(status, 200);
    assert!(body.contains("\"live\":true"), "live stream: {body}");
    let (status, body) =
        http_get_with_retry(addr, &format!("/api/stream/watch/{stream_id}/grant"), 3)
            .await
            .expect("watch grant");
    assert_eq!(status, 200);
    assert!(body.contains("\"livekit_url\":\"wss://rtc.test\""));
    let (status, _) = http_post_json(
        addr,
        &format!("/api/stream/watch/{stream_id}/heartbeat"),
        "{\"watcher_id\":\"viewer-1\"}",
    )
    .await
    .expect("watch heartbeat");
    assert_eq!(status, 204);
    let (status, body) = http_get_with_retry(addr, &format!("/api/stream/watch/{stream_id}"), 3)
        .await
        .expect("watch state");
    assert_eq!(status, 200);
    assert!(body.contains("\"watching\":1"), "watcher count: {body}");
    // Watcher ids are client-generated (a browser UUID); junk beyond the
    // cap is rejected at the boundary and never reaches the registry.
    let long_id = "x".repeat(65);
    let (status, _) = http_post_json(
        addr,
        &format!("/api/stream/watch/{stream_id}/heartbeat"),
        &format!("{{\"watcher_id\":\"{long_id}\"}}"),
    )
    .await
    .expect("oversized heartbeat");
    assert_eq!(status, 400);

    // Dead capability ids answer 404, never a grant.
    let (status, _) = http_get_with_retry(addr, "/api/stream/watch/unknown-id", 3)
        .await
        .expect("unknown watch state");
    assert_eq!(status, 404);
    let (status, _) = http_get_with_retry(addr, "/api/stream/publish/unknown-token", 3)
        .await
        .expect("unknown publish grant");
    assert_eq!(status, 404);

    api_task.abort();
}

/// GET that also returns the raw response head, for header assertions.
async fn http_get_with_header(
    addr: SocketAddr,
    path: &str,
    header: Option<(&str, &str)>,
) -> std::io::Result<(u16, String, String)> {
    let extra = header
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         {extra}Connection: close\r\n\
         \r\n",
        host = addr
    );
    http_exchange(addr, request).await
}

async fn http_post_json_with_header(
    addr: SocketAddr,
    path: &str,
    body: &str,
    header: Option<(&str, &str)>,
) -> std::io::Result<(u16, String)> {
    let extra = header
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         {extra}Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        host = addr,
        len = body.len(),
    );
    let (status, _, body) = http_exchange(addr, request).await?;
    Ok((status, body))
}

async fn http_exchange(
    addr: SocketAddr,
    request: String,
) -> std::io::Result<(u16, String, String)> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.write_all(request.as_bytes()).await?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let response = String::from_utf8_lossy(&raw).into_owned();
    let status = response
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let (head, body) = response
        .split_once("\r\n\r\n")
        .map(|(head, body)| (head.to_string(), body.to_string()))
        .unwrap_or((response.clone(), String::new()));
    Ok((status, head, body))
}

async fn http_post_json(
    addr: SocketAddr,
    path: &str,
    body: &str,
) -> std::io::Result<(u16, String)> {
    let mut stream = TcpStream::connect(addr).await?;
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        host = addr,
        len = body.len(),
    );
    stream.write_all(request.as_bytes()).await?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let response = String::from_utf8_lossy(&raw).into_owned();
    let status = response
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    Ok((status, body))
}

async fn http_get_with_retry(
    addr: SocketAddr,
    path: &str,
    retries: usize,
) -> std::io::Result<(u16, String)> {
    let mut last_err = None;
    for attempt in 0..retries {
        match http_get(addr, path).await {
            Ok(response) => return Ok(response),
            Err(err) => {
                if attempt + 1 == retries {
                    return Err(err);
                }
                last_err = Some(err);
                sleep(Duration::from_millis(20)).await;
            }
        }
    }
    Err(last_err.expect("last error"))
}

/// `Connection: close` so the server hangs up after the body and a plain
/// read-to-end sees the whole response.
async fn http_get(addr: SocketAddr, path: &str) -> std::io::Result<(u16, String)> {
    let mut stream = TcpStream::connect(addr).await?;
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\
         \r\n",
        host = addr
    );
    stream.write_all(request.as_bytes()).await?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let response = String::from_utf8_lossy(&raw).into_owned();
    let status = response
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    Ok((status, body))
}

async fn ws_upgrade_status_with_retry(
    addr: SocketAddr,
    token: &str,
    retries: usize,
) -> std::io::Result<u16> {
    let mut last_err = None;
    for attempt in 0..retries {
        match ws_upgrade_status(addr, token).await {
            Ok(status) => return Ok(status),
            Err(err) => {
                if attempt + 1 == retries {
                    return Err(err);
                }
                last_err = Some(err);
                sleep(Duration::from_millis(20)).await;
            }
        }
    }
    Err(last_err.expect("last error"))
}

async fn ws_upgrade_status(addr: SocketAddr, token: &str) -> std::io::Result<u16> {
    let mut stream = TcpStream::connect(addr).await?;
    let request = format!(
        "GET /api/ws/pair?token={token} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
        host = addr
    );
    stream.write_all(request.as_bytes()).await?;

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    let first_line = response.lines().next().unwrap_or_default();
    let status = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    Ok(status)
}
