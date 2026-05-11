use axum::{
    Json, Router,
    extract::{Path, Query, State as AxumState},
    http::StatusCode,
    routing::{delete, get, post},
};
use late_core::models::{rss_entry::RssEntry, rss_feed::RssFeed};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/rss/feeds", get(get_feeds))
        .route("/api/native/rss/feeds", post(post_feed))
        .route("/api/native/rss/feeds/{id}", delete(delete_feed))
        .route("/api/native/rss/entries", get(get_entries))
        .route("/api/native/rss/entries/{id}/dismiss", post(post_dismiss_entry))
        .route("/api/native/rss/unread", get(get_unread_count))
}

// ── Feeds ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct FeedItem {
    id: String,
    url: String,
    title: String,
    active: bool,
    last_checked_at: Option<String>,
    last_success_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Deserialize)]
struct AddFeedBody {
    url: String,
}

async fn get_feeds(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<FeedItem>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let feeds = RssFeed::list_for_user(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(feeds.into_iter().map(feed_to_item).collect()))
}

async fn post_feed(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<AddFeedBody>,
) -> Result<(StatusCode, Json<FeedItem>), ApiError> {
    let url = body.url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ApiError::BadRequest("url must start with http:// or https://"));
    }
    if url.len() > 1000 {
        return Err(ApiError::BadRequest("url exceeds 1000 characters"));
    }

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let feed = RssFeed::create_for_user(&client, auth.user_id, url)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok((StatusCode::CREATED, Json(feed_to_item(feed))))
}

async fn delete_feed(
    auth: NativeAuthUser,
    Path(id): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let feed_id = Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("invalid id"))?;
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let deleted = RssFeed::delete_for_user(&client, auth.user_id, feed_id)
        .await
        .map_err(|_| ApiError::Db)?;
    if deleted == 0 {
        return Err(ApiError::NotFound("feed not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ── Entries ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct EntriesParams {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct EntryItem {
    id: String,
    feed_id: String,
    feed_title: String,
    feed_url: String,
    url: String,
    title: String,
    summary: String,
    published_at: Option<String>,
}

#[derive(Serialize)]
struct UnreadCountResponse {
    unread: i64,
}

async fn get_entries(
    auth: NativeAuthUser,
    Query(params): Query<EntriesParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<EntryItem>>, ApiError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let views = RssEntry::list_visible_for_user(&client, auth.user_id, limit)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(
        views
            .into_iter()
            .map(|v| EntryItem {
                id: v.entry.id.to_string(),
                feed_id: v.entry.feed_id.to_string(),
                feed_title: v.feed_title,
                feed_url: v.feed_url,
                url: v.entry.url,
                title: v.entry.title,
                summary: v.entry.summary,
                published_at: v.entry.published_at.map(|d| d.to_rfc3339()),
            })
            .collect(),
    ))
}

async fn post_dismiss_entry(
    auth: NativeAuthUser,
    Path(id): Path<String>,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let entry_id = Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("invalid id"))?;
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    RssEntry::dismiss(&client, auth.user_id, entry_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_unread_count(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<UnreadCountResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let unread = RssEntry::unread_count_for_user(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(UnreadCountResponse { unread }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn feed_to_item(f: RssFeed) -> FeedItem {
    FeedItem {
        id: f.id.to_string(),
        url: f.url,
        title: f.title,
        active: f.active,
        last_checked_at: f.last_checked_at.map(|d| d.to_rfc3339()),
        last_success_at: f.last_success_at.map(|d| d.to_rfc3339()),
        last_error: f.last_error,
    }
}
