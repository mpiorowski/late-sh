use axum::{
    Json, Router,
    extract::{Query, State as AxumState},
    http::StatusCode,
    routing::{get, post},
};
use late_core::models::notification::Notification;
use serde::{Deserialize, Serialize};

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/notifications", get(get_notifications))
        .route("/api/native/notifications/read", post(post_notifications_read))
        .route("/api/native/notifications/unread", get(get_unread_count))
}

#[derive(Deserialize)]
struct NotifParams {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct NotificationItem {
    id: String,
    actor_username: String,
    room_slug: Option<String>,
    message_preview: String,
    timestamp: String,
}

#[derive(Serialize)]
struct UnreadCountResponse {
    unread: i64,
}

async fn get_notifications(
    auth: NativeAuthUser,
    Query(params): Query<NotifParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<NotificationItem>>, ApiError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let views = Notification::list_for_user(&client, auth.user_id, limit)
        .await
        .map_err(|_| ApiError::Db)?;

    let items = views
        .into_iter()
        .map(|n| NotificationItem {
            id: n.id.to_string(),
            actor_username: n.actor_username,
            room_slug: n.room_slug,
            message_preview: n.message_preview,
            timestamp: n.created.to_rfc3339(),
        })
        .collect();

    Ok(Json(items))
}

async fn post_notifications_read(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    Notification::mark_all_read(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_unread_count(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<UnreadCountResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let unread = Notification::unread_count(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(UnreadCountResponse { unread }))
}
