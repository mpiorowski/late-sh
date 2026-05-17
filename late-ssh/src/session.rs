use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use late_core::audio::VizFrame;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, mpsc::Sender};
use uuid::Uuid;

use crate::authz::Permissions;

// WebSocket → SSH session routing for browser-sent visualization data and
// other inbound SSH-side effects. The matching outbound channel (mute,
// volume, clipboard request, force-mute) lives in `paired_clients.rs`.
//
// Flow:
//   Browser (WS) sends Heartbeat + Viz frames
//     → API/WS handler looks up token
//       → SessionRegistry sends SessionMessage over mpsc
//         → ssh.rs receives and forwards into App
//           → App updates visualizer buffer used by TUI render

#[derive(Debug, Clone)]
pub enum SessionMessage {
    Heartbeat,
    Viz(VizFrame),
    ClipboardImage {
        data: Vec<u8>,
    },
    ClipboardImageFailed {
        message: String,
    },
    Terminate {
        reason: String,
    },
    ArtboardBanChanged {
        banned: bool,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    PermissionsChanged {
        permissions: Permissions,
    },
    RoomRemoved {
        room_id: Uuid,
        slug: String,
        message: String,
    },
    /// A browser just attached on this session token. The SSH side responds
    /// by pushing the user's stored audio source so a refreshed page lands
    /// in the right mode.
    BrowserPaired,
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, Sender<SessionMessage>>>>,
}

pub fn new_session_token() -> String {
    compact_uuid(Uuid::now_v7())
}

fn compact_uuid(uuid: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(uuid.as_bytes())
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, token: String, tx: Sender<SessionMessage>) {
        tracing::info!(token_hint = %token_hint(&token), "registered cli session token");
        let mut sessions = self.sessions.write().await;
        sessions.insert(token, tx);
    }

    pub async fn unregister(&self, token: &str) {
        tracing::info!(token_hint = %token_hint(token), "unregistered cli session token");
        let mut sessions = self.sessions.write().await;
        sessions.remove(token);
    }

    pub async fn has_session(&self, token: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(token)
    }

    pub async fn send_message(&self, token: &str, msg: SessionMessage) -> bool {
        // 1. Get the Sender (holding read lock)
        let tx = {
            let sessions = self.sessions.read().await;
            sessions.get(token).cloned()
        }; // Lock dropped here

        // 2. Send (async, no lock held)
        if let Some(tx) = tx {
            match tx.send(msg).await {
                Ok(_) => true,
                Err(e) => {
                    tracing::error!(error = ?e, "failed to send session message");
                    false
                }
            }
        } else {
            tracing::warn!(
                token_hint = %token_hint(token),
                "no session found for message"
            );
            false
        }
    }
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_send() {
        let registry = SessionRegistry::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        registry.register("tok1".to_string(), tx).await;

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
        registry.register("tok1".to_string(), tx).await;
        assert!(registry.has_session("tok1").await);

        registry.unregister("tok1").await;
        assert!(!registry.has_session("tok1").await);
    }

    #[tokio::test]
    async fn unregister_removes_session() {
        let registry = SessionRegistry::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        registry.register("tok1".to_string(), tx).await;
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
        registry.register("tok1".to_string(), tx1).await;
        registry.register("tok1".to_string(), tx2).await;

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
        registry.register("tok1".to_string(), tx).await;

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
        registry.register("tok1".to_string(), tx).await;
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
}
