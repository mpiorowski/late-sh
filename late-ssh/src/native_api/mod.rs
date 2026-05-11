pub mod articles;
pub mod artboard;
pub mod auth;
pub mod bonsai;
pub mod chat;
pub mod chips;
pub mod games;
pub mod media;
pub mod notifications;
pub mod profile;
pub mod rss;
pub mod showcase;
pub mod users;
pub mod work_profiles;
pub mod ws;

use axum::{
    Json, Router,
    extract::FromRequestParts,
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::IntoResponse,
};
use late_core::models::native_token::NativeToken;
use serde::Serialize;
use uuid::Uuid;

use crate::state::State;

// ── Typed API error ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    BadRequest(&'static str),
    TooManyRequests,
    Db,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"),
            Self::Db => (StatusCode::INTERNAL_SERVER_ERROR, "db error"),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

// ── Auth extractor ────────────────────────────────────────────────────────────

pub struct NativeAuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub raw_token: String,
}

impl FromRequestParts<State> for NativeAuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or(ApiError::Unauthorized("missing bearer token"))?
            .to_owned();

        let client = state.db.get().await.map_err(|_| ApiError::Db)?;

        let (user_id, username) = NativeToken::find_user_by_token(&client, &token)
            .await
            .map_err(|_| ApiError::Db)?
            .ok_or(ApiError::Unauthorized("invalid or expired token"))?;

        Ok(NativeAuthUser { user_id, username, raw_token: token })
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<State> {
    Router::new()
        .merge(articles::router())
        .merge(artboard::router())
        .merge(auth::router())
        .merge(bonsai::router())
        .merge(chat::router())
        .merge(chips::router())
        .merge(games::router())
        .merge(media::router())
        .merge(notifications::router())
        .merge(profile::router())
        .merge(rss::router())
        .merge(showcase::router())
        .merge(users::router())
        .merge(work_profiles::router())
        .merge(ws::router())
}
