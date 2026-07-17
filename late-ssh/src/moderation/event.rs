use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::authz::Permissions;
use crate::moderation::command::{
    ArtboardAction, AudioAction, RoleAction, RoomModAction, ServerUserAction,
};

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
    RoomSlowModeChanged {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        room_id: Uuid,
        room_slug: String,
        interval_secs: Option<i32>,
        expires_at: Option<DateTime<Utc>>,
        reason: String,
        notified_sessions: usize,
    },
    RoomRenamed {
        actor_user_id: Uuid,
        room_id: Uuid,
        old_slug: String,
        new_slug: String,
    },
    UserRenamed {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        old_username: String,
        new_username: String,
        active_user_updated: bool,
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
    ArtboardRestored {
        actor_user_id: Uuid,
        source_key: String,
        backup_key: Option<String>,
        reason: String,
    },
    ArtboardCurated {
        actor_user_id: Uuid,
        board_key: String,
        reason: String,
    },
    AudioAction {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        action: AudioAction,
        banned: bool,
        expires_at: Option<DateTime<Utc>>,
        reason: String,
    },
    RoleAction {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        action: RoleAction,
        permissions: Permissions,
        notified_sessions: usize,
    },
}
