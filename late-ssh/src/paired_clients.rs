use late_core::MutexRecover;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::audio::client_state::{ClientAudioState, ClientKind};
use crate::metrics;

// Multiplexed outbound channel to every paired client (browser + CLI) for a
// given SSH session token. Carries audio control (mute/volume/force-mute) and
// clipboard fan-out. The registry owns the "browser is the audio-output
// priority" policy: when a browser appears on a token, every CLI on that
// token is force-muted; when the last browser leaves, the CLI mute is
// relaxed.

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

    /// Remove the matching entry and, if doing so leaves the token with zero
    /// browsers after removing a browser, atomically relaxes the server-imposed
    /// CLI mute on the same token. Holding a single lock across removal and
    /// CLI-sender collection closes the race where a new browser could register
    /// between the two steps and have its ForceMute clobbered by a stale unmute.
    pub fn unregister_if_match(
        &self,
        token: &str,
        registration_id: u64,
    ) -> Option<UnregisterResult> {
        let (result, cli_senders_to_unmute) = {
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
            let cli_senders = if removed_kind == ClientKind::Browser && browsers_remaining == 0 {
                entries
                    .iter()
                    .filter(|entry| entry.state.client_kind == ClientKind::Cli)
                    .map(|entry| entry.tx.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            tracing::info!(
                token_hint = %token_hint(token),
                registration_id,
                ?removed_kind,
                browsers_remaining,
                relax_cli_mute = !cli_senders.is_empty(),
                "unregistered paired client session"
            );
            if entries.is_empty() {
                clients.remove(token);
            }
            (
                UnregisterResult {
                    removed_kind,
                    browsers_remaining,
                },
                cli_senders,
            )
        };

        for tx in cli_senders_to_unmute {
            if tx
                .send(PairControlMessage::ForceMute { mute: false })
                .is_err()
            {
                tracing::warn!(
                    token_hint = %token_hint(token),
                    "failed to relax CLI mute after browser disconnect"
                );
            }
        }

        Some(result)
    }

    /// Broadcast a control message to every paired client of `token`. Returns
    /// the number of entries that accepted the message.
    pub fn send_control(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, |_| true) > 0
    }

    /// Send a control message to paired entries whose `client_kind` matches the
    /// predicate. Used to target CLI-only force-mute or browser-only controls.
    /// Returns the number of entries that accepted the message.
    fn send_control_filter<F>(
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

    /// Apply a state update for a single entry and atomically enforce the
    /// browser-priority mute policy under the same lock. Returns the update
    /// outcome for callers that need it; the policy side-effects (ForceMute to
    /// every CLI on the token) are dispatched after the lock is released.
    ///
    /// Policy:
    /// - A browser just appeared on this token (transition into Browser kind) —
    ///   every CLI on the token gets ForceMute { mute: true }.
    /// - A CLI just identified itself with a browser already paired AND the CLI
    ///   does not already report `muted == true` — same. The `muted` guard
    ///   stops a WS reconnect from overriding a state the CLI is already in
    ///   (e.g. user-initiated local mute).
    ///
    /// Holding the lock across the decision closes the same TOCTOU window that
    /// `unregister_if_match` closes on the disconnect side.
    pub fn update_state_and_enforce_mute_policy(
        &self,
        token: &str,
        registration_id: u64,
        new_state: ClientAudioState,
    ) -> Option<UpdateStateResult> {
        let (result, cli_senders_to_mute) = {
            let mut clients = self.clients.lock_recover();
            let entries = clients.get_mut(token)?;
            let entry = entries
                .iter_mut()
                .find(|entry| entry.registration_id == registration_id)?;

            let previous_kind = entry.state.client_kind;
            let previous_labels = entry.state.cli_usage_labels();
            let new_labels = new_state.cli_usage_labels();

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

            let new_kind = new_state.client_kind;
            let new_muted = new_state.muted;
            entry.state = new_state;

            let browsers_total = entries
                .iter()
                .filter(|entry| entry.state.client_kind == ClientKind::Browser)
                .count();

            let browser_just_appeared =
                new_kind == ClientKind::Browser && previous_kind != ClientKind::Browser;
            let cli_joined_with_browser =
                new_kind == ClientKind::Cli && browsers_total > 0 && !new_muted;

            let cli_senders = if browser_just_appeared || cli_joined_with_browser {
                entries
                    .iter()
                    .filter(|entry| entry.state.client_kind == ClientKind::Cli)
                    .map(|entry| entry.tx.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            (
                UpdateStateResult {
                    previous_kind,
                    new_kind,
                    browsers_total,
                },
                cli_senders,
            )
        };

        for tx in cli_senders_to_mute {
            if tx
                .send(PairControlMessage::ForceMute { mute: true })
                .is_err()
            {
                tracing::warn!(
                    token_hint = %token_hint(token),
                    "failed to enforce CLI force-mute"
                );
            }
        }

        Some(result)
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

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::audio::client_state::{ClientKind, ClientPlatform, ClientSshMode};

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
        registry.update_state_and_enforce_mute_policy(
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
