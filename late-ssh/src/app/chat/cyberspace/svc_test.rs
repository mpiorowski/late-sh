use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use late_core::models::cyberspace_account::CyberspaceAccount;
use late_core::test_utils::{create_test_user, test_db};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::app::chat::cyberspace::svc::{CsEvent, CyberspaceService};

/// A base URL nothing listens on: any code path that unexpectedly touches
/// the network fails fast instead of calling the real cyberspace API.
fn dead_service(db: late_core::db::Db) -> CyberspaceService {
    CyberspaceService::new(db, "http://127.0.0.1:1".to_string())
}

async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<CsEvent>) -> CsEvent {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("event within timeout")
        .expect("channel open")
}

#[tokio::test]
async fn session_init_reports_unlinked_users_without_network() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "cs-fresh").await;
    let service = dead_service(test_db.db.clone());
    let mut rx = service.subscribe_events();

    service.session_init_task(user.id);

    match next_event(&mut rx).await {
        CsEvent::LinkStatus {
            user_id,
            username,
            feed_read_at,
            circ_rooms,
            circ_room_reads,
        } => {
            assert_eq!(user_id, user.id);
            assert_eq!(username, None);
            assert_eq!(feed_read_at, None);
            assert!(circ_rooms.is_empty());
            assert!(circ_room_reads.is_empty());
        }
        other => panic!("expected LinkStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn session_init_reports_the_linked_username() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-linked").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    let service = dead_service(test_db.db.clone());
    let mut rx = service.subscribe_events();

    service.session_init_task(user.id);

    match next_event(&mut rx).await {
        CsEvent::LinkStatus {
            user_id,
            username,
            feed_read_at,
            circ_rooms,
            circ_room_reads,
        } => {
            assert_eq!(user_id, user.id);
            assert_eq!(username.as_deref(), Some("odd"));
            assert_eq!(feed_read_at, None, "a fresh link has read nothing yet");
            assert!(circ_rooms.is_empty(), "a fresh link pins no chat rooms");
            assert!(
                circ_room_reads.is_empty(),
                "a fresh link has read no chat rooms"
            );
        }
        other => panic!("expected LinkStatus, got {other:?}"),
    }
}

/// A one-endpoint stand-in for their auth API: every request is answered
/// with a valid refresh envelope, and the caller counts how many arrived.
async fn fake_refresh_server() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake cyberspace server");
    let addr = listener.local_addr().expect("listener addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let accept_hits = Arc::clone(&hits);
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(serve_refresh_requests(socket, Arc::clone(&accept_hits)));
        }
    });
    (format!("http://{addr}"), hits)
}

/// Answer every HTTP request on one connection, counting each. Requests are
/// counted rather than connections because a keep-alive client can send all
/// of its refreshes down a single socket.
async fn serve_refresh_requests(mut socket: tokio::net::TcpStream, hits: Arc<AtomicUsize>) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let headers_end = loop {
            match buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                Some(position) => break position + 4,
                None => match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                },
            }
        };
        let content_length = String::from_utf8_lossy(&buffer[..headers_end])
            .to_lowercase()
            .lines()
            .find_map(|line| line.strip_prefix("content-length:")?.trim().parse().ok())
            .unwrap_or(0usize);
        while buffer.len() < headers_end + content_length {
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            }
        }
        buffer.drain(..headers_end + content_length);
        hits.fetch_add(1, Ordering::SeqCst);
        let body = r#"{"data":{"idToken":"tok-fresh","rtdbUrl":"http://127.0.0.1:1"}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        if socket.write_all(response.as_bytes()).await.is_err() {
            return;
        }
    }
}

#[tokio::test]
async fn concurrent_token_requests_share_one_refresh_call() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-refresh-dedup").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");

    let (base_url, hits) = fake_refresh_server().await;
    let service = CyberspaceService::new(test_db.db.clone(), base_url);

    // Opening a room fires history, stream, and presence at once, and each
    // asks for a token. A cold cache must produce one refresh, not three.
    let (a, b, c) = tokio::join!(
        service.id_token(user.id),
        service.id_token(user.id),
        service.id_token(user.id),
    );
    assert_eq!(a.expect("first token"), "tok-fresh");
    assert_eq!(b.expect("second token"), "tok-fresh");
    assert_eq!(c.expect("third token"), "tok-fresh");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "three concurrent callers must share one refresh POST"
    );
}

#[tokio::test]
async fn unlink_forgets_the_stored_account() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-unlinker").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    let service = dead_service(test_db.db.clone());
    let mut rx = service.subscribe_events();

    service.unlink_task(user.id);

    match next_event(&mut rx).await {
        CsEvent::Unlinked { user_id } => assert_eq!(user_id, user.id),
        other => panic!("expected Unlinked, got {other:?}"),
    }
    assert!(
        CyberspaceAccount::find_by_user_id(&client, user.id)
            .await
            .expect("find")
            .is_none()
    );
}
