use anyhow::Result;
use deadpool_postgres::GenericClient;
use late_core::models::{moderation_audit_log::ModerationAuditLog, user::User};
use serde_json::Value;
use uuid::Uuid;

use crate::authz::{Caps, Permissions, Tier};

pub(crate) struct ModAuditRecord {
    pub(crate) permissions: Permissions,
    pub(crate) target_is_self: bool,
    pub(crate) audit_action: &'static str,
    pub(crate) target_kind: &'static str,
    pub(crate) target_id: Option<Uuid>,
    pub(crate) metadata: Value,
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

pub(crate) async fn record_mod_audit(
    client: &impl GenericClient,
    actor_user_id: Uuid,
    record: ModAuditRecord,
) -> Result<()> {
    if record.permissions.should_audit(record.target_is_self) {
        ModerationAuditLog::record(
            client,
            actor_user_id,
            record.audit_action,
            record.target_kind,
            record.target_id,
            record.metadata,
        )
        .await?;
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
        anyhow::bail!("Cannot edit or delete this message")
    }
}

pub(crate) const fn cap_for_server_ban(duration: Option<chrono::Duration>) -> Caps {
    if duration.is_some() {
        Caps::TEMP_BAN_USER
    } else {
        Caps::PERMA_BAN_USER
    }
}
