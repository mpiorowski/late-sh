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
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use late_core::telemetry::TracedExt;
use serde::Deserialize;

use crate::{AppState, error::AppError, metrics};

/// Header the late-ssh API uses for the publish-token claim secret. On the
/// browser side the secret lives as an HttpOnly cookie scoped to this
/// console's URL path; the proxy translates between the two so page JS
/// never touches it.
const PUBLISH_CLAIM_HEADER: &str = "x-late-publish-claim";
const PUBLISH_CLAIM_COOKIE: &str = "golive_claim";

fn publish_claim_from_cookies(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == PUBLISH_CLAIM_COOKIE).then(|| value.to_string())
    })
}

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

#[derive(Deserialize)]
struct WatchGrantParams {
    watcher_id: String,
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

/// The page's stable watcher id rides through to late-ssh, which turns it
/// into the viewer's LiveKit identity so retries reuse one participant. It
/// is validated here too, since it is interpolated into the internal query
/// string: same alnum/dash shape as a capability id.
async fn watch_grant_handler(
    Path(id): Path<String>,
    Query(params): Query<WatchGrantParams>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    if !valid_capability_id(&params.watcher_id) {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    proxy_get(
        &state,
        &id,
        &format!(
            "/api/stream/watch/{id}/grant?watcher_id={}",
            params.watcher_id
        ),
    )
    .await
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

/// Publish grant proxy with the claim-once cookie exchange: the browser's
/// claim cookie rides upstream as a header, and the claiming (first) fetch
/// gets the minted secret back as an HttpOnly cookie scoped to this
/// console's path, so a leaked publish URL cannot fetch a grant from any
/// other browser (403 from upstream, forwarded as-is).
async fn golive_grant_handler(
    Path(token): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    if !valid_capability_id(&token) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let url = format!(
        "{}/api/stream/publish/{token}",
        state.config.ssh_internal_url
    );
    let mut request = state.http_client.get(&url);
    if let Some(claim) = publish_claim_from_cookies(&headers) {
        request = request.header(PUBLISH_CLAIM_HEADER, claim);
    }
    let response = match request.send_traced().await {
        Ok(response) => response,
        Err(err) => return Ok(upstream_error_status(err).into_response()),
    };
    let new_claim = response
        .headers()
        .get(PUBLISH_CLAIM_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse publish grant")?;
    let mut page_response = (StatusCode::OK, Json(body)).into_response();
    if let Some(secret) = new_claim {
        // HttpOnly: page JS never sees the secret. No `Secure` attribute so
        // plain-http local dev keeps working; the cookie is path-scoped to
        // one capability URL, so its exposure matches the URL's own.
        let cookie = format!(
            "{PUBLISH_CLAIM_COOKIE}={secret}; Path=/golive/{token}; HttpOnly; SameSite=Strict"
        );
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            page_response
                .headers_mut()
                .insert(header::SET_COOKIE, value);
        }
    }
    Ok(page_response)
}

async fn golive_state_handler(
    Path(token): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    if !valid_capability_id(&token) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let url = format!(
        "{}/api/stream/publish/{token}/state",
        state.config.ssh_internal_url
    );
    let mut request = state.http_client.post(&url).json(&body);
    if let Some(claim) = publish_claim_from_cookies(&headers) {
        request = request.header(PUBLISH_CLAIM_HEADER, claim);
    }
    let response = match request.send_traced().await {
        Ok(response) => response,
        Err(err) => return Ok(upstream_error_status(err).into_response()),
    };
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    Ok(status.into_response())
}

/// Same-origin GET proxy to late-ssh, forwarding the status code: a 404 is
/// the page's "stream is gone" signal, so it must survive the hop.
/// `send_traced` folds non-2xx responses into `Err` (`error_for_status`),
/// so upstream statuses are recovered from the error instead of surfacing
/// as a late-web 500.
async fn proxy_get(state: &AppState, id: &str, path: &str) -> Result<Response, AppError> {
    if !valid_capability_id(id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let url = format!("{}{path}", state.config.ssh_internal_url);
    let response = match state.http_client.get(&url).send_traced().await {
        Ok(response) => response,
        Err(err) => return Ok(upstream_error_status(err).into_response()),
    };
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse stream state")?;
    Ok((StatusCode::OK, Json(body)).into_response())
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
    let response = match state.http_client.post(&url).json(&body).send_traced().await {
        Ok(response) => response,
        Err(err) => return Ok(upstream_error_status(err).into_response()),
    };
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    Ok(status.into_response())
}

/// The status to hand the page for a failed upstream call: the upstream's
/// own status when it answered (404 = stream gone), 502 for transport
/// failures.
fn upstream_error_status(err: reqwest::Error) -> StatusCode {
    match err.status() {
        Some(status) => StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
        None => StatusCode::BAD_GATEWAY,
    }
}
