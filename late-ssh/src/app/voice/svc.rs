use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use late_core::MutexRecover;
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VoiceSnapshot {
    pub enabled: bool,
    pub room_name: String,
    pub livekit_url: Option<String>,
    pub participants: Vec<VoiceParticipant>,
}

impl VoiceSnapshot {
    pub fn participant(&self, user_id: Uuid) -> Option<&VoiceParticipant> {
        self.participants
            .iter()
            .find(|participant| participant.user_id == user_id)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceListenTicket {
    pub room: String,
    pub url: String,
    pub token: String,
}

#[derive(Clone)]
pub struct VoiceService {
    config: VoiceConfig,
    inner: Arc<Mutex<VoiceInner>>,
    tx: watch::Sender<VoiceSnapshot>,
}

#[derive(Default)]
struct VoiceInner {
    participants: HashMap<Uuid, VoiceParticipant>,
    /// Users a moderator has removed from voice. While blocked, no join ticket
    /// is minted and any self-reported presence is dropped. Runtime-only - the
    /// block clears on `allow` or a server restart (it is not persisted).
    blocked: HashSet<Uuid>,
}

impl VoiceService {
    pub fn new(config: VoiceConfig) -> Self {
        let snapshot = VoiceSnapshot {
            enabled: config.enabled,
            room_name: config.room_name.clone(),
            livekit_url: config.livekit_url.clone(),
            participants: Vec::new(),
        };
        let (tx, _) = watch::channel(snapshot);
        Self {
            config,
            inner: Arc::new(Mutex::new(VoiceInner::default())),
            tx,
        }
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

    pub fn join_ticket(
        &self,
        user_id: Uuid,
        username: &str,
        muted: bool,
        deafened: bool,
    ) -> anyhow::Result<VoiceJoinTicket> {
        if !self.config.enabled {
            anyhow::bail!("Voice is not configured");
        }
        if self.is_blocked(user_id) {
            anyhow::bail!("You have been removed from voice by a moderator");
        }

        let room = self.config.room_name.clone();
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

    pub fn listen_ticket(&self) -> anyhow::Result<VoiceListenTicket> {
        if !self.config.enabled {
            anyhow::bail!("Voice is not configured");
        }

        let room = self.config.room_name.clone();
        let url = self
            .config
            .livekit_url
            .clone()
            .context("voice enabled without LiveKit URL")?;
        let identity = format!("web-listener-{}", Uuid::new_v4());
        let token = self.mint_livekit_token_with_grants(
            &identity,
            "web listener",
            &room,
            LiveKitTokenGrants {
                room_create: false,
                can_publish: false,
                can_subscribe: true,
                can_publish_data: false,
            },
        )?;

        Ok(VoiceListenTicket { room, url, token })
    }

    pub fn apply_client_state(&self, user_id: Uuid, username: String, state: VoiceClientState) {
        if !state.joined {
            self.leave(user_id);
            return;
        }

        if state.room.as_deref() != Some(self.config.room_name.as_str()) {
            self.leave(user_id);
            return;
        }

        // A moderator-blocked user stays out even if their client keeps
        // reporting presence.
        if self.is_blocked(user_id) {
            self.leave(user_id);
            return;
        }

        {
            let mut inner = self.inner.lock_recover();
            inner.participants.insert(
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
            inner.participants.remove(&user_id).is_some()
        };
        if removed {
            self.publish_snapshot();
        }
    }

    /// Moderator action: remove a user from voice now and block them from
    /// rejoining (no join ticket is minted) until `allow` lifts it or the server
    /// restarts. Enforcement is the token gate in `join_ticket`, so a blocked
    /// user genuinely cannot publish or subscribe. Runtime-only; not persisted.
    pub fn kick(&self, user_id: Uuid) -> bool {
        let changed = {
            let mut inner = self.inner.lock_recover();
            let newly_blocked = inner.blocked.insert(user_id);
            let was_present = inner.participants.remove(&user_id).is_some();
            newly_blocked || was_present
        };
        if changed {
            self.publish_snapshot();
        }
        changed
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
        user_id: Uuid,
        username: String,
        muted: bool,
        deafened: bool,
        speaking: bool,
    ) {
        self.apply_client_state(
            user_id,
            username,
            VoiceClientState {
                joined: true,
                room: Some(self.config.room_name.clone()),
                muted,
                deafened,
                speaking,
            },
        );
    }

    pub fn prune_stale(&self, ttl: Duration) {
        let cutoff = Utc::now() - ttl;
        let pruned = {
            let mut inner = self.inner.lock_recover();
            let before = inner.participants.len();
            inner
                .participants
                .retain(|_, participant| participant.updated_at >= cutoff);
            inner.participants.len() != before
        };
        if pruned {
            self.publish_snapshot();
        }
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
                room_join: true,
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
        let mut participants = {
            let inner = self.inner.lock_recover();
            inner.participants.values().cloned().collect::<Vec<_>>()
        };
        participants.sort_by(|a, b| {
            a.username
                .to_ascii_lowercase()
                .cmp(&b.username.to_ascii_lowercase())
                .then_with(|| a.user_id.cmp(&b.user_id))
        });
        let _ = self.tx.send(VoiceSnapshot {
            enabled: self.config.enabled,
            room_name: self.config.room_name.clone(),
            livekit_url: self.config.livekit_url.clone(),
            participants,
        });
    }
}

#[derive(Clone, Copy)]
struct LiveKitTokenGrants {
    room_create: bool,
    can_publish: bool,
    can_subscribe: bool,
    can_publish_data: bool,
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
mod tests {
    use super::*;
    use serde_json::Value;

    fn enabled_service() -> VoiceService {
        VoiceService::new(
            VoiceConfig::enabled(
                "ws://localhost:7880".to_string(),
                "devkey".to_string(),
                "secret".to_string(),
                "late-voice".to_string(),
            )
            .expect("voice config"),
        )
    }

    fn claims_from_token(token: &str) -> Value {
        let payload = token.split('.').nth(1).expect("jwt payload");
        let bytes = URL_SAFE_NO_PAD
            .decode(payload.as_bytes())
            .expect("decode payload");
        serde_json::from_slice(&bytes).expect("claims json")
    }

    #[test]
    fn join_ticket_does_not_grant_room_create() {
        let service = enabled_service();
        let ticket = service
            .join_ticket(Uuid::from_u128(1), "alice", true, false)
            .expect("join ticket");
        let claims = claims_from_token(&ticket.token);

        assert_eq!(claims["video"]["roomCreate"], false);
        assert_eq!(claims["video"]["roomJoin"], true);
        assert_eq!(claims["video"]["canPublish"], true);
        assert_eq!(claims["video"]["canSubscribe"], true);
    }

    #[test]
    fn listen_ticket_is_subscribe_only_without_room_create() {
        let service = enabled_service();
        let ticket = service.listen_ticket().expect("listen ticket");
        let claims = claims_from_token(&ticket.token);

        assert_eq!(claims["video"]["roomCreate"], false);
        assert_eq!(claims["video"]["roomJoin"], true);
        assert_eq!(claims["video"]["canPublish"], false);
        assert_eq!(claims["video"]["canSubscribe"], true);
        assert_eq!(claims["video"]["canPublishData"], false);
    }

    #[test]
    fn kicked_user_is_denied_a_join_ticket_until_allowed() {
        let service = enabled_service();
        let user = Uuid::from_u128(7);

        assert!(service.join_ticket(user, "spammer", true, false).is_ok());
        assert!(service.kick(user));
        assert!(service.is_blocked(user));
        // The token gate is the enforcement: no ticket means no LiveKit access.
        assert!(service.join_ticket(user, "spammer", true, false).is_err());

        assert!(service.allow(user));
        assert!(!service.is_blocked(user));
        assert!(service.join_ticket(user, "spammer", true, false).is_ok());
    }

    #[test]
    fn kick_removes_a_present_participant_and_keeps_them_out() {
        let service = enabled_service();
        let _rx = service.subscribe();
        let user = Uuid::from_u128(9);

        service.update_local_state(user, "noisy".to_string(), false, false, true);
        assert!(service.snapshot().participant(user).is_some());

        service.kick(user);
        assert!(service.snapshot().participant(user).is_none());

        // A blocked client that keeps reporting presence is dropped, not re-added.
        service.update_local_state(user, "noisy".to_string(), false, false, true);
        assert!(service.snapshot().participant(user).is_none());
    }
}
