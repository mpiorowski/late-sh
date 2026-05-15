use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use late_core::{MutexRecover, audio::VizFrame};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{RwLock, mpsc::Sender, mpsc::UnboundedSender};
use uuid::Uuid;

use crate::app::audio::client_state::{ClientAudioState, ClientKind};
use crate::authz::Permissions;
use crate::metrics;

// WebSocket → SSH session routing for browser-sent visualization data.
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
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    RequestClipboardImage,
    ForceMute { mute: bool },
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, Sender<SessionMessage>>>>,
}

#[derive(Clone, Default)]
pub struct PairedClientRegistry {
    clients: Arc<Mutex<HashMap<String, Vec<PairControlEntry>>>>,
    next_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PairControlEntry {
    registration_id: u64,
    tx: UnboundedSender<PairControlMessage>,
    state: ClientAudioState,
    usage_total_recorded: bool,
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
        let entries = clients.entry(token.clone()).or_default();
        tracing::info!(
            token_hint = %token_hint(&token),
            registration_id,
            prior_entries = entries.len(),
            "registered paired client session"
        );
        entries.push(PairControlEntry {
            registration_id,
            tx,
            state: ClientAudioState::default(),
            usage_total_recorded: false,
        });
        registration_id
    }

    /// Remove the matching entry. Returns the removed entry's `client_kind` and
    /// the number of browser entries remaining on the token afterward, so the
    /// caller can decide whether to relax a server-imposed CLI mute.
    pub fn unregister_if_match(
        &self,
        token: &str,
        registration_id: u64,
    ) -> Option<UnregisterResult> {
        let mut clients = self.clients.lock_recover();
        let entries = clients.get_mut(token)?;
        let position = entries
            .iter()
            .position(|entry| entry.registration_id == registration_id)?;
        let removed = entries.remove(position);
        if let Some((ssh_mode, platform)) = removed.state.cli_usage_labels() {
            metrics::add_cli_pair_active(-1, ssh_mode, platform);
        }
        let removed_kind = removed.state.client_kind;
        let browsers_remaining = entries
            .iter()
            .filter(|entry| entry.state.client_kind == ClientKind::Browser)
            .count();
        tracing::info!(
            token_hint = %token_hint(token),
            registration_id,
            ?removed_kind,
            browsers_remaining,
            "unregistered paired client session"
        );
        if entries.is_empty() {
            clients.remove(token);
        }
        Some(UnregisterResult {
            removed_kind,
            browsers_remaining,
        })
    }

    /// Broadcast a control message to every paired client of `token`. Returns
    /// the number of entries that accepted the message.
    pub fn send_control(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, |_| true) > 0
    }

    /// Send a control message to paired entries whose `client_kind` matches the
    /// predicate. Used to target CLI-only force-mute or browser-only controls.
    /// Returns the number of entries that accepted the message.
    pub fn send_control_filter<F>(
        &self,
        token: &str,
        msg: PairControlMessage,
        mut matches: F,
    ) -> usize
    where
        F: FnMut(ClientKind) -> bool,
    {
        let targets: Vec<UnboundedSender<PairControlMessage>> = {
            let clients = self.clients.lock_recover();
            clients
                .get(token)
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|entry| matches(entry.state.client_kind))
                        .map(|entry| entry.tx.clone())
                        .collect()
                })
                .unwrap_or_default()
        };

        if targets.is_empty() {
            return 0;
        }

        let mut delivered = 0;
        for tx in targets {
            if tx.send(msg.clone()).is_ok() {
                delivered += 1;
            } else {
                tracing::warn!(
                    token_hint = %token_hint(token),
                    "failed to send paired client control message"
                );
            }
        }
        delivered
    }

    /// Update the audio state of a single entry. Returns the state of the entry
    /// *after* the update plus the count of browser entries currently paired on
    /// this token, so the caller can decide whether to push ForceMute.
    pub fn update_state(
        &self,
        token: &str,
        registration_id: u64,
        state: ClientAudioState,
    ) -> Option<UpdateStateResult> {
        let mut clients = self.clients.lock_recover();
        let entries = clients.get_mut(token)?;
        let entry = entries
            .iter_mut()
            .find(|entry| entry.registration_id == registration_id)?;

        let previous_kind = entry.state.client_kind;
        let previous_labels = entry.state.cli_usage_labels();
        let new_labels = state.cli_usage_labels();

        if previous_labels != new_labels {
            if let Some((ssh_mode, platform)) = previous_labels {
                metrics::add_cli_pair_active(-1, ssh_mode, platform);
            }
            if let Some((ssh_mode, platform)) = new_labels {
                metrics::add_cli_pair_active(1, ssh_mode, platform);
            }
        }

        if !entry.usage_total_recorded
            && let Some((ssh_mode, platform)) = new_labels
        {
            metrics::record_cli_pair_usage(ssh_mode, platform);
            entry.usage_total_recorded = true;
        }

        entry.state = state;
        let new_kind = entry.state.client_kind;

        let browsers_total = entries
            .iter()
            .filter(|entry| entry.state.client_kind == ClientKind::Browser)
            .count();

        Some(UpdateStateResult {
            previous_kind,
            new_kind,
            browsers_total,
        })
    }

    /// Snapshot the state of the most recently registered entry, preferring a
    /// browser if one is present. Callers that need the SSH user's own paired
    /// client (typically a browser) use this to inspect mute/volume state.
    pub fn snapshot(&self, token: &str) -> Option<ClientAudioState> {
        let clients = self.clients.lock_recover();
        let entries = clients.get(token)?;
        entries
            .iter()
            .rev()
            .find(|entry| entry.state.client_kind == ClientKind::Browser)
            .or_else(|| entries.last())
            .map(|entry| entry.state.clone())
    }

    pub fn has_browser(&self, token: &str) -> bool {
        let clients = self.clients.lock_recover();
        clients
            .get(token)
            .map(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.state.client_kind == ClientKind::Browser)
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnregisterResult {
    pub removed_kind: ClientKind,
    pub browsers_remaining: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct UpdateStateResult {
    pub previous_kind: ClientKind,
    pub new_kind: ClientKind,
    pub browsers_total: usize,
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::audio::client_state::{ClientKind, ClientPlatform, ClientSshMode};

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

    #[test]
    fn paired_client_send_control_delivers_message() {
        let registry = PairedClientRegistry::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        registry.register("tok1".to_string(), tx);

        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    }

    #[test]
    fn paired_client_unregister_if_match_removes_only_matching_entry() {
        let registry = PairedClientRegistry::new();
        let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
        let first = registry.register("tok1".to_string(), tx1);
        let second = registry.register("tok1".to_string(), tx2);

        registry.unregister_if_match("tok1", first);

        // Only the surviving entry should receive subsequent broadcasts.
        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert!(rx1.try_recv().is_err());
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
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Macos,
                capabilities: vec!["clipboard_image".to_string()],
                muted: true,
                volume_percent: 35,
            },
        );

        let snapshot = registry.snapshot("tok1").unwrap();
        assert_eq!(snapshot.client_kind, ClientKind::Cli);
        assert_eq!(snapshot.ssh_mode, ClientSshMode::Native);
        assert_eq!(snapshot.platform, ClientPlatform::Macos);
        assert!(snapshot.supports_clipboard_image());
        assert!(snapshot.muted);
        assert_eq!(snapshot.volume_percent, 35);
    }
}
