use late_core::MutexRecover;
use late_core::models::user::AudioSource;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::app::audio::client_state::{ClientAudioState, ClientKind, ClientSshMode};
use crate::metrics;

// Multiplexed outbound channel to every paired client (browser + CLI) for a
// given SSH session token. Carries audio control (mute/volume/source) and
// clipboard fan-out.
//
// Audio surface policy is intentionally small:
// - CLI plays Icecast only when the user's source is Icecast.
// - Real browser plays YouTube when paired; otherwise a capable CLI may spawn
//   the embedded webview helper as its YouTube fallback.
// - Browser plays Icecast only when no CLI is paired for the token; otherwise
//   switching back to Icecast just pauses the web YouTube player so the CLI is
//   the single Icecast surface.

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    RequestClipboardImage,
    /// Per-user setting: tell paired clients which audio source the user wants
    /// to hear. Server is the source of truth (persisted in
    /// `users.settings.audio_source`). Browsers swap their playback element;
    /// CLIs gate their Icecast decoder on this. YouTube-capable CLIs also use
    /// it to start or stop their embedded webview helper.
    SetPlaybackSource {
        source: AudioSource,
        /// Whether the browser should use its `<audio>` Icecast element when
        /// `source == Icecast`. False when a CLI is paired, because the CLI is
        /// then the single Icecast surface. CLI clients ignore this field.
        web_icecast_enabled: bool,
        /// Whether a YouTube-capable native CLI should spawn its embedded
        /// webview helper when `source == Youtube`. False when a real browser
        /// is paired for the token, because the browser is then the preferred
        /// YouTube surface. Browser clients ignore this field.
        embedded_webview_enabled: bool,
    },
    VoiceJoin {
        room: String,
        url: String,
        token: String,
        muted: bool,
        deafened: bool,
    },
    VoiceLeave,
    VoiceSetMuted {
        muted: bool,
    },
    VoiceSetDeafened {
        deafened: bool,
    },
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
    user_id: Uuid,
    audio_source: AudioSource,
}

#[derive(Debug, Clone, Copy)]
pub struct UpdateStateResult {
    pub previous_kind: ClientKind,
    pub new_kind: ClientKind,
}

impl PairedClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        token: String,
        tx: UnboundedSender<PairControlMessage>,
        user_id: Uuid,
        audio_source: AudioSource,
    ) -> u64 {
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
            user_id,
            audio_source,
        });
        registration_id
    }

    /// Remove the matching entry. The API disconnect path replays playback
    /// source afterward so remaining browsers can react to CLI presence
    /// changes.
    pub fn unregister_if_match(&self, token: &str, registration_id: u64) {
        let mut clients = self.clients.lock_recover();
        let Some(entries) = clients.get_mut(token) else {
            return;
        };
        let Some(position) = entries
            .iter()
            .position(|entry| entry.registration_id == registration_id)
        else {
            return;
        };
        let removed = entries.remove(position);
        if let Some((ssh_mode, platform)) = removed.state.cli_usage_labels() {
            metrics::add_cli_pair_active(-1, ssh_mode, platform);
        }
        tracing::info!(
            token_hint = %token_hint(token),
            registration_id,
            removed_kind = ?removed.state.client_kind,
            "unregistered paired client session"
        );
        if entries.is_empty() {
            clients.remove(token);
        }
    }

    /// Broadcast a control message to every paired client of `token`. Returns
    /// the number of entries that accepted the message.
    pub fn send_control(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, |_| true) > 0
    }

    /// Send a control message only to browser entries on `token`. Used for
    /// browser-only signals.
    pub fn send_control_to_browsers(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, |state| state.client_kind == ClientKind::Browser) > 0
    }

    /// Send a voice control message to native CLIs on `token` that advertise
    /// voice support. Browsers and older CLIs are skipped.
    pub fn send_control_to_voice_cli(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, ClientAudioState::supports_voice) > 0
    }

    /// True when the browser should be allowed to play the Icecast `<audio>`
    /// element for this token. A paired CLI owns Icecast, so the browser must
    /// stay silent on Icecast to avoid doubled streams.
    pub fn web_icecast_enabled(&self, token: &str) -> bool {
        let clients = self.clients.lock_recover();
        !clients
            .get(token)
            .map(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.state.client_kind == ClientKind::Cli)
            })
            .unwrap_or(false)
    }

    /// True when the native CLI should provide the YouTube webview fallback
    /// for this token. A real browser wins over the embedded helper; an
    /// existing webview-helper entry does not suppress itself.
    pub fn embedded_webview_enabled(&self, token: &str) -> bool {
        let clients = self.clients.lock_recover();
        !clients
            .get(token)
            .map(|entries| entries.iter().any(|entry| entry.is_real_browser()))
            .unwrap_or(false)
    }

    /// Re-send each paired entry's cached playback source for `token`, with a
    /// fresh browser Icecast allowance derived from current CLI presence.
    pub fn broadcast_playback_source_for_token(&self, token: &str) -> bool {
        let targets: Vec<_> = {
            let clients = self.clients.lock_recover();
            let Some(entries) = clients.get(token) else {
                return false;
            };
            let web_icecast_enabled = web_icecast_enabled_for_entries(entries);
            let embedded_webview_enabled = embedded_webview_enabled_for_entries(entries);
            entries
                .iter()
                .map(|entry| {
                    (
                        entry.tx.clone(),
                        entry.audio_source,
                        web_icecast_enabled,
                        embedded_webview_enabled,
                    )
                })
                .collect()
        };

        let mut delivered = 0;
        for (tx, source, web_icecast_enabled, embedded_webview_enabled) in targets {
            if tx
                .send(PairControlMessage::SetPlaybackSource {
                    source,
                    web_icecast_enabled,
                    embedded_webview_enabled,
                })
                .is_ok()
            {
                delivered += 1;
            } else {
                tracing::warn!(
                    token_hint = %token_hint(token),
                    "failed to replay paired playback source"
                );
            }
        }
        delivered > 0
    }

    /// Send a control message to paired entries whose `client_kind` matches the
    /// predicate. Used to target browser-only controls.
    /// Returns the number of entries that accepted the message.
    fn send_control_filter<F>(&self, token: &str, msg: PairControlMessage, mut matches: F) -> usize
    where
        F: FnMut(&ClientAudioState) -> bool,
    {
        let targets: Vec<UnboundedSender<PairControlMessage>> = {
            let clients = self.clients.lock_recover();
            clients
                .get(token)
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|entry| matches(&entry.state))
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

    /// Record a state update for an entry and return the kind transition for
    /// the caller. Pure state bookkeeping — playback gating lives on the
    /// client side (CLI gates on `audio_source`, browser swaps its player on
    /// `SetPlaybackSource`).
    pub fn update_state_and_enforce_mute_policy(
        &self,
        token: &str,
        registration_id: u64,
        new_state: ClientAudioState,
    ) -> Option<UpdateStateResult> {
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
        entry.state = new_state;

        Some(UpdateStateResult {
            previous_kind,
            new_kind,
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

    /// True when any paired native CLI on `token` advertises voice support.
    /// This intentionally scans every paired entry because `snapshot` prefers
    /// browser/webview entries for music UI state.
    pub fn has_voice_cli(&self, token: &str) -> bool {
        let clients = self.clients.lock_recover();
        clients
            .get(token)
            .is_some_and(|entries| entries.iter().any(|entry| entry.state.supports_voice()))
    }

    /// Send a clipboard-image request to a paired CLI on `token` that
    /// advertises the capability. Browser entries and capability-less CLIs
    /// are skipped — only one CLI per token can serve the clipboard.
    /// Returns true iff a capable CLI was found and the message queued.
    /// Distinct from `send_control` because the audio-priority `snapshot`
    /// would shadow the CLI entry once a browser is paired.
    pub fn request_clipboard_image(&self, token: &str) -> bool {
        let tx = {
            let clients = self.clients.lock_recover();
            clients.get(token).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.state.supports_clipboard_image())
                    .map(|entry| entry.tx.clone())
            })
        };
        let Some(tx) = tx else {
            return false;
        };
        if tx.send(PairControlMessage::RequestClipboardImage).is_err() {
            tracing::warn!(
                token_hint = %token_hint(token),
                "failed to send paired clipboard image request"
            );
            return false;
        }
        true
    }

    /// Update every entry for `user_id` to the new audio source and push
    /// `SetPlaybackSource` to each (CLI and browser alike). The CLI uses it to
    /// gate its Icecast decoder; the browser uses it to swap playback element.
    /// Browser Icecast is disabled whenever a CLI is present on the token, and
    /// embedded CLI webview is disabled whenever a real browser is present.
    pub fn set_audio_source(&self, user_id: Uuid, source: AudioSource) {
        let mut targets = Vec::new();
        {
            let mut clients = self.clients.lock_recover();
            for entries in clients.values_mut() {
                let web_icecast_enabled = web_icecast_enabled_for_entries(entries);
                let embedded_webview_enabled = embedded_webview_enabled_for_entries(entries);
                for entry in entries.iter_mut() {
                    if entry.user_id != user_id {
                        continue;
                    }
                    entry.audio_source = source;
                    targets.push((
                        entry.tx.clone(),
                        web_icecast_enabled,
                        embedded_webview_enabled,
                    ));
                }
            }
        }

        for (tx, web_icecast_enabled, embedded_webview_enabled) in targets {
            if tx
                .send(PairControlMessage::SetPlaybackSource {
                    source,
                    web_icecast_enabled,
                    embedded_webview_enabled,
                })
                .is_err()
            {
                tracing::warn!("failed to push SetPlaybackSource after audio source change");
            }
        }
    }
}

impl PairControlEntry {
    fn is_real_browser(&self) -> bool {
        self.state.client_kind == ClientKind::Browser
            && self.state.ssh_mode != ClientSshMode::Webview
    }
}

fn web_icecast_enabled_for_entries(entries: &[PairControlEntry]) -> bool {
    !entries
        .iter()
        .any(|entry| entry.state.client_kind == ClientKind::Cli)
}

fn embedded_webview_enabled_for_entries(entries: &[PairControlEntry]) -> bool {
    !entries.iter().any(PairControlEntry::is_real_browser)
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
        registry.register(
            "tok1".to_string(),
            tx,
            Uuid::now_v7(),
            AudioSource::default(),
        );

        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    }

    #[test]
    fn paired_client_unregister_if_match_removes_only_matching_entry() {
        let registry = PairedClientRegistry::new();
        let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
        let first = registry.register(
            "tok1".to_string(),
            tx1,
            Uuid::now_v7(),
            AudioSource::default(),
        );
        let second = registry.register(
            "tok1".to_string(),
            tx2,
            Uuid::now_v7(),
            AudioSource::default(),
        );

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
        let registration_id = registry.register(
            "tok1".to_string(),
            tx,
            Uuid::now_v7(),
            AudioSource::default(),
        );
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

    #[test]
    fn voice_cli_detection_ignores_browser_preferred_snapshot() {
        let registry = PairedClientRegistry::new();
        let user_id = Uuid::now_v7();

        let (cli_tx, _cli_rx) = tokio::sync::mpsc::unbounded_channel();
        let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::default());
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            cli_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Linux,
                capabilities: vec!["voice".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );

        let (webview_tx, _webview_rx) = tokio::sync::mpsc::unbounded_channel();
        let webview_id = registry.register(
            "tok1".to_string(),
            webview_tx,
            user_id,
            AudioSource::Youtube,
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            webview_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Webview,
                platform: ClientPlatform::Linux,
                capabilities: vec!["youtube".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );

        assert_eq!(
            registry.snapshot("tok1").unwrap().client_kind,
            ClientKind::Browser
        );
        assert!(registry.has_voice_cli("tok1"));
    }

    #[test]
    fn paired_client_request_clipboard_image_reaches_cli_when_browser_paired() {
        let registry = PairedClientRegistry::new();

        let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
        let cli_id = registry.register(
            "tok1".to_string(),
            cli_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            cli_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Linux,
                capabilities: vec!["clipboard_image".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );

        let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
        let browser_id = registry.register(
            "tok1".to_string(),
            browser_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            browser_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Unknown,
                platform: ClientPlatform::Unknown,
                capabilities: Vec::new(),
                muted: false,
                volume_percent: 30,
            },
        );

        assert!(registry.request_clipboard_image("tok1"));
        assert_eq!(
            cli_rx.try_recv().unwrap(),
            PairControlMessage::RequestClipboardImage
        );
        assert!(browser_rx.try_recv().is_err());
    }

    #[test]
    fn paired_client_request_clipboard_image_false_when_only_browser() {
        let registry = PairedClientRegistry::new();
        let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
        let browser_id = registry.register(
            "tok1".to_string(),
            browser_tx,
            Uuid::now_v7(),
            AudioSource::default(),
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            browser_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Unknown,
                platform: ClientPlatform::Unknown,
                capabilities: Vec::new(),
                muted: false,
                volume_percent: 30,
            },
        );

        assert!(!registry.request_clipboard_image("tok1"));
        assert!(browser_rx.try_recv().is_err());
    }

    #[test]
    fn state_update_never_sends_pair_control_message() {
        // CLI playback gating lives on the CLI side (it reads
        // SetPlaybackSource and silences the Icecast decoder when source !=
        // Icecast). The server's state-update path is pure bookkeeping and
        // must not push anything back at the client.
        let registry = PairedClientRegistry::new();
        let user_id = Uuid::now_v7();

        let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
        let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Youtube);
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            cli_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Linux,
                capabilities: vec!["youtube".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );
        assert!(cli_rx.try_recv().is_err());
    }

    #[test]
    fn set_audio_source_pushes_playback_source_to_every_entry() {
        let registry = PairedClientRegistry::new();
        let user_id = Uuid::now_v7();

        let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
        let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Icecast);
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            cli_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Linux,
                capabilities: vec!["youtube".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );
        let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
        let browser_id = registry.register(
            "tok1".to_string(),
            browser_tx,
            user_id,
            AudioSource::Icecast,
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            browser_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Unknown,
                platform: ClientPlatform::Unknown,
                capabilities: Vec::new(),
                muted: false,
                volume_percent: 30,
            },
        );

        registry.set_audio_source(user_id, AudioSource::Youtube);
        assert_eq!(
            cli_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );
        assert_eq!(
            browser_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );

        registry.set_audio_source(user_id, AudioSource::Icecast);
        assert_eq!(
            cli_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Icecast,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );
        assert_eq!(
            browser_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Icecast,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );
    }

    #[test]
    fn browser_only_token_can_play_web_icecast() {
        let registry = PairedClientRegistry::new();
        let user_id = Uuid::now_v7();

        let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
        let browser_id = registry.register(
            "tok1".to_string(),
            browser_tx,
            user_id,
            AudioSource::Youtube,
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            browser_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Unknown,
                platform: ClientPlatform::Unknown,
                capabilities: Vec::new(),
                muted: false,
                volume_percent: 30,
            },
        );

        registry.set_audio_source(user_id, AudioSource::Icecast);
        assert_eq!(
            browser_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Icecast,
                web_icecast_enabled: true,
                embedded_webview_enabled: false,
            }
        );
    }

    #[test]
    fn embedded_webview_is_enabled_only_when_no_real_browser_is_paired() {
        let registry = PairedClientRegistry::new();
        let user_id = Uuid::now_v7();

        let (cli_tx, mut cli_rx) = tokio::sync::mpsc::unbounded_channel();
        let cli_id = registry.register("tok1".to_string(), cli_tx, user_id, AudioSource::Icecast);
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            cli_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Linux,
                capabilities: vec!["youtube".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );

        let (webview_tx, mut webview_rx) = tokio::sync::mpsc::unbounded_channel();
        let webview_id = registry.register(
            "tok1".to_string(),
            webview_tx,
            user_id,
            AudioSource::Icecast,
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            webview_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Webview,
                platform: ClientPlatform::Linux,
                capabilities: vec!["youtube".to_string()],
                muted: false,
                volume_percent: 30,
            },
        );

        registry.set_audio_source(user_id, AudioSource::Youtube);
        assert_eq!(
            cli_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: true,
            }
        );
        assert_eq!(
            webview_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: true,
            }
        );

        let (browser_tx, mut browser_rx) = tokio::sync::mpsc::unbounded_channel();
        let browser_id = registry.register(
            "tok1".to_string(),
            browser_tx,
            user_id,
            AudioSource::Youtube,
        );
        registry.update_state_and_enforce_mute_policy(
            "tok1",
            browser_id,
            ClientAudioState {
                client_kind: ClientKind::Browser,
                ssh_mode: ClientSshMode::Unknown,
                platform: ClientPlatform::Unknown,
                capabilities: Vec::new(),
                muted: false,
                volume_percent: 30,
            },
        );

        assert!(registry.broadcast_playback_source_for_token("tok1"));
        assert_eq!(
            cli_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );
        assert_eq!(
            webview_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );
        assert_eq!(
            browser_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: false,
            }
        );

        registry.unregister_if_match("tok1", browser_id);

        assert!(registry.broadcast_playback_source_for_token("tok1"));
        assert_eq!(
            cli_rx.try_recv().unwrap(),
            PairControlMessage::SetPlaybackSource {
                source: AudioSource::Youtube,
                web_icecast_enabled: false,
                embedded_webview_enabled: true,
            }
        );
    }
}
