use super::*;

#[tokio::test]
async fn register_and_send() {
    let registry = SessionRegistry::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    registry
        .register("tok1".to_string(), tx, Uuid::now_v7())
        .await;

    let sent = registry
        .send_message("tok1", SessionMessage::Heartbeat)
        .await;
    assert!(sent);

    let msg = rx.recv().await.unwrap();
    assert!(matches!(msg, SessionMessage::Heartbeat));
}

#[tokio::test]
async fn send_to_unknown_returns_false() {
    let registry = SessionRegistry::new();
    let sent = registry
        .send_message("unknown", SessionMessage::Heartbeat)
        .await;
    assert!(!sent);
}

#[tokio::test]
async fn has_session_reflects_registration() {
    let registry = SessionRegistry::new();
    assert!(!registry.has_session("tok1").await);

    let (tx, _rx) = tokio::sync::mpsc::channel(10);
    registry
        .register("tok1".to_string(), tx, Uuid::now_v7())
        .await;
    assert!(registry.has_session("tok1").await);

    registry.unregister("tok1").await;
    assert!(!registry.has_session("tok1").await);
}

#[tokio::test]
async fn unregister_removes_session() {
    let registry = SessionRegistry::new();
    let (tx, _rx) = tokio::sync::mpsc::channel(10);
    registry
        .register("tok1".to_string(), tx, Uuid::now_v7())
        .await;
    registry.unregister("tok1").await;

    let sent = registry
        .send_message("tok1", SessionMessage::Heartbeat)
        .await;
    assert!(!sent);
}

#[tokio::test]
async fn register_overwrites_existing() {
    let registry = SessionRegistry::new();
    let (tx1, _rx1) = tokio::sync::mpsc::channel(10);
    let (tx2, mut rx2) = tokio::sync::mpsc::channel(10);
    registry
        .register("tok1".to_string(), tx1, Uuid::now_v7())
        .await;
    registry
        .register("tok1".to_string(), tx2, Uuid::now_v7())
        .await;

    let sent = registry
        .send_message("tok1", SessionMessage::Heartbeat)
        .await;
    assert!(sent);
    let msg = rx2.recv().await.unwrap();
    assert!(matches!(msg, SessionMessage::Heartbeat));
}

#[tokio::test]
async fn send_viz_frame() {
    let registry = SessionRegistry::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    registry
        .register("tok1".to_string(), tx, Uuid::now_v7())
        .await;

    let frame = VizFrame {
        bands: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        rms: 0.5,
        track_pos_ms: 1000,
    };
    let sent = registry
        .send_message("tok1", SessionMessage::Viz(frame))
        .await;
    assert!(sent);

    match rx.recv().await.unwrap() {
        SessionMessage::Viz(f) => {
            assert_eq!(f.rms, 0.5);
            assert_eq!(f.track_pos_ms, 1000);
        }
        _ => panic!("expected Viz message"),
    }
}

#[tokio::test]
async fn send_fails_when_receiver_dropped() {
    let registry = SessionRegistry::new();
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    registry
        .register("tok1".to_string(), tx, Uuid::now_v7())
        .await;
    drop(rx);

    let sent = registry
        .send_message("tok1", SessionMessage::Heartbeat)
        .await;
    assert!(!sent);
}

#[test]
fn token_hint_redacts_full_value() {
    assert_eq!(super::token_hint("abcdefgh-ijkl"), "abcdefgh..(13)");
}

#[test]
fn new_session_token_is_compact_urlsafe_base64() {
    let token = new_session_token();

    assert_eq!(token.len(), 22);
    assert!(
        token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    );

    let decoded = URL_SAFE_NO_PAD.decode(token.as_bytes()).unwrap();
    assert_eq!(decoded.len(), 16);
}
