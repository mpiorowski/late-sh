use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::authz::Permissions;
use crate::moderation::command::{ArtboardAction, RoleAction, RoomModAction, ServerUserAction};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModerationEvent {
    RoomAction {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        room_id: Uuid,
        room_slug: String,
        action: RoomModAction,
        reason: String,
        notified_sessions: usize,
    },
    RoomRenamed {
        actor_user_id: Uuid,
        room_id: Uuid,
        old_slug: String,
        new_slug: String,
    },
    ServerUserAction {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        target_username: String,
        action: ServerUserAction,
        reason: String,
        terminated_sessions: usize,
    },
    ArtboardAction {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        action: ArtboardAction,
        banned: bool,
        expires_at: Option<DateTime<Utc>>,
        reason: String,
        notified_sessions: usize,
    },
    RoleAction {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        action: RoleAction,
        permissions: Permissions,
        notified_sessions: usize,
    },
}
