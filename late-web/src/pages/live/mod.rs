//! "Watch me" stream pages: the public watch page (`/live/{id}`, the
//! `/listen` sibling) and the streamer's go-live console (`/golive/{token}`).
//!
//! Both are thin LiveKit browser clients. late-web only serves the pages and
//! proxies their capability-id lookups to late-ssh (which owns stream
//! registry state and LiveKit token minting); media flows browser -> LiveKit
//! -> browser and never touches these servers. The ids in the URLs are
//! random per-stream capabilities minted in the TUI: no login, no cookies,
//! dead as soon as the stream ends.

use anyhow::Context;
use askama::Template;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use late_core::telemetry::TracedExt;

use crate::{AppState, error::AppError, metrics};

#[cfg(test)]
mod live_test;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/live/{id}", get(watch_page_handler))
        .route("/live/{id}/state", get(watch_state_handler))
        .route("/live/{id}/grant", get(watch_grant_handler))
        .route("/live/{id}/heartbeat", post(watch_heartbeat_handler))
        .route("/golive/{token}", get(golive_page_handler))
        .route("/golive/{token}/grant", get(golive_grant_handler))
        .route("/golive/{token}/state", post(golive_state_handler))
}

/// Capability ids are hex/dash tokens minted by late-ssh. Anything else is
/// rejected before it can be interpolated into an internal API path.
fn valid_capability_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[derive(Template)]
#[template(path = "pages/live/watch.html")]
struct WatchPage<'a> {
    stream_id: &'a str,
}

#[derive(Template)]
#[template(path = "pages/live/golive.html")]
struct GoLivePage<'a> {
    publish_token: &'a str,
}

async fn watch_page_handler(Path(id): Path<String>) -> Result<Response, AppError> {
    if !valid_capability_id(&id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    metrics::record_page_view("live", false);
    Ok(Html(WatchPage { stream_id: &id }.render()?).into_response())
}

async fn golive_page_handler(Path(token): Path<String>) -> Result<Response, AppError> {
    if !valid_capability_id(&token) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    metrics::record_page_view("golive", false);
    Ok(Html(
        GoLivePage {
            publish_token: &token,
        }
        .render()?,
    )
    .into_response())
}

async fn watch_state_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    proxy_get(&state, &id, &format!("/api/stream/watch/{id}")).await
}

async fn watch_grant_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    proxy_get(&state, &id, &format!("/api/stream/watch/{id}/grant")).await
}

async fn watch_heartbeat_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    proxy_post(
        &state,
        &id,
        &format!("/api/stream/watch/{id}/heartbeat"),
        body,
    )
    .await
}

async fn golive_grant_handler(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    proxy_get(&state, &token, &format!("/api/stream/publish/{token}")).await
}

async fn golive_state_handler(
    Path(token): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    proxy_post(
        &state,
        &token,
        &format!("/api/stream/publish/{token}/state"),
        body,
    )
    .await
}

/// Same-origin GET proxy to late-ssh, forwarding the status code: a 404 is
/// the page's "stream is gone" signal, so it must survive the hop.
async fn proxy_get(state: &AppState, id: &str, path: &str) -> Result<Response, AppError> {
    if !valid_capability_id(id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let url = format!("{}{path}", state.config.ssh_internal_url);
    let response = state
        .http_client
        .get(&url)
        .send_traced()
        .await
        .context("failed to fetch stream state")?;
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if !status.is_success() {
        return Ok(status.into_response());
    }
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse stream state")?;
    Ok((status, Json(body)).into_response())
}

async fn proxy_post(
    state: &AppState,
    id: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<Response, AppError> {
    if !valid_capability_id(id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let url = format!("{}{path}", state.config.ssh_internal_url);
    let response = state
        .http_client
        .post(&url)
        .json(&body)
        .send_traced()
        .await
        .context("failed to report stream state")?;
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    Ok(status.into_response())
}
