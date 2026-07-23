use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use late_core::audio::VizFrame;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, mpsc::Sender};
use uuid::Uuid;

use crate::authz::Permissions;

// WebSocket → SSH session routing for browser-sent visualization data and
// other inbound SSH-side effects. The matching outbound channel (mute,
// volume, clipboard request, source selection) lives in `paired_clients.rs`.
//
// Flow:
//   Browser (WS) sends Heartbeat + Viz frames
//     → API/WS handler looks up token
//       → SessionRegistry sends SessionMessage over mpsc
//         → ssh.rs receives and forwards into App
//           → App drops Viz payloads (the sidebar wave is synthetic; the
//             variant survives for old clients until the pipeline removal
//             in VIZ_WAVE_BRIEF.md lands)

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
    Toast {
        message: String,
        error: bool,
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
    UltimateCast {
        ultimate_id: String,
        seed: u64,
        duration_ms: u64,
    },
    UltimateCooldownUpdated {
        ultimate_id: String,
        remaining_ms: u64,
    },
    UltimateCooldownDbRereadOk {
        cooldowns: Vec<(String, u64)>,
    },
    UltimateCastRejected {
        ultimate_id: String,
        remaining_ms: u64,
    },
}

struct SessionEntry {
    tx: Sender<SessionMessage>,
    user_id: Uuid,
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, SessionEntry>>>,
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

    pub async fn register(&self, token: String, tx: Sender<SessionMessage>, user_id: Uuid) {
        tracing::info!(token_hint = %token_hint(&token), "registered cli session token");
        let mut sessions = self.sessions.write().await;
        sessions.insert(token, SessionEntry { tx, user_id });
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

    /// Look up the user_id associated with a paired session token. Returns
    /// None if the session has disconnected since the WS pair handshake
    /// started.
    pub async fn user_for(&self, token: &str) -> Option<Uuid> {
        let sessions = self.sessions.read().await;
        sessions.get(token).map(|entry| entry.user_id)
    }

    pub async fn send_message(&self, token: &str, msg: SessionMessage) -> bool {
        // 1. Get the Sender (holding read lock)
        let tx = {
            let sessions = self.sessions.read().await;
            sessions.get(token).map(|entry| entry.tx.clone())
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
#[path = "session_test.rs"]
mod session_test;
