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
use serde::Serialize;
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
    pub livekit_url: Option<String>,
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
            api_key: None,
            api_secret: None,
            room_name: "late-voice".to_string(),
        }
    }

    pub fn enabled(
        livekit_url: String,
        api_key: String,
        api_secret: String,
        room_name: String,
    ) -> anyhow::Result<Self> {
        if livekit_url.trim().is_empty() {
            anyhow::bail!("LATE_LIVEKIT_URL must not be empty when voice is enabled");
        }
        if api_key.trim().is_empty() {
            anyhow::bail!("LATE_LIVEKIT_API_KEY must not be empty when voice is enabled");
        }
        if api_secret.trim().is_empty() {
            anyhow::bail!("LATE_LIVEKIT_API_SECRET must not be empty when voice is enabled");
        }
        if room_name.trim().is_empty() {
            anyhow::bail!("LATE_VOICE_ROOM must not be empty when voice is enabled");
        }
        Ok(Self {
            enabled: true,
            livekit_url: Some(livekit_url),
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
            http: reqwest::Client::new(),
        }
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

    /// Force-disconnect a participant from a LiveKit room via the server API.
    /// This is what actually ends an in-progress session on `kick`; the block
    /// set only prevents rejoining. No-op when voice is not configured.
    pub async fn remove_participant(
        &self,
        livekit_room: &str,
        user_id: Uuid,
    ) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        let url = self
            .config
            .livekit_url
            .as_deref()
            .context("voice enabled without LiveKit URL")?;
        let http_base = livekit_http_base(url)?;
        let token = self.mint_livekit_token_with_grants(
            &Uuid::new_v4().to_string(),
            "late-mod",
            livekit_room,
            LiveKitTokenGrants {
                room_admin: true,
                room_create: false,
                can_publish: false,
                can_subscribe: false,
                can_publish_data: false,
            },
        )?;
        let endpoint = format!("{http_base}/twirp/livekit.RoomService/RemoveParticipant");
        let resp = self
            .http
            .post(endpoint)
            .bearer_auth(token)
            .json(&RemoveParticipantRequest {
                room: livekit_room,
                identity: &user_id.to_string(),
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
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
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
                room_join: !grants.room_admin,
                room_admin: grants.room_admin,
                room_create: grants.room_create,
                can_publish: grants.can_publish,
                can_subscribe: grants.can_subscribe,
                can_publish_data: grants.can_publish_data,
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
    can_publish: bool,
    can_subscribe: bool,
    can_publish_data: bool,
}

#[derive(Serialize)]
struct RemoveParticipantRequest<'a> {
    room: &'a str,
    identity: &'a str,
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
    #[serde(rename = "roomCreate")]
    room_create: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;

