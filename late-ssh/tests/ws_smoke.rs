mod helpers;

use helpers::{new_test_db, test_app_state, test_config};
use late_ssh::api::run_api_server_with_listener;
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
        .register("tok-one".to_string(), session_tx_one)
        .await;
    let (session_tx_two, _rx_two) = tokio::sync::mpsc::channel(1);
    state
        .session_registry
        .register("tok-two".to_string(), session_tx_two)
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
