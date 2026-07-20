use late_core::MutexRecover;
use late_core::models::user::{AudioSource, IcecastStream, RadioStation};
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
use crate::app::audio::stations;
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
    /// Ask a capable CLI to read its clipboard image. `request_id` is echoed
    /// back in the `clipboard_image`/`clipboard_image_failed` payload so a
    /// late response to a timed-out request can't satisfy a newer one. Old
    /// CLIs ignore the field and echo nothing; the server then falls back to
    /// token-level matching.
    RequestClipboardImage {
        request_id: u64,
    },
    /// Per-user setting: tell paired clients which audio source the user wants
    /// to hear. Server is the source of truth (persisted in
    /// `users.settings.audio_source`). Browsers swap their playback element;
    /// CLIs gate their Icecast decoder on this. YouTube-capable CLIs also use
    /// it to start or stop their embedded webview helper.
    SetPlaybackSource {
        source: AudioSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        stream_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        station: Option<String>,
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

#[derive(Clone)]
pub struct PairedClientRegistry {
    clients: Arc<Mutex<HashMap<String, Vec<PairControlEntry>>>>,
    next_id: Arc<AtomicU64>,
    icecast_base_url: Arc<String>,
    /// Tokens with an outstanding `RequestClipboardImage`, mapped to that
    /// request's id. Inbound clipboard payloads are dropped unless their
    /// token holds a slot here (so a rogue paired client cannot queue
    /// multi-MB images into the session channel), and an echoed id that
    /// doesn't match the slot is a late answer to an older, timed-out
    /// request and is dropped too.
    clipboard_requests: Arc<Mutex<HashMap<String, u64>>>,
    next_clipboard_request_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PairControlEntry {
    registration_id: u64,
    tx: UnboundedSender<PairControlMessage>,
    state: ClientAudioState,
    usage_total_recorded: bool,
    user_id: Uuid,
    audio_source: AudioSource,
    icecast_stream: IcecastStream,
    radio_station: RadioStation,
}

#[derive(Debug, Clone, Copy)]
pub struct UpdateStateResult {
    pub previous_kind: ClientKind,
    pub new_kind: ClientKind,
    pub previous_claimed_icecast_output: bool,
    pub new_claims_icecast_output: bool,
}

impl PairedClientRegistry {
    pub fn new(icecast_base_url: impl Into<String>) -> Self {
        Self {
            clients: Arc::default(),
            next_id: Arc::default(),
            icecast_base_url: Arc::new(icecast_base_url.into()),
            clipboard_requests: Arc::default(),
            next_clipboard_request_id: Arc::default(),
        }
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
            icecast_stream: IcecastStream::default(),
            radio_station: RadioStation::default(),
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
            self.clipboard_requests.lock_recover().remove(token);
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
            .map(|entries| entries.iter().any(PairControlEntry::claims_icecast_output))
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
                    playback_target(
                        entry,
                        &self.icecast_base_url,
                        web_icecast_enabled,
                        embedded_webview_enabled,
                    )
                })
                .collect()
        };

        let mut delivered = 0;
        for (tx, msg) in targets {
            if tx.send(msg).is_ok() {
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
        let previous_claimed_icecast_output = entry.claims_icecast_output();
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
        let new_claims_icecast_output = entry.claims_icecast_output();

        Some(UpdateStateResult {
            previous_kind,
            new_kind,
            previous_claimed_icecast_output,
            new_claims_icecast_output,
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

    /// Muted state of the most recently registered CLI entry on `token`, if
    /// any. Used to align a connecting webview helper to the session's
    /// current runtime mute instead of the boot preference: helper respawns
    /// and pair-WS reconnects mid-session must not unmute a muted session.
    pub fn cli_muted(&self, token: &str) -> Option<bool> {
        let clients = self.clients.lock_recover();
        clients
            .get(token)?
            .iter()
            .rev()
            .find(|entry| entry.state.client_kind == ClientKind::Cli)
            .map(|entry| entry.state.muted)
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
        let request_id = self
            .next_clipboard_request_id
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if tx
            .send(PairControlMessage::RequestClipboardImage { request_id })
            .is_err()
        {
            tracing::warn!(
                token_hint = %token_hint(token),
                "failed to send paired clipboard image request"
            );
            return false;
        }
        self.clipboard_requests
            .lock_recover()
            .insert(token.to_string(), request_id);
        true
    }

    /// Consume the outstanding clipboard request for `token`, if any. Called
    /// by the pair WS handler before it accepts an inbound clipboard image or
    /// failure payload; a `false` return means the payload is unsolicited and
    /// must be dropped. `request_id` is the id the client echoed back (None
    /// from older CLIs that don't echo). An echo for a different id is a late
    /// answer to an already-replaced request: it is refused and the slot
    /// stays armed for the response still owed.
    pub fn take_clipboard_request(&self, token: &str, request_id: Option<u64>) -> bool {
        let mut requests = self.clipboard_requests.lock_recover();
        let Some(&outstanding) = requests.get(token) else {
            return false;
        };
        match request_id {
            Some(echoed) if echoed != outstanding => false,
            _ => {
                requests.remove(token);
                true
            }
        }
    }

    /// Drop the outstanding clipboard request for `token`. Called when the
    /// session-side wait times out, so the slot doesn't stay armed forever
    /// and a late response can't be accepted as if it were fresh.
    pub fn cancel_clipboard_request(&self, token: &str) {
        self.clipboard_requests.lock_recover().remove(token);
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
                    targets.push(playback_target(
                        entry,
                        &self.icecast_base_url,
                        web_icecast_enabled,
                        embedded_webview_enabled,
                    ));
                }
            }
        }

        for (tx, msg) in targets {
            if tx.send(msg).is_err() {
                tracing::warn!("failed to push SetPlaybackSource after audio source change");
            }
        }
    }

    pub fn set_stream_preferences(
        &self,
        user_id: Uuid,
        icecast_stream: IcecastStream,
        radio_station: RadioStation,
    ) {
        let mut clients = self.clients.lock_recover();
        for entries in clients.values_mut() {
            for entry in entries.iter_mut() {
                if entry.user_id == user_id {
                    entry.icecast_stream = icecast_stream;
                    entry.radio_station = radio_station;
                }
            }
        }
    }

    pub fn set_icecast_stream(&self, user_id: Uuid, stream: IcecastStream) {
        self.update_stream_choice(user_id, Some(stream), None);
    }

    pub fn set_radio_station(&self, user_id: Uuid, station: RadioStation) {
        self.update_stream_choice(user_id, None, Some(station));
    }

    fn update_stream_choice(
        &self,
        user_id: Uuid,
        icecast_stream: Option<IcecastStream>,
        radio_station: Option<RadioStation>,
    ) {
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
                    if let Some(stream) = icecast_stream {
                        entry.icecast_stream = stream;
                    }
                    if let Some(station) = radio_station {
                        entry.radio_station = station;
                    }
                    targets.push(playback_target(
                        entry,
                        &self.icecast_base_url,
                        web_icecast_enabled,
                        embedded_webview_enabled,
                    ));
                }
            }
        }

        for (tx, msg) in targets {
            if tx.send(msg).is_err() {
                tracing::warn!("failed to push SetPlaybackSource after stream choice change");
            }
        }
    }
}

impl PairControlEntry {
    fn is_real_browser(&self) -> bool {
        self.state.client_kind == ClientKind::Browser
            && self.state.ssh_mode != ClientSshMode::Webview
    }

    fn claims_icecast_output(&self) -> bool {
        self.state.client_kind == ClientKind::Cli && self.state.icecast_output_available
    }
}

fn web_icecast_enabled_for_entries(entries: &[PairControlEntry]) -> bool {
    !entries.iter().any(PairControlEntry::claims_icecast_output)
}

fn embedded_webview_enabled_for_entries(entries: &[PairControlEntry]) -> bool {
    !entries.iter().any(PairControlEntry::is_real_browser)
}

fn playback_target(
    entry: &PairControlEntry,
    icecast_base_url: &str,
    web_icecast_enabled: bool,
    embedded_webview_enabled: bool,
) -> (UnboundedSender<PairControlMessage>, PairControlMessage) {
    (
        entry.tx.clone(),
        playback_message(
            icecast_base_url,
            entry.audio_source,
            entry.icecast_stream,
            entry.radio_station,
            web_icecast_enabled,
            embedded_webview_enabled,
        ),
    )
}

pub fn playback_message(
    icecast_base_url: &str,
    source: AudioSource,
    icecast_stream: IcecastStream,
    radio_station: RadioStation,
    web_icecast_enabled: bool,
    embedded_webview_enabled: bool,
) -> PairControlMessage {
    let selection =
        stations::resolve_stream_selection(icecast_base_url, source, icecast_stream, radio_station);
    PairControlMessage::SetPlaybackSource {
        source,
        stream_url: selection.as_ref().map(|selection| selection.url.clone()),
        station: selection.map(|selection| selection.station.to_string()),
        web_icecast_enabled,
        embedded_webview_enabled,
    }
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}


