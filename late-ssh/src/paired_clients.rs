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
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use crate::app::audio::client_state::{ClientAudioState, ClientKind};
use crate::app::audio::stations;
use crate::metrics;

// Multiplexed outbound channel to every paired client for a given SSH session
// token. Carries audio control (mute/volume/source) and clipboard fan-out.
//
// Paired clients are the native CLI and the CLI's own embedded webview helper.
// Browser pairing is gone, so there is no surface arbitration left: the source
// alone decides who is audible.
// - CLI plays a direct stream when the user's source is Icecast or Radio.
// - The webview helper plays YouTube when the user's source is YouTube.

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    /// Absolute mute/volume writes, relayed from a paired client's own
    /// `set_muted`/`set_volume` pair-WS events (a desktop MPRIS widget, media
    /// keys). Absolute rather than a toggle so every paired client converges
    /// on the same state even if one of them had drifted.
    SetMuted {
        muted: bool,
    },
    SetVolume {
        volume_percent: u8,
    },
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
    /// `users.settings.audio_source`). The CLI gates its direct-stream decoder
    /// on this and starts or stops its embedded webview helper for YouTube.
    ///
    /// There is no surface-arbitration flag here anymore. Browser pairing is
    /// gone, so the audible surface follows straight from the source: the CLI
    /// owns direct streams, the webview helper owns YouTube.
    SetPlaybackSource {
        source: AudioSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        stream_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        station: Option<String>,
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
    /// Ask a capable CLI to open a URL in the user's default browser. Sent
    /// for `/watch @user` and `/golive` so the stream page lands where the
    /// browser already is; raw SSH sessions get a QR modal instead.
    OpenUrl {
        url: String,
    },
}

/// Capacity of each paired client's outbound control queue. Control messages
/// are small state replays; a client that stops reading gets newest-dropped
/// behavior instead of an unbounded queue and re-syncs on reconnect.
pub const PAIR_CONTROL_QUEUE_CAP: usize = 64;
/// Max concurrent paired sockets per session token. A legitimate pairing is a
/// CLI plus its webview helper, with headroom for a reconnect overlapping a
/// stale entry; anything past this is a leak or an amplifier (one free SSH
/// token used to be an unbounded socket mint).
pub const MAX_PAIRED_CLIENTS_PER_TOKEN: usize = 8;

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
    tx: Sender<PairControlMessage>,
    state: ClientAudioState,
    usage_total_recorded: bool,
    user_id: Uuid,
    audio_source: AudioSource,
    icecast_stream: IcecastStream,
    radio_station: RadioStation,
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

    /// Returns `None` when the token already holds
    /// [`MAX_PAIRED_CLIENTS_PER_TOKEN`] live entries; the caller must close
    /// the socket instead of pairing it.
    pub fn register(
        &self,
        token: String,
        tx: Sender<PairControlMessage>,
        user_id: Uuid,
        audio_source: AudioSource,
    ) -> Option<u64> {
        let registration_id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut clients = self.clients.lock_recover();
        let entries = clients.entry(token.clone()).or_default();
        if entries.len() >= MAX_PAIRED_CLIENTS_PER_TOKEN {
            tracing::warn!(
                token_hint = %token_hint(&token),
                entries = entries.len(),
                "rejecting paired client: token at capacity"
            );
            return None;
        }
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
        Some(registration_id)
    }

    /// Remove the matching entry. The API disconnect path replays playback
    /// source afterward so remaining clients react to CLI presence changes.
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

    /// Send a voice control message to native CLIs on `token` that advertise
    /// voice support. The webview helper and older CLIs are skipped.
    pub fn send_control_to_voice_cli(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, ClientAudioState::supports_voice) > 0
    }

    /// Deliver to the first paired CLI advertising the `open_url`
    /// capability. Returns whether anyone got it, so callers can fall back
    /// to a QR modal for raw SSH sessions.
    pub fn send_control_to_open_url_cli(&self, token: &str, msg: PairControlMessage) -> bool {
        self.send_control_filter(token, msg, ClientAudioState::supports_open_url) > 0
    }

    /// Re-send each paired entry's cached playback source for `token`.
    pub fn broadcast_playback_source_for_token(&self, token: &str) -> bool {
        let targets: Vec<_> = {
            let clients = self.clients.lock_recover();
            let Some(entries) = clients.get(token) else {
                return false;
            };
            entries
                .iter()
                .map(|entry| playback_target(entry, &self.icecast_base_url))
                .collect()
        };

        let mut delivered = 0;
        for (tx, msg) in targets {
            match tx.try_send(msg) {
                Ok(()) => delivered += 1,
                Err(err) => {
                    tracing::warn!(
                        token_hint = %token_hint(token),
                        error = %try_send_reason(&err),
                        "failed to replay paired playback source"
                    );
                }
            }
        }
        delivered > 0
    }

    /// Send a control message to the paired entries a predicate accepts, e.g.
    /// only those advertising voice support.
    /// Returns the number of entries that accepted the message.
    fn send_control_filter<F>(&self, token: &str, msg: PairControlMessage, mut matches: F) -> usize
    where
        F: FnMut(&ClientAudioState) -> bool,
    {
        let targets: Vec<Sender<PairControlMessage>> = {
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
            match tx.try_send(msg.clone()) {
                Ok(()) => delivered += 1,
                Err(err) => {
                    tracing::warn!(
                        token_hint = %token_hint(token),
                        error = %try_send_reason(&err),
                        "failed to send paired client control message"
                    );
                }
            }
        }
        delivered
    }

    /// Record a state update for an entry and return its new kind. Pure state
    /// bookkeeping — playback gating lives on the client side (CLI gates on
    /// `audio_source`, the webview helper swaps its player on
    /// `SetPlaybackSource`), so nothing here needs to rebroadcast a source.
    pub fn update_state_and_enforce_mute_policy(
        &self,
        token: &str,
        registration_id: u64,
        new_state: ClientAudioState,
    ) -> Option<ClientKind> {
        let mut clients = self.clients.lock_recover();
        let entries = clients.get_mut(token)?;
        let entry = entries
            .iter_mut()
            .find(|entry| entry.registration_id == registration_id)?;

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
        Some(new_kind)
    }

    /// Snapshot the state of the most recently registered entry, preferring
    /// the webview helper when one is present. The helper only runs while the
    /// user is on YouTube, and it is then the audible surface, so its
    /// mute/volume is what the sidebar should report.
    pub fn snapshot(&self, token: &str) -> Option<ClientAudioState> {
        let clients = self.clients.lock_recover();
        let entries = clients.get(token)?;
        entries
            .iter()
            .rev()
            .find(|entry| entry.state.client_kind == ClientKind::Webview)
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
    /// webview entries for music UI state.
    pub fn has_voice_cli(&self, token: &str) -> bool {
        let clients = self.clients.lock_recover();
        clients
            .get(token)
            .is_some_and(|entries| entries.iter().any(|entry| entry.state.supports_voice()))
    }

    /// Send a clipboard-image request to a paired CLI on `token` that
    /// advertises the capability. Webview entries and capability-less CLIs
    /// are skipped — only one CLI per token can serve the clipboard.
    /// Returns true iff a capable CLI was found and the message queued.
    /// Distinct from `send_control` because the audio-priority `snapshot`
    /// would shadow the CLI entry once the webview helper is paired.
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
        if let Err(err) = tx.try_send(PairControlMessage::RequestClipboardImage { request_id }) {
            tracing::warn!(
                token_hint = %token_hint(token),
                error = %try_send_reason(&err),
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
    /// `SetPlaybackSource` to each. The CLI uses it to gate its direct-stream
    /// decoder and to start or stop its embedded webview helper.
    pub fn set_audio_source(&self, user_id: Uuid, source: AudioSource) {
        let mut targets = Vec::new();
        {
            let mut clients = self.clients.lock_recover();
            for entries in clients.values_mut() {
                for entry in entries.iter_mut() {
                    if entry.user_id != user_id {
                        continue;
                    }
                    entry.audio_source = source;
                    targets.push(playback_target(entry, &self.icecast_base_url));
                }
            }
        }

        for (tx, msg) in targets {
            if let Err(err) = tx.try_send(msg) {
                tracing::warn!(
                    error = %try_send_reason(&err),
                    "failed to push SetPlaybackSource after audio source change"
                );
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
                    targets.push(playback_target(entry, &self.icecast_base_url));
                }
            }
        }

        for (tx, msg) in targets {
            if let Err(err) = tx.try_send(msg) {
                tracing::warn!(
                    error = %try_send_reason(&err),
                    "failed to push SetPlaybackSource after stream choice change"
                );
            }
        }
    }
}

fn playback_target(
    entry: &PairControlEntry,
    icecast_base_url: &str,
) -> (Sender<PairControlMessage>, PairControlMessage) {
    (
        entry.tx.clone(),
        playback_message(
            icecast_base_url,
            entry.audio_source,
            entry.icecast_stream,
            entry.radio_station,
        ),
    )
}

pub fn playback_message(
    icecast_base_url: &str,
    source: AudioSource,
    icecast_stream: IcecastStream,
    radio_station: RadioStation,
) -> PairControlMessage {
    let selection =
        stations::resolve_stream_selection(icecast_base_url, source, icecast_stream, radio_station);
    PairControlMessage::SetPlaybackSource {
        source,
        stream_url: selection.as_ref().map(|selection| selection.url.clone()),
        station: selection.map(|selection| selection.station.to_string()),
    }
}

fn try_send_reason(
    err: &tokio::sync::mpsc::error::TrySendError<PairControlMessage>,
) -> &'static str {
    match err {
        tokio::sync::mpsc::error::TrySendError::Full(_) => "queue full",
        tokio::sync::mpsc::error::TrySendError::Closed(_) => "receiver closed",
    }
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

#[cfg(test)]
#[path = "paired_clients_test.rs"]
mod paired_clients_test;
