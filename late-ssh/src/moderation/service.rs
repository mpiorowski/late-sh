use anyhow::Result;
use chrono::Utc;
use late_core::{
    db::Db,
    models::{
        artboard_ban::{ArtboardBan, ArtboardBanListItem},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        game_room::GameRoom,
        moderation_audit_log::{ModerationAuditLog, ModerationAuditLogListItem},
        room_ban::{RoomBan, RoomBanListItem},
        server_ban::{ServerBan, ServerBanActivation, ServerBanListItem},
        user::User,
    },
};
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::authz::{Caps, Permissions, Tier};
use crate::moderation::command::{
    ArtboardAction, BanListScope, ModCommand, RoleAction, RoomModAction, ServerUserAction,
    mod_help_lines, normalize_mod_slug, parse_mod_command, strip_user_prefix,
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
            ModCommand::Bans { scope, limit } => self.list_bans(permissions, scope, limit).await,
            ModCommand::Audit { limit } => self.list_audit(permissions, limit).await,
            ModCommand::RenameRoom { slug, new_slug } => {
                self.rename_room(actor_user_id, permissions, &slug, &new_slug)
                    .await
            }
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

    async fn list_bans(
        &self,
        permissions: Permissions,
        scope: BanListScope,
        limit: i64,
    ) -> Result<Vec<String>> {
        ensure_mod_surface(permissions)?;
        let client = self.db.get().await?;
        match scope {
            BanListScope::All => {
                let server = ServerBan::active_with_usernames(&client, limit).await?;
                let artboard = ArtboardBan::active_with_usernames(&client, limit).await?;
                let room = RoomBan::active_with_usernames(&client, limit).await?;
                if server.is_empty() && artboard.is_empty() && room.is_empty() {
                    return Ok(vec!["no active bans".to_string()]);
                }
                let mut lines = vec![format!("active bans (limit {limit} per section)")];
                append_section(
                    &mut lines,
                    "server bans",
                    server
                        .iter()
                        .map(format_server_ban_item)
                        .collect::<Vec<_>>(),
                );
                append_section(
                    &mut lines,
                    "artboard bans",
                    artboard
                        .iter()
                        .map(format_artboard_ban_item)
                        .collect::<Vec<_>>(),
                );
                append_section(
                    &mut lines,
                    "room bans",
                    room.iter().map(format_room_ban_item).collect::<Vec<_>>(),
                );
                Ok(lines)
            }
            BanListScope::Server => {
                let items = ServerBan::active_with_usernames(&client, limit).await?;
                Ok(single_section(
                    "active server bans",
                    "no active server bans",
                    items.iter().map(format_server_ban_item).collect(),
                ))
            }
            BanListScope::Artboard => {
                let items = ArtboardBan::active_with_usernames(&client, limit).await?;
                Ok(single_section(
                    "active artboard bans",
                    "no active artboard bans",
                    items.iter().map(format_artboard_ban_item).collect(),
                ))
            }
            BanListScope::Room { slug } => {
                let room = find_room_by_mod_slug(&client, &slug).await?;
                let room_slug = room.slug.clone().unwrap_or_else(|| room.kind.clone());
                let items =
                    RoomBan::active_for_room_with_usernames(&client, room.id, limit).await?;
                Ok(single_section(
                    &format!("active room bans for #{room_slug}"),
                    &format!("no active room bans for #{room_slug}"),
                    items.iter().map(format_room_ban_item).collect(),
                ))
            }
        }
    }

    async fn list_audit(&self, permissions: Permissions, limit: i64) -> Result<Vec<String>> {
        ensure_has(permissions, Caps::VIEW_STAFF_INFO)?;
        let client = self.db.get().await?;
        let items = ModerationAuditLog::recent_with_usernames(&client, limit).await?;
        if items.is_empty() {
            return Ok(vec!["no audit log entries".to_string()]);
        }
        let mut lines = vec![format!("recent audit log entries (limit {limit})")];
        lines.extend(items.iter().map(format_audit_log_item));
        Ok(lines)
    }

    async fn rename_room(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        slug: &str,
        new_slug: &str,
    ) -> Result<Vec<String>> {
        ensure_has(permissions, Caps::RENAME_ROOM)?;
        let old_slug = normalize_mod_slug(slug)?;
        let new_slug = normalize_mod_slug(new_slug)?;
        if old_slug == "general" {
            anyhow::bail!("cannot rename #general");
        }
        if new_slug == "general" {
            anyhow::bail!("cannot rename room to reserved #general");
        }

        let mut client = self.db.get().await?;
        let room = find_room_by_mod_slug(&client, &old_slug).await?;
        let current_slug = room.slug.clone().unwrap_or_else(|| room.kind.clone());
        if current_slug == new_slug {
            return Ok(vec![format!("room already named #{new_slug}")]);
        }

        let tx = client.transaction().await?;
        let updated = ChatRoom::rename_non_dm_slug(&tx, room.id, &new_slug).await?;
        if updated == 0 {
            anyhow::bail!("room not found: #{old_slug}");
        }
        if room.kind == "game" {
            GameRoom::rename_by_chat_room_id(&tx, room.id, &new_slug).await?;
        }
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(false),
            actor_user_id,
            "rename_room",
            "room",
            Some(room.id),
            json!({ "old_slug": current_slug, "new_slug": new_slug }),
        )
        .await?;
        tx.commit().await?;
        let _ = self.event_tx.send(ModerationEvent::RoomRenamed {
            actor_user_id,
            room_id: room.id,
            old_slug: current_slug.clone(),
            new_slug: new_slug.clone(),
        });
        Ok(vec![format!("renamed #{current_slug} to #{new_slug}")])
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

fn single_section(title: &str, empty: &str, items: Vec<String>) -> Vec<String> {
    if items.is_empty() {
        vec![empty.to_string()]
    } else {
        let mut lines = vec![format!("{title}:")];
        lines.extend(items);
        lines
    }
}

fn append_section(lines: &mut Vec<String>, title: &str, items: Vec<String>) {
    lines.push(format!("{title}:"));
    if items.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(items);
    }
}

fn format_server_ban_item(item: &ServerBanListItem) -> String {
    let target = item
        .target_username
        .as_deref()
        .or(item.ban.snapshot_username.as_deref())
        .map(user_label)
        .unwrap_or_else(|| item.ban.target_user_id.to_string());
    let actor = item
        .actor_username
        .as_deref()
        .map(user_label)
        .unwrap_or_else(|| item.ban.actor_user_id.to_string());
    let ip = item
        .ban
        .ip_address
        .as_deref()
        .map(|ip| format!(" ip: {ip}"))
        .unwrap_or_default();
    format!(
        "- {target} by {actor} expires: {}{} reason: {}",
        format_expires_at(item.ban.expires_at),
        ip,
        format_reason(&item.ban.reason)
    )
}

fn format_artboard_ban_item(item: &ArtboardBanListItem) -> String {
    let target = item
        .target_username
        .as_deref()
        .map(user_label)
        .unwrap_or_else(|| item.ban.target_user_id.to_string());
    let actor = item
        .actor_username
        .as_deref()
        .map(user_label)
        .unwrap_or_else(|| item.ban.actor_user_id.to_string());
    format!(
        "- {target} by {actor} expires: {} reason: {}",
        format_expires_at(item.ban.expires_at),
        format_reason(&item.ban.reason)
    )
}

fn format_room_ban_item(item: &RoomBanListItem) -> String {
    let room = item
        .room_slug
        .as_deref()
        .map(|slug| format!("#{slug}"))
        .unwrap_or_else(|| item.ban.room_id.to_string());
    let target = item
        .target_username
        .as_deref()
        .map(user_label)
        .unwrap_or_else(|| item.ban.target_user_id.to_string());
    let actor = item
        .actor_username
        .as_deref()
        .map(user_label)
        .unwrap_or_else(|| item.ban.actor_user_id.to_string());
    format!(
        "- {room} {target} by {actor} expires: {} reason: {}",
        format_expires_at(item.ban.expires_at),
        format_reason(&item.ban.reason)
    )
}

fn format_audit_log_item(item: &ModerationAuditLogListItem) -> String {
    let actor = item
        .actor_username
        .as_deref()
        .map(user_label)
        .unwrap_or_else(|| item.log.actor_user_id.to_string());
    let target = if item.log.target_kind == "user" {
        item.target_username
            .as_deref()
            .map(user_label)
            .or_else(|| item.log.target_id.map(|id| id.to_string()))
            .unwrap_or_else(|| "none".to_string())
    } else {
        item.log
            .target_id
            .map(|id| format!("{}:{id}", item.log.target_kind))
            .unwrap_or_else(|| item.log.target_kind.clone())
    };
    let metadata = if item
        .log
        .metadata
        .as_object()
        .is_some_and(|map| map.is_empty())
    {
        String::new()
    } else {
        format!(" metadata: {}", item.log.metadata)
    };
    format!(
        "- {} {actor} {} target: {target}{metadata}",
        item.log.created.format("%Y-%m-%d %H:%M UTC"),
        item.log.action
    )
}

fn format_expires_at(expires_at: Option<chrono::DateTime<Utc>>) -> String {
    expires_at
        .map(|expires_at| expires_at.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "permanent".to_string())
}

fn format_reason(reason: &str) -> &str {
    if reason.trim().is_empty() {
        "-"
    } else {
        reason.trim()
    }
}

fn user_label(username: &str) -> String {
    format!("@{username}")
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
