use anyhow::Result;
use chrono::Utc;
use late_core::{
    db::Db,
    models::{
        artboard_ban::ArtboardBan,
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        moderation_audit_log::ModerationAuditLog,
        room_ban::RoomBan,
        server_ban::{ServerBan, ServerBanActivation},
        user::User,
    },
};
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::authz::{Caps, Permissions, Tier};
use crate::moderation::command::{
    ArtboardAction, ModCommand, RoleAction, RoomModAction, ServerUserAction, mod_help_lines,
    normalize_mod_slug, parse_mod_command, strip_user_prefix,
};
use crate::moderation::event::ModerationEvent;
use crate::moderation::session_effects::ModerationSessionEffects;

#[derive(Clone)]
pub(crate) struct ModerationService {
    db: Db,
    effects: ModerationSessionEffects,
    event_tx: broadcast::Sender<ModerationEvent>,
    force_admin: bool,
}

struct RoomModRequest {
    action: RoomModAction,
    slug: String,
    username: String,
    duration: Option<chrono::Duration>,
    reason: String,
}

impl ModerationService {
    pub(crate) fn new(
        db: Db,
        effects: ModerationSessionEffects,
        event_tx: broadcast::Sender<ModerationEvent>,
        force_admin: bool,
    ) -> Self {
        Self {
            db,
            effects,
            event_tx,
            force_admin,
        }
    }

    pub(crate) async fn run_command(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        input: &str,
    ) -> Result<Vec<String>> {
        let command = parse_mod_command(input)?;
        match command {
            ModCommand::Help { topic } => Ok(mod_help_lines(topic.as_deref())),
            ModCommand::User { username } => self.user_detail(permissions, &username).await,
            ModCommand::RoomAction {
                action,
                slug,
                username,
                duration,
                reason,
            } => {
                self.room_action(
                    actor_user_id,
                    permissions,
                    RoomModRequest {
                        action,
                        slug,
                        username,
                        duration,
                        reason,
                    },
                )
                .await
            }
            ModCommand::ServerUser {
                action,
                username,
                duration,
                reason,
            } => {
                self.server_user(
                    actor_user_id,
                    permissions,
                    action,
                    &username,
                    duration,
                    reason,
                )
                .await
            }
            ModCommand::Artboard {
                action,
                username,
                duration,
                reason,
            } => {
                self.artboard(
                    actor_user_id,
                    permissions,
                    action,
                    &username,
                    duration,
                    reason,
                )
                .await
            }
            ModCommand::Role { action, username } => {
                self.role(actor_user_id, permissions, action, &username)
                    .await
            }
        }
    }

    async fn user_detail(&self, permissions: Permissions, username: &str) -> Result<Vec<String>> {
        ensure_mod_surface(permissions)?;
        let client = self.db.get().await?;
        let user = find_user_by_mod_name(&client, username).await?;
        let server_ban = ServerBan::find_active_for_user_id(&client, user.id).await?;
        let artboard_ban = ArtboardBan::find_active_for_user(&client, user.id).await?;
        Ok(vec![
            format!("@{}", user.username),
            format!("id: {}", user.id),
            format!("admin: {}", user.is_admin),
            format!("moderator: {}", user.is_moderator),
            format!("created: {}", user.created.format("%Y-%m-%d %H:%M UTC")),
            format!("last_seen: {}", user.last_seen.format("%Y-%m-%d %H:%M UTC")),
            format!("server_banned: {}", server_ban.is_some()),
            format!("artboard_banned: {}", artboard_ban.is_some()),
        ])
    }

    async fn room_action(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        request: RoomModRequest,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let room = find_room_by_mod_slug(&client, &request.slug).await?;
        let target = find_user_by_mod_name(&client, &request.username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = tier_for_user(&target);
        let cap = match request.action {
            RoomModAction::Kick => Caps::KICK_FROM_ROOM,
            RoomModAction::Ban => Caps::BAN_FROM_ROOM,
            RoomModAction::Unban => Caps::UNBAN_FROM_ROOM,
        };
        ensure_can(permissions, cap, target_tier)?;
        let room_slug = room.slug.clone().unwrap_or_else(|| room.kind.clone());
        let tx = client.transaction().await?;
        match request.action {
            RoomModAction::Kick => {
                ChatRoomMember::leave(&tx, room.id, target.id).await?;
            }
            RoomModAction::Ban => {
                let expires_at = request.duration.map(|d| Utc::now() + d);
                RoomBan::activate(
                    &tx,
                    room.id,
                    target.id,
                    actor_user_id,
                    &request.reason,
                    expires_at,
                )
                .await?;
                ChatRoomMember::leave(&tx, room.id, target.id).await?;
            }
            RoomModAction::Unban => {
                RoomBan::delete_for_room_and_user(&tx, room.id, target.id).await?;
            }
        }
        let audit_action = match request.action {
            RoomModAction::Kick => "room_kick",
            RoomModAction::Ban => "room_ban",
            RoomModAction::Unban => "room_unban",
        };
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(false),
            actor_user_id,
            audit_action,
            "user",
            Some(target.id),
            json!({ "room_id": room.id, "room_slug": room.slug, "reason": request.reason }),
        )
        .await?;
        tx.commit().await?;
        let notified_sessions =
            if matches!(request.action, RoomModAction::Kick | RoomModAction::Ban) {
                let notified = self
                    .effects
                    .notify_room_removed(
                        target.id,
                        room.id,
                        room_slug.clone(),
                        match request.action {
                            RoomModAction::Kick => "Removed from room".to_string(),
                            RoomModAction::Ban => "Banned from room".to_string(),
                            RoomModAction::Unban => unreachable!(),
                        },
                    )
                    .await;
                if notified > 0 {
                    tracing::info!(
                        room_id = %room.id,
                        target_user_id = %target.id,
                        notified,
                        "room moderation command notified active sessions"
                    );
                }
                notified
            } else {
                0
            };
        let _ = self.event_tx.send(ModerationEvent::RoomAction {
            actor_user_id,
            target_user_id: target.id,
            room_id: room.id,
            room_slug: room_slug.clone(),
            action: request.action,
            reason: request.reason,
            notified_sessions,
        });
        Ok(vec![format!(
            "{} @{} in #{}",
            request.action.past_tense(),
            target.username,
            room_slug
        )])
    }

    async fn server_user(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: ServerUserAction,
        username: &str,
        duration: Option<chrono::Duration>,
        reason: String,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target = find_user_by_mod_name(&client, username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = tier_for_user(&target);
        let cap = match action {
            ServerUserAction::Kick => Caps::KICK_USER,
            ServerUserAction::Ban => cap_for_server_ban(duration),
            ServerUserAction::Unban => Caps::UNBAN_USER,
        };
        ensure_can(permissions, cap, target_tier)?;
        let active_snapshot = self.effects.snapshot_for_server_ban(target.id);
        let tx = client.transaction().await?;
        match action {
            ServerUserAction::Kick => {}
            ServerUserAction::Ban => {
                let expires_at = duration.map(|d| Utc::now() + d);
                let ip_address = active_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.peer_ip)
                    .map(|ip| ip.to_string());
                ServerBan::activate(
                    &tx,
                    ServerBanActivation {
                        target_user_id: target.id,
                        fingerprint: Some(&target.fingerprint),
                        ip_address: ip_address.as_deref(),
                        snapshot_username: Some(&target.username),
                        actor_user_id,
                        reason: &reason,
                        expires_at,
                    },
                )
                .await?;
            }
            ServerUserAction::Unban => {
                ServerBan::delete_active_for_user(&tx, target.id, &target.fingerprint).await?;
            }
        }
        let audit_action = match action {
            ServerUserAction::Kick => "server_kick",
            ServerUserAction::Ban => "server_ban",
            ServerUserAction::Unban => "server_unban",
        };
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(false),
            actor_user_id,
            audit_action,
            "user",
            Some(target.id),
            json!({ "reason": reason }),
        )
        .await?;
        tx.commit().await?;
        let terminated_sessions =
            if matches!(action, ServerUserAction::Kick | ServerUserAction::Ban) {
                let terminated = self
                    .effects
                    .terminate_user_sessions(target.id, action.termination_reason())
                    .await;
                tracing::info!(
                    target_user_id = %target.id,
                    action = action.audit_name(),
                    terminated,
                    "server moderation command terminated active sessions"
                );
                terminated
            } else {
                0
            };
        let _ = self.event_tx.send(ModerationEvent::ServerUserAction {
            actor_user_id,
            target_user_id: target.id,
            target_username: target.username.clone(),
            action,
            reason,
            terminated_sessions,
        });
        Ok(vec![format!(
            "{} @{}",
            action.past_tense(),
            target.username
        )])
    }

    async fn artboard(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: ArtboardAction,
        username: &str,
        duration: Option<chrono::Duration>,
        reason: String,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target = find_user_by_mod_name(&client, username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = tier_for_user(&target);
        let cap = match action {
            ArtboardAction::Ban => Caps::BAN_FROM_ARTBOARD,
            ArtboardAction::Unban => Caps::UNBAN_FROM_ARTBOARD,
        };
        ensure_can(permissions, cap, target_tier)?;
        let expires_at = matches!(action, ArtboardAction::Ban)
            .then(|| duration.map(|d| Utc::now() + d))
            .flatten();
        let tx = client.transaction().await?;
        match action {
            ArtboardAction::Ban => {
                ArtboardBan::activate(&tx, target.id, actor_user_id, &reason, expires_at).await?;
            }
            ArtboardAction::Unban => {
                ArtboardBan::delete_for_user(&tx, target.id).await?;
            }
        }
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(false),
            actor_user_id,
            action.audit_name(),
            "user",
            Some(target.id),
            json!({ "reason": reason }),
        )
        .await?;
        tx.commit().await?;
        let banned = matches!(action, ArtboardAction::Ban);
        let notified_sessions = self
            .effects
            .notify_artboard_ban_changed(target.id, banned, expires_at)
            .await;
        if notified_sessions > 0 {
            tracing::info!(
                target_user_id = %target.id,
                banned,
                notified = notified_sessions,
                "artboard moderation command updated active sessions"
            );
        }
        let _ = self.event_tx.send(ModerationEvent::ArtboardAction {
            actor_user_id,
            target_user_id: target.id,
            action,
            banned,
            expires_at,
            reason,
            notified_sessions,
        });
        Ok(vec![format!(
            "{} @{}",
            action.past_tense(),
            target.username
        )])
    }

    async fn role(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: RoleAction,
        username: &str,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target = find_user_by_mod_name(&client, username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = tier_for_user(&target);
        let cap = match action {
            RoleAction::GrantMod => Caps::GRANT_MOD,
            RoleAction::RevokeMod => Caps::REVOKE_MOD,
        };
        ensure_can(permissions, cap, target_tier)?;
        let (new_is_moderator, label) = match action {
            RoleAction::GrantMod => (true, "granted moderator to"),
            RoleAction::RevokeMod => (false, "revoked moderator from"),
        };
        let tx = client.transaction().await?;
        User::set_moderator(&tx, target.id, new_is_moderator).await?;
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(false),
            actor_user_id,
            action.audit_name(),
            "user",
            Some(target.id),
            json!({}),
        )
        .await?;
        tx.commit().await?;
        let permissions = Permissions::new(target.is_admin || self.force_admin, new_is_moderator);
        let notified_sessions = self
            .effects
            .notify_permissions_changed(target.id, permissions)
            .await;
        if notified_sessions > 0 {
            tracing::info!(
                target_user_id = %target.id,
                notified = notified_sessions,
                "role moderation command updated active session permissions"
            );
        }
        let _ = self.event_tx.send(ModerationEvent::RoleAction {
            actor_user_id,
            target_user_id: target.id,
            action,
            permissions,
            notified_sessions,
        });
        Ok(vec![format!("{label} @{}", target.username)])
    }
}

pub(crate) fn ensure_mod_surface(permissions: Permissions) -> Result<()> {
    ensure_has(permissions, Caps::OPEN_MOD_SURFACE)
}

pub(crate) fn ensure_has(permissions: Permissions, cap: Caps) -> Result<()> {
    if permissions.has(cap) {
        Ok(())
    } else {
        anyhow::bail!("moderator or admin only")
    }
}

pub(crate) fn ensure_can(permissions: Permissions, cap: Caps, target: Tier) -> Result<()> {
    if permissions.can(cap, target) {
        Ok(())
    } else {
        anyhow::bail!("moderator or admin only")
    }
}

pub(crate) fn ensure_not_self(actor_user_id: Uuid, target_user_id: Uuid) -> Result<()> {
    if actor_user_id == target_user_id {
        anyhow::bail!("cannot target yourself");
    }
    Ok(())
}

pub(crate) fn tier_for_user(user: &User) -> Tier {
    Tier::from_user_flags(user.is_admin, user.is_moderator)
}

pub(crate) async fn target_tier_for_user_id(
    client: &tokio_postgres::Client,
    target_user_id: Uuid,
) -> Result<Tier> {
    let author = User::get(client, target_user_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("target user not found"))?;
    Ok(tier_for_user(&author))
}

pub(crate) fn ensure_message_permission(
    permissions: Permissions,
    is_owner: bool,
    cap: Caps,
    target_tier: Tier,
) -> Result<()> {
    if is_owner || permissions.can(cap, target_tier) {
        Ok(())
    } else {
        anyhow::bail!("cannot edit or delete this message")
    }
}

pub(crate) const fn cap_for_server_ban(duration: Option<chrono::Duration>) -> Caps {
    if duration.is_some() {
        Caps::TEMP_BAN_USER
    } else {
        Caps::PERMA_BAN_USER
    }
}

async fn find_user_by_mod_name(client: &tokio_postgres::Client, username: &str) -> Result<User> {
    User::find_by_username(client, &strip_user_prefix(username))
        .await?
        .ok_or_else(|| anyhow::anyhow!("user not found: @{username}"))
}

async fn find_room_by_mod_slug(client: &tokio_postgres::Client, slug: &str) -> Result<ChatRoom> {
    let slug = normalize_mod_slug(slug)?;
    ChatRoom::find_non_dm_by_slug(client, &slug)
        .await?
        .ok_or_else(|| anyhow::anyhow!("room not found: #{slug}"))
}
