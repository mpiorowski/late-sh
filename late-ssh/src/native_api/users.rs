use axum::{
    Json, Router,
    extract::{Path, State as AxumState},
    routing::get,
};
use late_core::models::{profile::Profile, user::User};
use serde::Serialize;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/me", get(get_me))
        .route("/api/native/users/online", get(get_online_users))
        .route("/api/native/users/{username}", get(get_user_profile))
}

#[derive(Serialize)]
struct MeResponse {
    user_id: String,
    username: String,
}

async fn get_me(auth: NativeAuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        user_id: auth.user_id.to_string(),
        username: auth.username,
    })
}

#[derive(Serialize)]
struct OnlineUser {
    user_id: String,
    username: String,
}

async fn get_online_users(
    _auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<Vec<OnlineUser>> {
    let users = state.active_users.lock().unwrap_or_else(|e| e.into_inner());
    let list = users
        .iter()
        .map(|(id, u)| OnlineUser {
            user_id: id.to_string(),
            username: u.username.clone(),
        })
        .collect();
    Json(list)
}

#[derive(Serialize)]
struct PublicProfile {
    username: String,
    bio: String,
    country: Option<String>,
    timezone: Option<String>,
    ide: Option<String>,
    terminal: Option<String>,
    os: Option<String>,
    langs: Vec<String>,
    member_since: Option<String>,
}

async fn get_user_profile(
    _auth: NativeAuthUser,
    Path(username): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<Json<PublicProfile>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let user = User::find_by_username(&client, &username)
        .await
        .map_err(|_| ApiError::Db)?
        .ok_or(ApiError::NotFound("user not found"))?;

    let profile = Profile::load(&client, user.id).await.map_err(|_| ApiError::Db)?;

    Ok(Json(PublicProfile {
        username: profile.username,
        bio: profile.bio,
        country: profile.country,
        timezone: profile.timezone,
        ide: profile.ide,
        terminal: profile.terminal,
        os: profile.os,
        langs: profile.langs,
        member_since: profile.created_at.map(|d| d.to_rfc3339()),
    }))
}
