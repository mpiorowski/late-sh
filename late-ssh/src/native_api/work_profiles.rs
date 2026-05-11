use axum::{
    Json, Router,
    extract::{Path, Query, State as AxumState},
    http::StatusCode,
    routing::{delete, get, put},
};
use late_core::models::work_profile::{WorkProfile, WorkProfileParams};
use serde::{Deserialize, Serialize};
use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/work-profiles", get(get_work_profiles))
        .route("/api/native/work-profiles/{slug}", get(get_work_profile_by_slug))
        .route("/api/native/work-profile", put(put_work_profile))
        .route("/api/native/work-profile", delete(delete_work_profile))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct WorkProfileItem {
    id: String,
    user_id: String,
    slug: String,
    headline: String,
    status: String,
    work_type: String,
    location: String,
    contact: String,
    links: Vec<String>,
    skills: Vec<String>,
    summary: String,
}

#[derive(Deserialize)]
struct UpsertWorkProfileBody {
    headline: String,
    status: String,
    work_type: String,
    location: String,
    contact: String,
    #[serde(default)]
    links: Vec<String>,
    #[serde(default)]
    skills: Vec<String>,
    summary: String,
}

async fn get_work_profiles(
    _auth: NativeAuthUser,
    Query(params): Query<ListParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<WorkProfileItem>>, ApiError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let profiles = WorkProfile::list_recent(&client, limit).await.map_err(|_| ApiError::Db)?;
    Ok(Json(profiles.into_iter().map(to_item).collect()))
}

async fn get_work_profile_by_slug(
    _auth: NativeAuthUser,
    Path(slug): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<Json<WorkProfileItem>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let profile = WorkProfile::find_by_slug(&client, &slug)
        .await
        .map_err(|_| ApiError::Db)?
        .ok_or(ApiError::NotFound("work profile not found"))?;
    Ok(Json(to_item(profile)))
}

async fn put_work_profile(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<UpsertWorkProfileBody>,
) -> Result<Json<WorkProfileItem>, ApiError> {
    if body.headline.trim().is_empty() {
        return Err(ApiError::BadRequest("headline is required"));
    }
    if body.headline.len() > 120 {
        return Err(ApiError::BadRequest("headline exceeds 120 characters"));
    }
    if body.summary.len() > 2000 {
        return Err(ApiError::BadRequest("summary exceeds 2000 characters"));
    }
    if body.links.len() > 10 {
        return Err(ApiError::BadRequest("too many links (max 10)"));
    }
    if body.skills.len() > 20 {
        return Err(ApiError::BadRequest("too many skills (max 20)"));
    }
    for link in &body.links {
        if !link.starts_with("http://") && !link.starts_with("https://") {
            return Err(ApiError::BadRequest("each link must start with http:// or https://"));
        }
    }

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;

    // Derive slug from the user's username
    let slug = client
        .query_opt("SELECT username FROM users WHERE id = $1", &[&auth.user_id])
        .await
        .map_err(|_| ApiError::Db)?
        .and_then(|row| {
            let u: String = row.get("username");
            if u.is_empty() { None } else { Some(u) }
        })
        .ok_or(ApiError::NotFound("user not found"))?;

    // Check for existing profile and update, or create
    let existing = WorkProfile::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;

    let params = WorkProfileParams {
        user_id: auth.user_id,
        slug: slug.clone(),
        headline: body.headline.trim().to_string(),
        status: body.status.trim().to_string(),
        work_type: body.work_type.trim().to_string(),
        location: body.location.trim().to_string(),
        contact: body.contact.trim().to_string(),
        links: body.links,
        skills: body.skills,
        summary: body.summary.trim().to_string(),
    };

    let profile = if let Some(existing) = existing.into_iter().next() {
        WorkProfile::update_by_user_id(&client, auth.user_id, existing.id, params)
            .await
            .map_err(|_| ApiError::Db)?
            .ok_or(ApiError::NotFound("work profile not found"))?
    } else {
        WorkProfile::create_by_user_id(&client, auth.user_id, params)
            .await
            .map_err(|_| ApiError::Db)?
    };

    Ok(Json(to_item(profile)))
}

async fn delete_work_profile(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let existing = WorkProfile::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    let Some(profile) = existing.into_iter().next() else {
        return Err(ApiError::NotFound("no work profile found"));
    };
    WorkProfile::delete_by_user_id(&client, auth.user_id, profile.id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(StatusCode::NO_CONTENT)
}

fn to_item(p: WorkProfile) -> WorkProfileItem {
    WorkProfileItem {
        id: p.id.to_string(),
        user_id: p.user_id.to_string(),
        slug: p.slug,
        headline: p.headline,
        status: p.status,
        work_type: p.work_type,
        location: p.location,
        contact: p.contact,
        links: p.links,
        skills: p.skills,
        summary: p.summary,
    }
}
