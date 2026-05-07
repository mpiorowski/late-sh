use std::{collections::HashSet, net::IpAddr};

use chrono::{DateTime, Utc};
use late_core::MutexRecover;
use uuid::Uuid;

use crate::{
    authz::Permissions,
    session::{SessionMessage, SessionRegistry},
    state::ActiveUsers,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServerBanSnapshot {
    pub(crate) username: String,
    pub(crate) fingerprint: Option<String>,
    pub(crate) peer_ip: Option<IpAddr>,
}

#[derive(Clone, Default)]
pub(crate) struct ModerationSessionEffects {
    active_users: Option<ActiveUsers>,
    session_registry: Option<SessionRegistry>,
}

impl ModerationSessionEffects {
    pub(crate) fn new(
        active_users: Option<ActiveUsers>,
        session_registry: Option<SessionRegistry>,
    ) -> Self {
        Self {
            active_users,
            session_registry,
        }
    }

    pub(crate) fn snapshot_for_server_ban(&self, user_id: Uuid) -> Option<ServerBanSnapshot> {
        let active_users = self.active_users.as_ref()?;
        let guard = active_users.lock_recover();
        let user = guard.get(&user_id)?;
        let session = user
            .sessions
            .iter()
            .find(|session| session.peer_ip.is_some())?;
        Some(ServerBanSnapshot {
            username: user.username.clone(),
            fingerprint: session.fingerprint.clone(),
            peer_ip: session.peer_ip,
        })
    }

    pub(crate) async fn notify_room_removed(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        slug: String,
        message: String,
    ) -> usize {
        let mut notified = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if self
                .send(
                    &token,
                    SessionMessage::RoomRemoved {
                        room_id,
                        slug: slug.clone(),
                        message: message.clone(),
                    },
                )
                .await
            {
                notified += 1;
            }
        }
        notified
    }

    pub(crate) async fn terminate_user_sessions(&self, user_id: Uuid, reason: &str) -> usize {
        let mut terminated = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if self
                .send(
                    &token,
                    SessionMessage::Terminate {
                        reason: reason.to_string(),
                    },
                )
                .await
            {
                terminated += 1;
            }
        }
        terminated
    }

    pub(crate) async fn notify_artboard_ban_changed(
        &self,
        user_id: Uuid,
        banned: bool,
        expires_at: Option<DateTime<Utc>>,
    ) -> usize {
        let mut notified = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if self
                .send(
                    &token,
                    SessionMessage::ArtboardBanChanged { banned, expires_at },
                )
                .await
            {
                notified += 1;
            }
        }
        notified
    }

    pub(crate) async fn notify_permissions_changed(
        &self,
        user_id: Uuid,
        permissions: Permissions,
    ) -> usize {
        let mut notified = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if self
                .send(&token, SessionMessage::PermissionsChanged { permissions })
                .await
            {
                notified += 1;
            }
        }
        notified
    }

    pub(crate) fn update_active_username(&self, user_id: Uuid, username: &str) -> bool {
        let Some(active_users) = self.active_users.as_ref() else {
            return false;
        };
        let mut guard = active_users.lock_recover();
        let Some(user) = guard.get_mut(&user_id) else {
            return false;
        };
        user.username = username.to_string();
        true
    }

    async fn send(&self, token: &str, msg: SessionMessage) -> bool {
        let Some(registry) = self.session_registry.as_ref() else {
            return false;
        };
        registry.send_message(token, msg).await
    }

    fn session_tokens_for_user_id(&self, user_id: Uuid) -> Vec<String> {
        let Some(active_users) = self.active_users.as_ref() else {
            return Vec::new();
        };
        let guard = active_users.lock_recover();
        guard
            .get(&user_id)
            .map(|user| unique_session_tokens(user.sessions.iter().map(|session| &session.token)))
            .unwrap_or_default()
    }
}

fn unique_session_tokens<'a>(tokens: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tokens
        .filter(|token| seen.insert((*token).clone()))
        .cloned()
        .collect()
}
