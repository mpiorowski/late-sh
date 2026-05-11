use axum::{
    Json, Router,
    extract::State as AxumState,
    routing::{get, put},
};
use late_core::models::{
    profile::{Profile, ProfileParams},
    user::User,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/profile", get(get_profile))
        .route("/api/native/profile", put(put_profile))
}

#[derive(Serialize)]
struct ProfileResponse {
    username: String,
    bio: String,
    country: Option<String>,
    timezone: Option<String>,
    ide: Option<String>,
    terminal: Option<String>,
    os: Option<String>,
    langs: Vec<String>,
    notify_kinds: Vec<String>,
    notify_bell: bool,
    notify_cooldown_mins: i32,
    notify_format: Option<String>,
    theme_id: Option<String>,
    enable_background_color: bool,
    show_dashboard_header: bool,
    show_right_sidebar: bool,
    show_games_sidebar: bool,
    show_settings_on_connect: bool,
    favorite_room_ids: Vec<String>,
    member_since: Option<String>,
}

async fn get_profile(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<ProfileResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let profile = Profile::load(&client, auth.user_id).await.map_err(|_| ApiError::Db)?;
    Ok(Json(profile_to_response(profile)))
}

#[derive(Deserialize)]
struct UpdateProfileBody {
    username: Option<String>,
    bio: Option<String>,
    country: Option<String>,
    timezone: Option<String>,
    ide: Option<String>,
    terminal: Option<String>,
    os: Option<String>,
    langs: Option<Vec<String>>,
    notify_kinds: Option<Vec<String>>,
    notify_bell: Option<bool>,
    notify_cooldown_mins: Option<i32>,
    notify_format: Option<String>,
    theme_id: Option<String>,
    enable_background_color: Option<bool>,
    show_dashboard_header: Option<bool>,
    show_right_sidebar: Option<bool>,
    show_games_sidebar: Option<bool>,
    show_settings_on_connect: Option<bool>,
    favorite_room_ids: Option<Vec<String>>,
}

async fn put_profile(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<UpdateProfileBody>,
) -> Result<Json<ProfileResponse>, ApiError> {
    // Validate
    if let Some(ref u) = body.username {
        let trimmed = u.trim();
        if trimmed.is_empty() || trimmed.len() > 32 {
            return Err(ApiError::BadRequest("username must be 1–32 characters"));
        }
    }
    if let Some(ref b) = body.bio {
        if b.len() > 500 {
            return Err(ApiError::BadRequest("bio exceeds 500 characters"));
        }
    }
    if let Some(ref fmt) = body.notify_format {
        if !matches!(fmt.as_str(), "both" | "osc777" | "osc9") {
            return Err(ApiError::BadRequest(
                "notify_format must be one of: both, osc777, osc9",
            ));
        }
    }
    if let Some(ref ids) = body.favorite_room_ids {
        if ids.len() > 20 {
            return Err(ApiError::BadRequest("too many favorite rooms (max 20)"));
        }
        for id in ids {
            Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("invalid uuid in favorite_room_ids"))?;
        }
    }

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;

    // Load current profile to merge partial updates
    let current = Profile::load(&client, auth.user_id).await.map_err(|_| ApiError::Db)?;

    let favorite_room_ids = body
        .favorite_room_ids
        .as_deref()
        .map(|ids| {
            ids.iter()
                .map(|id| Uuid::parse_str(id).unwrap()) // already validated above
                .collect::<Vec<_>>()
        })
        .unwrap_or(current.favorite_room_ids);

    // Validate username uniqueness if changing
    let new_username = body
        .username
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or(&current.username)
        .to_string();

    if new_username != current.username {
        let existing = User::find_by_username(&client, &new_username)
            .await
            .map_err(|_| ApiError::Db)?;
        if existing.is_some() {
            return Err(ApiError::BadRequest("username already taken"));
        }
    }

    let params = ProfileParams {
        username: new_username,
        bio: body.bio.unwrap_or(current.bio),
        country: body.country.or(current.country),
        timezone: body.timezone.or(current.timezone),
        ide: body.ide.or(current.ide),
        terminal: body.terminal.or(current.terminal),
        os: body.os.or(current.os),
        langs: body.langs.unwrap_or(current.langs),
        notify_kinds: body.notify_kinds.unwrap_or(current.notify_kinds),
        notify_bell: body.notify_bell.unwrap_or(current.notify_bell),
        notify_cooldown_mins: body.notify_cooldown_mins.unwrap_or(current.notify_cooldown_mins),
        notify_format: body.notify_format.or(current.notify_format),
        theme_id: body.theme_id.or(current.theme_id),
        enable_background_color: body
            .enable_background_color
            .unwrap_or(current.enable_background_color),
        show_dashboard_header: body.show_dashboard_header.unwrap_or(current.show_dashboard_header),
        show_right_sidebar: body.show_right_sidebar.unwrap_or(current.show_right_sidebar),
        show_games_sidebar: body.show_games_sidebar.unwrap_or(current.show_games_sidebar),
        show_settings_on_connect: body
            .show_settings_on_connect
            .unwrap_or(current.show_settings_on_connect),
        favorite_room_ids,
    };

    let updated = Profile::update(&client, auth.user_id, params)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok(Json(profile_to_response(updated)))
}

fn profile_to_response(p: Profile) -> ProfileResponse {
    ProfileResponse {
        username: p.username,
        bio: p.bio,
        country: p.country,
        timezone: p.timezone,
        ide: p.ide,
        terminal: p.terminal,
        os: p.os,
        langs: p.langs,
        notify_kinds: p.notify_kinds,
        notify_bell: p.notify_bell,
        notify_cooldown_mins: p.notify_cooldown_mins,
        notify_format: p.notify_format,
        theme_id: p.theme_id,
        enable_background_color: p.enable_background_color,
        show_dashboard_header: p.show_dashboard_header,
        show_right_sidebar: p.show_right_sidebar,
        show_games_sidebar: p.show_games_sidebar,
        show_settings_on_connect: p.show_settings_on_connect,
        favorite_room_ids: p.favorite_room_ids.iter().map(Uuid::to_string).collect(),
        member_since: p.created_at.map(|d| d.to_rfc3339()),
    }
}
