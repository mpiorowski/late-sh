use axum::{
    Json, Router,
    extract::{Path, Query, State as AxumState},
    http::StatusCode,
    routing::{get, post},
};
use late_core::models::showcase::{Showcase, ShowcaseParams};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/showcase", get(get_showcase))
        .route("/api/native/showcase", post(post_showcase))
        .route("/api/native/showcase/mine", get(get_my_showcase))
        .route("/api/native/showcase/{id}", axum::routing::delete(delete_showcase))
}

#[derive(Deserialize)]
struct ShowcaseParams_ {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct ShowcaseItem {
    id: String,
    user_id: String,
    title: String,
    url: String,
    description: String,
    tags: Vec<String>,
    created: String,
}

#[derive(Deserialize)]
struct CreateShowcaseBody {
    title: String,
    url: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
}

async fn get_showcase(
    _auth: NativeAuthUser,
    Query(params): Query<ShowcaseParams_>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<ShowcaseItem>>, ApiError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let items = Showcase::list_recent(&client, limit)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok(Json(
        items
            .into_iter()
            .map(|s| ShowcaseItem {
                id: s.id.to_string(),
                user_id: s.user_id.to_string(),
                title: s.title,
                url: s.url,
                description: s.description,
                tags: s.tags,
                created: s.created.to_rfc3339(),
            })
            .collect(),
    ))
}

async fn post_showcase(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<CreateShowcaseBody>,
) -> Result<(StatusCode, Json<ShowcaseItem>), ApiError> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("title is required"));
    }
    if title.len() > 100 {
        return Err(ApiError::BadRequest("title exceeds 100 characters"));
    }

    let url = body.url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ApiError::BadRequest("url must start with http:// or https://"));
    }
    if url.len() > 500 {
        return Err(ApiError::BadRequest("url exceeds 500 characters"));
    }

    let description = body.description.trim();
    if description.len() > 500 {
        return Err(ApiError::BadRequest("description exceeds 500 characters"));
    }

    let tags: Vec<String> = body
        .tags
        .iter()
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty() && t.len() <= 30)
        .take(10)
        .collect();

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let item = Showcase::create_by_user_id(
        &client,
        auth.user_id,
        ShowcaseParams {
            user_id: Uuid::nil(),
            title: title.to_string(),
            url: url.to_string(),
            description: description.to_string(),
            tags,
        },
    )
    .await
    .map_err(|_| ApiError::Db)?;

    Ok((
        StatusCode::CREATED,
        Json(ShowcaseItem {
            id: item.id.to_string(),
            user_id: item.user_id.to_string(),
            title: item.title,
            url: item.url,
            description: item.description,
            tags: item.tags,
            created: item.created.to_rfc3339(),
        }),
    ))
}

async fn get_my_showcase(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<ShowcaseItem>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let items = Showcase::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(
        items
            .into_iter()
            .map(|s| ShowcaseItem {
                id: s.id.to_string(),
                user_id: s.user_id.to_string(),
                title: s.title,
                url: s.url,
                description: s.description,
                tags: s.tags,
                created: s.created.to_rfc3339(),
            })
            .collect(),
    ))
}

async fn delete_showcase(
    auth: NativeAuthUser,
    Path(id): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let item_id = Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("invalid id"))?;
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let deleted = Showcase::delete_by_user_id(&client, auth.user_id, item_id)
        .await
        .map_err(|_| ApiError::Db)?;
    if deleted == 0 {
        return Err(ApiError::NotFound("showcase item not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
