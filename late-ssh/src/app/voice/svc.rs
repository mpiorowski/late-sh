use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use late_core::{
    MutexRecover,
    db::Db,
    models::{
        chat_room_member::ChatRoomMember,
        voice_channel::{TARGET_CHAT_ROOM, VoiceChannel},
    },
};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct VoiceConfig {
    pub enabled: bool,
    /// Client-facing LiveKit URL, handed to browsers in join grants. In dev
    /// this is `ws://localhost:7880`, which only resolves from the host.
    pub livekit_url: Option<String>,
    /// Server-to-server base for Twirp API calls (RemoveParticipant, the
    /// Ingress API). Split from `livekit_url` because the two audiences
    /// differ: in dev the browser needs `localhost` while this process runs
    /// in a container where `localhost` is itself, not LiveKit.
    pub livekit_api_url: Option<String>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    /// Base name for LiveKit rooms. Each voice channel gets its own LiveKit
    /// room named `{room_name}-{voice_channel_id}`.
    pub room_name: String,
}

impl VoiceConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            livekit_url: None,
            livekit_api_url: None,
            api_key: None,
            api_secret: None,
            room_name: "late-voice".to_string(),
        }
    }

    pub fn enabled(
        livekit_url: String,
        livekit_api_url: String,
        api_key: String,
        api_secret: String,
        room_name: String,
    ) -> anyhow::Result<Self> {
        if livekit_url.trim().is_empty() {
            anyhow::bail!("voice livekit url must not be empty");
        }
        if livekit_api_url.trim().is_empty() {
            anyhow::bail!("voice livekit api url must not be empty");
        }
        if api_key.trim().is_empty() {
            anyhow::bail!("voice livekit api key must not be empty");
        }
        if api_secret.trim().is_empty() {
            anyhow::bail!("voice livekit api secret must not be empty");
        }
        if room_name.trim().is_empty() {
            anyhow::bail!("voice room name must not be empty");
        }
        Ok(Self {
            enabled: true,
            livekit_url: Some(livekit_url),
            livekit_api_url: Some(livekit_api_url),
            api_key: Some(api_key),
            api_secret: Some(api_secret),
            room_name,
        })
    }
}

impl fmt::Debug for VoiceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceConfig")
            .field("enabled", &self.enabled)
            .field("livekit_url", &self.livekit_url)
            .field("livekit_api_url", &self.livekit_api_url)
            .field("api_key_present", &self.api_key.is_some())
            .field("api_secret_present", &self.api_secret.is_some())
            .field("room_name", &self.room_name)
            .finish()
    }
}

/// A point-in-time view of who is in voice, keyed by voice channel id. A user
/// is in at most one voice channel at a time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VoiceSnapshot {
    pub enabled: bool,
    pub livekit_url: Option<String>,
    pub rooms: HashMap<Uuid, Vec<VoiceParticipant>>,
}

impl VoiceSnapshot {
    /// Participants in a given voice channel (empty if none).
    pub fn participants(&self, room_id: Uuid) -> &[VoiceParticipant] {
        self.rooms.get(&room_id).map_or(&[], Vec::as_slice)
    }

    pub fn participant(&self, room_id: Uuid, user_id: Uuid) -> Option<&VoiceParticipant> {
        self.participants(room_id)
            .iter()
            .find(|participant| participant.user_id == user_id)
    }

    /// The voice channel the user is currently in, if any.
    pub fn current_room(&self, user_id: Uuid) -> Option<Uuid> {
        self.rooms.iter().find_map(|(room_id, participants)| {
            participants
                .iter()
                .any(|participant| participant.user_id == user_id)
                .then_some(*room_id)
        })
    }

    pub fn is_joined(&self, user_id: Uuid) -> bool {
        self.current_room(user_id).is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceParticipant {
    pub user_id: Uuid,
    pub username: String,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceClientState {
    pub joined: bool,
    /// LiveKit room name the client reports being connected to. The voice
    /// channel id is parsed back out of it.
    pub room: Option<String>,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceJoinTicket {
    pub room: String,
    pub url: String,
    pub token: String,
    pub muted: bool,
    pub deafened: bool,
}

/// LiveKit connection details for a stream-room media page (the streamer's
/// go-live console or an anonymous watch page). Pure media plumbing: no
/// muted/deafened seed like [`VoiceJoinTicket`], because these pages are not
/// voice participants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamMediaTicket {
    pub room: String,
    pub url: String,
    pub token: String,
}

/// A WHIP ingress minted for an OBS stream: OBS pushes WebRTC to `url` with
/// `stream_key` as the bearer token, and the LiveKit ingress service
/// republishes it into the stream room. `url` comes from the server's
/// `ingress.whip_base_url` config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhipIngress {
    pub ingress_id: String,
    pub url: String,
    pub stream_key: String,
}

/// Outcome of a moderator `kick`. `changed` is whether anything actually changed
/// (newly blocked or removed). `livekit_room` is the LiveKit room the user was
/// in, if any, so the caller can force-disconnect them via `remove_participant`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VoiceKick {
    pub changed: bool,
    pub livekit_room: Option<String>,
}

#[derive(Clone)]
pub struct VoiceService {
    config: VoiceConfig,
    db: Option<Db>,
    inner: Arc<Mutex<VoiceInner>>,
    tx: watch::Sender<VoiceSnapshot>,
    http: reqwest::Client,
}

#[derive(Default)]
struct VoiceInner {
    /// voice_channel_id -> (user_id -> participant). A user appears in at most
    /// one voice channel.
    rooms: HashMap<Uuid, HashMap<Uuid, VoiceParticipant>>,
    /// Users a moderator has removed from voice. While blocked, no join ticket
    /// is minted and any self-reported presence is dropped. The block is
    /// server-wide (it spans every room) and runtime-only - it clears on
    /// `allow` or a server restart (it is not persisted).
    blocked: HashSet<Uuid>,
    /// The voice channel most recently authorized by a server-minted join
    /// ticket. Client-reported `voice_state` is accepted only for this room.
    authorized_room_by_user: HashMap<Uuid, Uuid>,
}

impl VoiceInner {
    /// Remove a user from whatever room they are in. Returns the room id they
    /// were removed from, if any. Drops the room entry once it goes empty.
    fn remove_user(&mut self, user_id: Uuid) -> Option<Uuid> {
        let mut found = None;
        for (room_id, participants) in &mut self.rooms {
            if participants.remove(&user_id).is_some() {
                found = Some(*room_id);
                break;
            }
        }
        if let Some(room_id) = found
            && self.rooms.get(&room_id).is_some_and(HashMap::is_empty)
        {
            self.rooms.remove(&room_id);
        }
        found
    }
}

impl VoiceService {
    pub fn new(config: VoiceConfig) -> Self {
        let snapshot = VoiceSnapshot {
            enabled: config.enabled,
            livekit_url: config.livekit_url.clone(),
            rooms: HashMap::new(),
        };
        let (tx, _) = watch::channel(snapshot);
        Self {
            config,
            db: None,
            inner: Arc::new(Mutex::new(VoiceInner::default())),
            tx,
            // Everything through this client is a server-to-server Twirp
            // call, and one of them runs inline in the stream sweep loop:
            // without a timeout a wedged LiveKit connection would stall
            // every stream TTL in the app for as long as the OS lets the
            // socket hang.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("building reqwest client"),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn with_db(mut self, db: Db) -> Self {
        self.db = Some(db);
        self
    }

    pub fn config(&self) -> &VoiceConfig {
        &self.config
    }

    pub fn snapshot(&self) -> VoiceSnapshot {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<VoiceSnapshot> {
        self.tx.subscribe()
    }

    /// LiveKit room name for a voice channel. The voice channel id is embedded
    /// as the suffix so it can be recovered from client-reported presence
    /// without a client protocol change.
    pub fn livekit_room_name(&self, room_id: Uuid) -> String {
        format!("{}-{}", self.config.room_name, room_id)
    }

    /// Recover the voice channel id from a LiveKit room name we minted.
    fn room_id_from_livekit(&self, livekit_room: &str) -> Option<Uuid> {
        let prefix = format!("{}-", self.config.room_name);
        livekit_room
            .strip_prefix(&prefix)
            .and_then(|suffix| Uuid::parse_str(suffix).ok())
    }

    pub fn join_ticket(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        username: &str,
        muted: bool,
        deafened: bool,
    ) -> anyhow::Result<VoiceJoinTicket> {
        if !self.config.enabled {
            anyhow::bail!("voice is not configured");
        }
        if self.is_blocked(user_id) {
            anyhow::bail!("you have been removed from voice by a moderator");
        }

        let room = self.livekit_room_name(room_id);
        let url = self
            .config
            .livekit_url
            .clone()
            .context("voice enabled without LiveKit URL")?;
        let token = self.mint_livekit_token(user_id, username, &room)?;

        Ok(VoiceJoinTicket {
            room,
            url,
            token,
            muted,
            deafened,
        })
    }

    pub async fn checked_join_ticket(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        username: &str,
        muted: bool,
        deafened: bool,
    ) -> anyhow::Result<VoiceJoinTicket> {
        let db = self
            .db
            .as_ref()
            .context("voice join authorization is not configured")?;
        let client = db.get().await?;
        let channel = VoiceChannel::find_enabled_by_id(&client, room_id)
            .await?
            .context("voice channel is not available")?;
        ensure_user_can_join_voice(&client, &channel, user_id).await?;
        let ticket = self.join_ticket(room_id, user_id, username, muted, deafened)?;
        self.authorize_room_for_user(user_id, room_id);
        Ok(ticket)
    }

    pub fn apply_client_state(&self, user_id: Uuid, username: String, state: VoiceClientState) {
        let Some(room_id) = state
            .room
            .as_deref()
            .and_then(|room| self.room_id_from_livekit(room))
        else {
            // Not joined, or a room we don't recognize: ensure they are gone.
            self.leave(user_id);
            return;
        };
        if !state.joined {
            self.leave(user_id);
            return;
        }

        // A moderator-blocked user stays out even if their client keeps
        // reporting presence.
        if self.is_blocked(user_id) {
            self.leave(user_id);
            return;
        }

        if self.authorized_room_for_user(user_id) != Some(room_id) {
            self.leave(user_id);
            return;
        }

        {
            let mut inner = self.inner.lock_recover();
            // A user is only ever in one room; clear any stale membership first.
            inner.remove_user(user_id);
            inner.rooms.entry(room_id).or_default().insert(
                user_id,
                VoiceParticipant {
                    user_id,
                    username,
                    muted: state.muted,
                    deafened: state.deafened,
                    speaking: state.speaking,
                    updated_at: Utc::now(),
                },
            );
        }
        self.publish_snapshot();
    }

    pub fn leave(&self, user_id: Uuid) {
        let removed = {
            let mut inner = self.inner.lock_recover();
            let removed = inner.remove_user(user_id).is_some();
            inner.authorized_room_by_user.remove(&user_id).is_some() || removed
        };
        if removed {
            self.publish_snapshot();
        }
    }

    /// Remove every known/authorized user from a voice channel and return the
    /// LiveKit identities to force-disconnect.
    pub fn revoke_channel(&self, room_id: Uuid) -> Vec<(String, Uuid)> {
        let users = {
            let mut inner = self.inner.lock_recover();
            let mut users = inner
                .rooms
                .remove(&room_id)
                .map(|participants| participants.into_keys().collect::<HashSet<_>>())
                .unwrap_or_default();
            inner
                .authorized_room_by_user
                .retain(|user_id, authorized_room| {
                    if *authorized_room == room_id {
                        users.insert(*user_id);
                        false
                    } else {
                        true
                    }
                });
            users
        };
        if !users.is_empty() {
            self.publish_snapshot();
        }
        let livekit_room = self.livekit_room_name(room_id);
        users
            .into_iter()
            .map(|user_id| (livekit_room.clone(), user_id))
            .collect()
    }

    /// Revoke one user's access to one voice channel. Returns a LiveKit
    /// removal target even if the user was only authorized but not in the
    /// local roster yet.
    pub fn revoke_user_from_channel(&self, room_id: Uuid, user_id: Uuid) -> Option<(String, Uuid)> {
        let changed = {
            let mut inner = self.inner.lock_recover();
            let mut changed = inner
                .rooms
                .get_mut(&room_id)
                .is_some_and(|participants| participants.remove(&user_id).is_some());
            if inner.rooms.get(&room_id).is_some_and(HashMap::is_empty) {
                inner.rooms.remove(&room_id);
            }
            if inner.authorized_room_by_user.get(&user_id) == Some(&room_id) {
                inner.authorized_room_by_user.remove(&user_id);
                changed = true;
            }
            changed
        };
        if changed {
            self.publish_snapshot();
            Some((self.livekit_room_name(room_id), user_id))
        } else {
            None
        }
    }

    /// Revoke one user from whichever voice channel they are currently in or
    /// most recently authorized for.
    pub fn revoke_user(&self, user_id: Uuid) -> Option<(String, Uuid)> {
        let room_id = {
            let mut inner = self.inner.lock_recover();
            let room_id = inner
                .remove_user(user_id)
                .or_else(|| inner.authorized_room_by_user.remove(&user_id));
            if room_id.is_none() {
                inner.authorized_room_by_user.remove(&user_id);
            }
            room_id
        };
        if let Some(room_id) = room_id {
            self.publish_snapshot();
            Some((self.livekit_room_name(room_id), user_id))
        } else {
            None
        }
    }

    /// Moderator action: remove a user from voice now and block them from
    /// rejoining any room (no join ticket is minted) until `allow` lifts it or
    /// the server restarts. Returns the LiveKit room they were in (if any) so
    /// the caller can force-disconnect an already-connected session via
    /// `remove_participant` - the block alone only stops *new* tickets, and a
    /// minted token stays valid until it expires. Runtime-only; not persisted.
    pub fn kick(&self, user_id: Uuid) -> VoiceKick {
        let (newly_blocked, room_id) = {
            let mut inner = self.inner.lock_recover();
            let newly_blocked = inner.blocked.insert(user_id);
            let room_id = inner
                .remove_user(user_id)
                .or_else(|| inner.authorized_room_by_user.remove(&user_id));
            (newly_blocked, room_id)
        };
        if newly_blocked || room_id.is_some() {
            self.publish_snapshot();
        }
        VoiceKick {
            changed: newly_blocked || room_id.is_some(),
            livekit_room: room_id.map(|id| self.livekit_room_name(id)),
        }
    }

    /// Lift a moderator voice block. Returns whether the user was blocked.
    pub fn allow(&self, user_id: Uuid) -> bool {
        self.inner.lock_recover().blocked.remove(&user_id)
    }

    pub fn is_blocked(&self, user_id: Uuid) -> bool {
        self.inner.lock_recover().blocked.contains(&user_id)
    }

    pub fn update_local_state(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        username: String,
        muted: bool,
        deafened: bool,
        speaking: bool,
    ) {
        self.authorize_room_for_user(user_id, room_id);
        self.apply_client_state(
            user_id,
            username,
            VoiceClientState {
                joined: true,
                room: Some(self.livekit_room_name(room_id)),
                muted,
                deafened,
                speaking,
            },
        );
    }

    fn authorize_room_for_user(&self, user_id: Uuid, room_id: Uuid) {
        self.inner
            .lock_recover()
            .authorized_room_by_user
            .insert(user_id, room_id);
    }

    fn authorized_room_for_user(&self, user_id: Uuid) -> Option<Uuid> {
        self.inner
            .lock_recover()
            .authorized_room_by_user
            .get(&user_id)
            .copied()
    }

    pub fn prune_stale(&self, ttl: Duration) {
        let cutoff = Utc::now() - ttl;
        let pruned = {
            let mut inner = self.inner.lock_recover();
            let before: usize = inner.rooms.values().map(HashMap::len).sum();
            for participants in inner.rooms.values_mut() {
                participants.retain(|_, participant| participant.updated_at >= cutoff);
            }
            inner
                .rooms
                .retain(|_, participants| !participants.is_empty());
            let after: usize = inner.rooms.values().map(HashMap::len).sum();
            after != before
        };
        if pruned {
            self.publish_snapshot();
        }
    }

    /// Force-disconnect the streamer's go-live console from a stream room's
    /// LiveKit channel. The console connects as `stream-{user_id}` (distinct
    /// from the CLI voice identity), so a plain participant removal by user
    /// id never finds it; this is the kill switch behind `/golive stop`,
    /// grace teardown, and a moderation voice kick.
    pub async fn remove_stream_publisher(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<()> {
        let room = self.livekit_room_name(room_id);
        self.remove_participant(&room, &format!("stream-{user_id}"))
            .await
    }

    /// Force-disconnect a participant from a LiveKit room via the server API.
    /// This is what actually ends an in-progress session on `kick`; the block
    /// set only prevents rejoining. No-op when voice is not configured.
    /// `identity` is the LiveKit identity: `{user_id}` for CLI voice,
    /// `stream-{user_id}` for a go-live console.
    pub async fn remove_participant(
        &self,
        livekit_room: &str,
        identity: &str,
    ) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        let url = self
            .config
            .livekit_api_url
            .as_deref()
            .context("voice enabled without LiveKit API URL")?;
        let http_base = livekit_http_base(url)?;
        let token = self.mint_livekit_token_with_grants(
            &Uuid::new_v4().to_string(),
            "late-mod",
            livekit_room,
            LiveKitTokenGrants {
                room_admin: true,
                room_create: false,
                ingress_admin: false,
                can_publish: false,
                can_publish_sources: None,
                can_subscribe: false,
                can_publish_data: false,
                hidden: false,
            },
        )?;
        let endpoint = format!("{http_base}/twirp/livekit.RoomService/RemoveParticipant");
        let resp = self
            .http
            .post(endpoint)
            .bearer_auth(token)
            .json(&RemoveParticipantRequest {
                room: livekit_room,
                identity,
            })
            .send()
            .await
            .context("failed to call LiveKit RemoveParticipant")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LiveKit RemoveParticipant failed: {status} {body}");
        }
        Ok(())
    }

    /// Create a WHIP ingress for an OBS stream into a stream room's LiveKit
    /// channel. The ingress participant identity is `stream-{user_id}`, the
    /// same identity the go-live console uses, so every existing teardown
    /// path (`remove_stream_publisher`, moderation) finds it. Transcoding is
    /// off: OBS already encodes WebRTC-compatible media (h264+opus) and the
    /// ingress service forwards it as-is, so the server never re-encodes.
    pub async fn create_whip_ingress(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        username: &str,
    ) -> anyhow::Result<WhipIngress> {
        let room = self.livekit_room_name(room_id);
        let info: IngressInfo = self
            .ingress_api_call(
                "CreateIngress",
                &CreateIngressRequest {
                    input_type: "WHIP_INPUT",
                    name: &format!("obs-{user_id}"),
                    room_name: &room,
                    participant_identity: &format!("stream-{user_id}"),
                    participant_name: username,
                    enable_transcoding: false,
                    // Label the mix as program audio. Advisory only: the
                    // label is not guaranteed to survive the
                    // `enable_transcoding: false` passthrough, so both
                    // consumers (CLI runtime, watch page) classify program
                    // audio by the `stream-*` identity, not by this label.
                    audio: IngressAudioOptions {
                        source: "SCREEN_SHARE_AUDIO",
                    },
                },
            )
            .await?;
        if info.url.is_empty() {
            anyhow::bail!(
                "LiveKit returned an ingress without a WHIP URL; is ingress.whip_base_url configured on the server?"
            );
        }
        if info.stream_key.is_empty() {
            anyhow::bail!("LiveKit returned an ingress without a stream key");
        }
        Ok(WhipIngress {
            ingress_id: info.ingress_id,
            url: info.url,
            stream_key: info.stream_key,
        })
    }

    /// Delete an ingress. This is what actually stops OBS from reconnecting:
    /// removing the participant alone leaves the stream key valid and OBS
    /// auto-reconnects through it.
    pub async fn delete_ingress(&self, ingress_id: &str) -> anyhow::Result<()> {
        let _: serde_json::Value = self
            .ingress_api_call("DeleteIngress", &DeleteIngressRequest { ingress_id })
            .await?;
        Ok(())
    }

    /// Every ingress id LiveKit currently holds, publishing or not. An empty
    /// filter lists them all: the boot reconciliation pass uses this to find
    /// stream keys left valid by a previous process.
    pub async fn list_ingress_ids(&self) -> anyhow::Result<Vec<String>> {
        let resp: ListIngressResponse = self
            .ingress_api_call("ListIngress", &ListIngressRequest { ingress_id: "" })
            .await?;
        Ok(resp.items.into_iter().map(|item| item.ingress_id).collect())
    }

    /// Whether an ingress is currently receiving and publishing media.
    /// `Ok(false)` covers every non-publishing state, including an ingress
    /// that no longer exists (deleted out of band).
    pub async fn ingress_publishing(&self, ingress_id: &str) -> anyhow::Result<bool> {
        let resp: ListIngressResponse = self
            .ingress_api_call("ListIngress", &ListIngressRequest { ingress_id })
            .await?;
        let publishing = resp.items.iter().any(|item| {
            item.ingress_id == ingress_id
                && item
                    .state
                    .as_ref()
                    .is_some_and(|state| state.status == "ENDPOINT_PUBLISHING")
        });
        Ok(publishing)
    }

    /// One Twirp call against the LiveKit Ingress API, authorized with a
    /// short-lived `ingressAdmin` token. Server-to-server only.
    async fn ingress_api_call<Req: Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        request: &Req,
    ) -> anyhow::Result<Resp> {
        if !self.config.enabled {
            anyhow::bail!("voice is not configured");
        }
        let url = self
            .config
            .livekit_api_url
            .as_deref()
            .context("voice enabled without LiveKit API URL")?;
        let http_base = livekit_http_base(url)?;
        let token = self.mint_livekit_token_with_grants(
            &Uuid::new_v4().to_string(),
            "late-ingress",
            "",
            LiveKitTokenGrants {
                room_admin: false,
                room_create: false,
                ingress_admin: true,
                can_publish: false,
                can_publish_sources: None,
                can_subscribe: false,
                can_publish_data: false,
                hidden: false,
            },
        )?;
        let endpoint = format!("{http_base}/twirp/livekit.Ingress/{method}");
        let resp = self
            .http
            .post(endpoint)
            .bearer_auth(token)
            .json(request)
            .send()
            .await
            .with_context(|| format!("failed to call LiveKit {method}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LiveKit {method} failed: {status} {body}");
        }
        resp.json()
            .await
            .with_context(|| format!("failed to decode LiveKit {method} response"))
    }

    /// Publisher ticket for the streamer's go-live console page. Publishing
    /// is restricted at the SFU grant level to screen share only: no
    /// browser mic exists anywhere in the system, so voice stays CLI-only
    /// with zero exceptions (a streamer talks through CLI voice like
    /// everyone else). Subscribe is allowed: a non-CLI streamer hears
    /// co-hosts through this page. The identity is `stream-{user_id}`,
    /// distinct from the user's CLI voice identity so a CLI streamer in
    /// voice is not kicked out of LiveKit by their own console connecting.
    pub fn stream_publish_ticket(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        username: &str,
    ) -> anyhow::Result<StreamMediaTicket> {
        if !self.config.enabled {
            anyhow::bail!("voice is not configured");
        }
        if self.is_blocked(user_id) {
            anyhow::bail!("you have been removed from voice by a moderator");
        }
        let room = self.livekit_room_name(room_id);
        let url = self
            .config
            .livekit_url
            .clone()
            .context("voice enabled without LiveKit URL")?;
        let token = self.mint_livekit_token_with_grants(
            &format!("stream-{user_id}"),
            username,
            &room,
            LiveKitTokenGrants {
                room_admin: false,
                room_create: false,
                ingress_admin: false,
                can_publish: true,
                can_publish_sources: Some(&["screen_share", "screen_share_audio"]),
                can_subscribe: true,
                can_publish_data: false,
                hidden: false,
            },
        )?;
        Ok(StreamMediaTicket { room, url, token })
    }

    /// Subscribe-only ticket for an anonymous watch page. `canPublish=false`
    /// is enforced at the SFU grant level: a tampered watch page still
    /// cannot open a mic. `hidden` keeps the anonymous viewer out of LiveKit
    /// participant rosters; the watcher count is served by heartbeats, not
    /// by room presence.
    pub fn stream_watch_ticket(
        &self,
        room_id: Uuid,
        identity: &str,
    ) -> anyhow::Result<StreamMediaTicket> {
        if !self.config.enabled {
            anyhow::bail!("voice is not configured");
        }
        let room = self.livekit_room_name(room_id);
        let url = self
            .config
            .livekit_url
            .clone()
            .context("voice enabled without LiveKit URL")?;
        let token = self.mint_livekit_token_with_grants(
            identity,
            "viewer",
            &room,
            LiveKitTokenGrants {
                room_admin: false,
                room_create: false,
                ingress_admin: false,
                can_publish: false,
                can_publish_sources: None,
                can_subscribe: true,
                can_publish_data: false,
                hidden: true,
            },
        )?;
        Ok(StreamMediaTicket { room, url, token })
    }

    fn mint_livekit_token(
        &self,
        user_id: Uuid,
        username: &str,
        room: &str,
    ) -> anyhow::Result<String> {
        self.mint_livekit_token_with_grants(
            &user_id.to_string(),
            username,
            room,
            LiveKitTokenGrants {
                room_admin: false,
                room_create: false,
                ingress_admin: false,
                can_publish: true,
                can_publish_sources: None,
                can_subscribe: true,
                can_publish_data: true,
                hidden: false,
            },
        )
    }

    fn mint_livekit_token_with_grants(
        &self,
        subject: &str,
        name: &str,
        room: &str,
        grants: LiveKitTokenGrants,
    ) -> anyhow::Result<String> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .context("voice enabled without LiveKit API key")?;
        let api_secret = self
            .config
            .api_secret
            .as_ref()
            .context("voice enabled without LiveKit API secret")?;
        let now = Utc::now().timestamp();
        let claims = LiveKitClaims {
            iss: api_key,
            sub: subject,
            name,
            nbf: now.saturating_sub(5),
            exp: now + 60 * 60,
            video: LiveKitVideoGrant {
                room,
                room_join: !(grants.room_admin || grants.ingress_admin),
                room_admin: grants.room_admin,
                ingress_admin: grants.ingress_admin,
                room_create: grants.room_create,
                can_publish: grants.can_publish,
                can_publish_sources: grants.can_publish_sources,
                can_subscribe: grants.can_subscribe,
                can_publish_data: grants.can_publish_data,
                hidden: grants.hidden,
            },
        };

        let header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&JwtHeader {
            alg: "HS256",
            typ: "JWT",
        })?);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
        let signing_input = format!("{header}.{payload}");
        let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes())
            .context("failed to initialize LiveKit token signer")?;
        mac.update(signing_input.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{signing_input}.{signature}"))
    }

    fn publish_snapshot(&self) {
        let rooms = {
            let inner = self.inner.lock_recover();
            inner
                .rooms
                .iter()
                .map(|(room_id, participants)| {
                    let mut list = participants.values().cloned().collect::<Vec<_>>();
                    list.sort_by(|a, b| {
                        a.username
                            .to_ascii_lowercase()
                            .cmp(&b.username.to_ascii_lowercase())
                            .then_with(|| a.user_id.cmp(&b.user_id))
                    });
                    (*room_id, list)
                })
                .collect::<HashMap<_, _>>()
        };
        let _ = self.tx.send(VoiceSnapshot {
            enabled: self.config.enabled,
            livekit_url: self.config.livekit_url.clone(),
            rooms,
        });
    }
}

/// Convert a LiveKit ws(s):// signalling URL to the http(s):// base used by its
/// server API.
fn livekit_http_base(url: &str) -> anyhow::Result<String> {
    let trimmed = url.trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("wss://") {
        Ok(format!("https://{rest}"))
    } else if let Some(rest) = trimmed.strip_prefix("ws://") {
        Ok(format!("http://{rest}"))
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else {
        anyhow::bail!("unrecognized LiveKit URL scheme: {url}");
    }
}

async fn ensure_user_can_join_voice(
    client: &tokio_postgres::Client,
    channel: &VoiceChannel,
    user_id: Uuid,
) -> anyhow::Result<()> {
    let chat_room_id = match channel.target_kind.as_str() {
        TARGET_CHAT_ROOM => channel.target_id,
        other => anyhow::bail!("unknown voice target kind: {other}"),
    };

    if !ChatRoomMember::is_member(client, chat_room_id, user_id).await? {
        anyhow::bail!("you are not a member of this voice room");
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct LiveKitTokenGrants {
    room_admin: bool,
    room_create: bool,
    /// Grants the LiveKit Ingress API (create/list/delete). Admin-only
    /// tokens minted for server-to-server calls, never handed to a client.
    ingress_admin: bool,
    can_publish: bool,
    /// LiveKit publish-source restriction (`canPublishSources`). `None`
    /// means the grant does not restrict sources.
    can_publish_sources: Option<&'static [&'static str]>,
    can_subscribe: bool,
    can_publish_data: bool,
    /// Hidden participants can subscribe but never appear in rosters. Used
    /// for anonymous watch-page viewers.
    hidden: bool,
}

#[derive(Serialize)]
struct RemoveParticipantRequest<'a> {
    room: &'a str,
    identity: &'a str,
}

// LiveKit's Twirp endpoints speak proto field names on the wire (snake_case:
// `ingress_id`, `stream_key`), not protojson camelCase. Requests are parsed
// leniently (either form works) but responses are emitted snake_case only, so
// these structs keep the raw Rust field names with no renames.
#[derive(Serialize)]
struct CreateIngressRequest<'a> {
    input_type: &'a str,
    name: &'a str,
    room_name: &'a str,
    participant_identity: &'a str,
    participant_name: &'a str,
    enable_transcoding: bool,
    audio: IngressAudioOptions<'a>,
}

/// Track-source label for the ingress's published audio. The OBS mix is
/// program audio, not a voice: the CLI voice runtime and the watch page both
/// discriminate on this label (mic = voice, everything else = program), so
/// `SCREEN_SHARE_AUDIO` here is load-bearing, not cosmetic.
#[derive(Serialize)]
struct IngressAudioOptions<'a> {
    source: &'a str,
}

#[derive(Serialize)]
struct DeleteIngressRequest<'a> {
    ingress_id: &'a str,
}

#[derive(Serialize)]
struct ListIngressRequest<'a> {
    ingress_id: &'a str,
}

#[derive(Deserialize)]
struct ListIngressResponse {
    #[serde(default)]
    items: Vec<IngressInfo>,
}

#[derive(Deserialize)]
struct IngressInfo {
    #[serde(default)]
    ingress_id: String,
    #[serde(default)]
    stream_key: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    state: Option<IngressState>,
}

#[derive(Deserialize)]
struct IngressState {
    /// Proto enum as its JSON string name; `ENDPOINT_PUBLISHING` is the only
    /// state that counts as media flowing.
    #[serde(default)]
    status: String,
}

#[derive(Serialize)]
struct JwtHeader<'a> {
    alg: &'a str,
    typ: &'a str,
}

#[derive(Serialize)]
struct LiveKitClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    name: &'a str,
    nbf: i64,
    exp: i64,
    video: LiveKitVideoGrant<'a>,
}

#[derive(Serialize)]
struct LiveKitVideoGrant<'a> {
    room: &'a str,
    #[serde(rename = "roomJoin")]
    room_join: bool,
    #[serde(rename = "roomAdmin")]
    room_admin: bool,
    #[serde(rename = "ingressAdmin", skip_serializing_if = "std::ops::Not::not")]
    ingress_admin: bool,
    #[serde(rename = "roomCreate")]
    room_create: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canPublishSources", skip_serializing_if = "Option::is_none")]
    can_publish_sources: Option<&'static [&'static str]>,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    hidden: bool,
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
