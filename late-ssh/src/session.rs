use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use late_core::MutexRecover;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{RwLock, mpsc::Sender, mpsc::UnboundedSender};
use uuid::Uuid;

// WebSocket → SSH session routing for browser-sent visualization data.
//
// Flow:
//   Browser (WS) sends Heartbeat + Viz frames
//     → API/WS handler looks up token
//       → SessionRegistry sends SessionMessage over mpsc
//         → ssh.rs receives and forwards into App
//           → App updates visualizer buffer used by TUI render

#[derive(Debug, Clone)]
pub struct BrowserVizFrame {
    pub bands: [f32; 8],
    pub rms: f32,
    pub position_ms: u64,
}

#[derive(Debug, Clone)]
pub enum SessionMessage {
    Heartbeat,
    Viz(BrowserVizFrame),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Browser,
    Cli,
    #[default]
    Unknown,
}

impl ClientKind {
    pub fn label(self) -> &'static str {
        match self {
            ClientKind::Browser => "Browser",
            ClientKind::Cli => "CLI",
            ClientKind::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientAudioState {
    pub client_kind: ClientKind,
    pub muted: bool,
    pub volume_percent: u8,
}

impl Default for ClientAudioState {
    fn default() -> Self {
        Self {
            client_kind: ClientKind::Unknown,
            muted: false,
            volume_percent: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, Sender<SessionMessage>>>>,
}

#[derive(Clone, Default)]
pub struct PairedClientRegistry {
    clients: Arc<Mutex<HashMap<String, PairControlEntry>>>,
    next_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PairControlEntry {
    registration_id: u64,
    tx: UnboundedSender<PairControlMessage>,
    state: ClientAudioState,
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

impl PairedClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, token: String, tx: UnboundedSender<PairControlMessage>) -> u64 {
        let registration_id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut clients = self.clients.lock_recover();
        if let Some(previous) = clients.get(&token) {
            // Legitimate reconnects hit this path; a surprise overwrite with an
            // unknown peer would indicate token takeover, so surface it loudly.
            tracing::warn!(
                token_hint = %token_hint(&token),
                previous_registration_id = previous.registration_id,
                registration_id,
                "paired client registration replaced existing entry"
            );
        } else {
            tracing::info!(
                token_hint = %token_hint(&token),
                registration_id,
                "registered paired client session"
            );
        }
        clients.insert(
            token,
            PairControlEntry {
                registration_id,
                tx,
                state: ClientAudioState::default(),
            },
        );
        registration_id
    }

    pub fn unregister_if_match(&self, token: &str, registration_id: u64) {
        let mut clients = self.clients.lock_recover();
        let should_remove = clients
            .get(token)
            .map(|entry| entry.registration_id == registration_id)
            .unwrap_or(false);
        if should_remove {
            tracing::info!(
                token_hint = %token_hint(token),
                registration_id,
                "unregistered paired client session"
            );
            clients.remove(token);
        }
    }

    pub fn send_control(&self, token: &str, msg: PairControlMessage) -> bool {
        let tx = {
            let clients = self.clients.lock().unwrap_or_else(|e| {
                tracing::warn!("paired client registry mutex poisoned, recovering");
                e.into_inner()
            });
            clients.get(token).map(|entry| entry.tx.clone())
        };

        if let Some(tx) = tx {
            if tx.send(msg).is_ok() {
                return true;
            }
            tracing::warn!(
                token_hint = %token_hint(token),
                "failed to send paired client control message"
            );
            return false;
        }

        tracing::warn!(
            token_hint = %token_hint(token),
            "no paired client found for control message"
        );
        false
    }

    pub fn update_state(&self, token: &str, registration_id: u64, state: ClientAudioState) {
        let mut clients = self.clients.lock_recover();
        if let Some(entry) = clients.get_mut(token)
            && entry.registration_id == registration_id
        {
            entry.state = state;
        }
    }

    pub fn snapshot(&self, token: &str) -> Option<ClientAudioState> {
        let clients = self.clients.lock_recover();
        clients.get(token).map(|entry| entry.state.clone())
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

        let frame = BrowserVizFrame {
            bands: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            rms: 0.5,
            position_ms: 1000,
        };
        let sent = registry
            .send_message("tok1", SessionMessage::Viz(frame))
            .await;
        assert!(sent);

        match rx.recv().await.unwrap() {
            SessionMessage::Viz(f) => {
                assert_eq!(f.rms, 0.5);
                assert_eq!(f.position_ms, 1000);
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

    #[test]
    fn paired_client_send_control_delivers_message() {
        let registry = PairedClientRegistry::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        registry.register("tok1".to_string(), tx);

        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    }

    #[test]
    fn paired_client_unregister_if_match_respects_latest_registration() {
        let registry = PairedClientRegistry::new();
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
        let first = registry.register("tok1".to_string(), tx1);
        let second = registry.register("tok1".to_string(), tx2);

        registry.unregister_if_match("tok1", first);

        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert_eq!(rx2.try_recv().unwrap(), PairControlMessage::ToggleMute);
        registry.unregister_if_match("tok1", second);
        assert!(!registry.send_control("tok1", PairControlMessage::ToggleMute));
    }

    #[test]
    fn paired_client_snapshot_tracks_latest_state() {
        let registry = PairedClientRegistry::new();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let registration_id = registry.register("tok1".to_string(), tx);
        registry.update_state(
            "tok1",
            registration_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                muted: true,
                volume_percent: 35,
            },
        );

        let snapshot = registry.snapshot("tok1").unwrap();
        assert_eq!(snapshot.client_kind, ClientKind::Cli);
        assert!(snapshot.muted);
        assert_eq!(snapshot.volume_percent, 35);
    }
}
