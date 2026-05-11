use axum::{
    Json, Router,
    extract::{Path, Query, State as AxumState},
    http::StatusCode,
    routing::{get, post},
};
use late_core::models::{
    chat_message::ChatMessage,
    chat_message_reaction::ChatMessageReaction,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    user::User,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ApiError, NativeAuthUser};
use crate::state::State;

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/rooms", get(get_rooms))
        .route("/api/native/rooms/{room}", get(get_room))
        .route("/api/native/rooms/{room}/history", get(get_room_history))
        .route("/api/native/rooms/{room}/messages", post(post_room_message))
        .route("/api/native/rooms/{room}/messages/{id}/react", post(post_message_react))
        .route("/api/native/rooms/{room}/members", get(get_room_members))
        .route("/api/native/rooms/{room}/read", post(post_room_read))
        .route("/api/native/dms", post(post_create_dm))
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub kind: String,
    pub unread_count: i64,
}

#[derive(Serialize)]
pub struct MessageItem {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub body: String,
    pub timestamp: String,
    pub reactions: Vec<ReactionItem>,
}

#[derive(Serialize)]
pub struct ReactionItem {
    pub emoji: String,
    pub count: i64,
}

#[derive(Serialize)]
struct MemberItem {
    user_id: String,
    username: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn get_rooms(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<RoomInfo>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let rooms = ChatRoom::list_for_user(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    let unread_map = ChatRoomMember::unread_counts_for_user(&client, auth.user_id)
        .await
        .unwrap_or_default();

    let items = rooms
        .into_iter()
        .map(|r| RoomInfo {
            id: r.id.to_string(),
            name: room_display_name(r.slug.as_deref()),
            slug: r.slug.clone().unwrap_or_default(),
            kind: r.kind.clone(),
            unread_count: unread_map.get(&r.id).copied().unwrap_or(0),
        })
        .collect();

    Ok(Json(items))
}

#[derive(Deserialize)]
struct HistoryParams {
    limit: Option<i64>,
}

async fn get_room_history(
    auth: NativeAuthUser,
    Path(room): Path<String>,
    Query(params): Query<HistoryParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<MessageItem>>, ApiError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let room_id = resolve_room_id(&client, &room).await?;

    let is_member = ChatRoomMember::is_member(&client, room_id, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    if !is_member {
        return Err(ApiError::Forbidden("not a member of this room"));
    }

    let messages = ChatMessage::list_recent(&client, room_id, limit)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok(Json(build_message_items(&client, messages).await))
}

#[derive(Deserialize)]
struct SendMessageBody {
    body: String,
    reply_to: Option<String>,
}

async fn post_room_message(
    auth: NativeAuthUser,
    Path(room): Path<String>,
    AxumState(state): AxumState<State>,
    Json(body): Json<SendMessageBody>,
) -> Result<StatusCode, ApiError> {
    let trimmed = body.body.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("message body is empty"));
    }
    if trimmed.len() > 4000 {
        return Err(ApiError::BadRequest("message body exceeds 4000 characters"));
    }
    let reply_to = body
        .reply_to
        .as_deref()
        .map(|s| Uuid::parse_str(s).map_err(|_| ApiError::BadRequest("invalid reply_to uuid")))
        .transpose()?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let room_id = resolve_room_id(&client, &room).await?;

    // Need room slug for announcements guard in chat service
    let chat_room = ChatRoom::get(&client, room_id)
        .await
        .map_err(|_| ApiError::Db)?
        .ok_or(ApiError::NotFound("room not found"))?;
    let room_slug = chat_room.slug.clone();
    drop(client);

    state.chat_service.send_message_with_reply_task(
        crate::app::chat::svc::SendMessageTask {
            user_id: auth.user_id,
            room_id,
            room_slug,
            body: trimmed.to_string(),
            reply_to_message_id: reply_to,
            request_id: Uuid::now_v7(),
            is_admin: false,
        },
    );

    Ok(StatusCode::ACCEPTED)
}

async fn get_room_members(
    auth: NativeAuthUser,
    Path(room): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<MemberItem>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let room_id = resolve_room_id(&client, &room).await?;

    let is_member = ChatRoomMember::is_member(&client, room_id, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    if !is_member {
        return Err(ApiError::Forbidden("not a member of this room"));
    }

    let user_ids = ChatRoomMember::list_user_ids(&client, room_id)
        .await
        .map_err(|_| ApiError::Db)?;
    let usernames = User::list_usernames_by_ids(&client, &user_ids)
        .await
        .unwrap_or_default();

    let items = user_ids
        .iter()
        .map(|id| MemberItem {
            user_id: id.to_string(),
            username: usernames.get(id).cloned().unwrap_or_default(),
        })
        .collect();

    Ok(Json(items))
}

async fn get_room(
    auth: NativeAuthUser,
    Path(room): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<Json<RoomInfo>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let room_id = resolve_room_id(&client, &room).await?;

    let chat_room = ChatRoom::get(&client, room_id)
        .await
        .map_err(|_| ApiError::Db)?
        .ok_or(ApiError::NotFound("room not found"))?;

    let is_member = ChatRoomMember::is_member(&client, room_id, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    if !is_member {
        return Err(ApiError::Forbidden("not a member of this room"));
    }

    let unread = ChatRoomMember::unread_counts_for_user(&client, auth.user_id)
        .await
        .unwrap_or_default()
        .get(&room_id)
        .copied()
        .unwrap_or(0);

    Ok(Json(RoomInfo {
        id: chat_room.id.to_string(),
        name: room_display_name(chat_room.slug.as_deref()),
        slug: chat_room.slug.clone().unwrap_or_default(),
        kind: chat_room.kind.clone(),
        unread_count: unread,
    }))
}

#[derive(Deserialize)]
struct CreateDmBody {
    username: String,
}

#[derive(Serialize)]
struct DmResponse {
    room_id: String,
    slug: String,
}

async fn post_create_dm(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<CreateDmBody>,
) -> Result<Json<DmResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let target = User::find_by_username(&client, body.username.trim())
        .await
        .map_err(|_| ApiError::Db)?
        .ok_or(ApiError::NotFound("user not found"))?;

    if target.id == auth.user_id {
        return Err(ApiError::BadRequest("cannot DM yourself"));
    }

    let room = ChatRoom::get_or_create_dm(&client, auth.user_id, target.id)
        .await
        .map_err(|_| ApiError::Db)?;

    // Ensure both parties are room members
    ChatRoomMember::join(&client, room.id, auth.user_id).await.ok();
    ChatRoomMember::join(&client, room.id, target.id).await.ok();

    Ok(Json(DmResponse {
        room_id: room.id.to_string(),
        slug: room.slug.clone().unwrap_or_default(),
    }))
}

#[derive(Deserialize)]
struct ReactBody {
    kind: i16,
}

async fn post_message_react(
    auth: NativeAuthUser,
    Path((room, message_id)): Path<(String, String)>,
    AxumState(state): AxumState<State>,
    Json(body): Json<ReactBody>,
) -> Result<StatusCode, ApiError> {
    if !(1..=8).contains(&body.kind) {
        return Err(ApiError::BadRequest("reaction kind must be 1–8"));
    }
    let msg_id = Uuid::parse_str(&message_id)
        .map_err(|_| ApiError::BadRequest("invalid message id"))?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let room_id = resolve_room_id(&client, &room).await?;

    let is_member = ChatRoomMember::is_member(&client, room_id, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    if !is_member {
        return Err(ApiError::Forbidden("not a member of this room"));
    }

    ChatMessageReaction::toggle(&client, msg_id, auth.user_id, body.kind)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn post_room_read(
    auth: NativeAuthUser,
    Path(room): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let room_id = resolve_room_id(&client, &room).await?;

    ChatRoomMember::mark_read_now(&client, room_id, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve a room path segment to a room UUID.
/// Accepts: UUID, "general", or any room slug.
pub(crate) async fn resolve_room_id(
    client: &deadpool_postgres::Client,
    room: &str,
) -> Result<Uuid, ApiError> {
    if let Ok(id) = Uuid::parse_str(room) {
        return Ok(id);
    }
    // Named lookup: "general" is a common shorthand; all other slugs go via DB.
    ChatRoom::find_non_dm_by_slug(client, room)
        .await
        .map_err(|_| ApiError::Db)?
        .map(|r| r.id)
        .ok_or(ApiError::NotFound("room not found"))
}

pub(crate) async fn build_message_items(
    client: &deadpool_postgres::Client,
    messages: Vec<ChatMessage>,
) -> Vec<MessageItem> {
    let author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
    let usernames = User::list_usernames_by_ids(client, &author_ids)
        .await
        .unwrap_or_default();

    let message_ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
    let reactions_map = ChatMessageReaction::list_summaries_for_messages(client, &message_ids)
        .await
        .unwrap_or_default();

    messages
        .iter()
        .rev()
        .map(|m| MessageItem {
            id: m.id.to_string(),
            user_id: m.user_id.to_string(),
            username: usernames.get(&m.user_id).cloned().unwrap_or_default(),
            body: m.body.clone(),
            timestamp: m.created.to_rfc3339(),
            reactions: reactions_map
                .get(&m.id)
                .map(|rs| {
                    rs.iter()
                        .map(|r| ReactionItem {
                            emoji: reaction_emoji(r.kind).to_string(),
                            count: r.count,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

pub(crate) fn reaction_emoji(kind: i16) -> &'static str {
    match kind {
        1 => "👍",
        2 => "🧡",
        3 => "😂",
        4 => "👀",
        5 => "🔥",
        6 => "🙌",
        7 => "🚀",
        8 => "🤔",
        _ => "?",
    }
}

fn room_display_name(slug: Option<&str>) -> String {
    match slug {
        None | Some("") => "Room".to_string(),
        Some(s) => {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}
