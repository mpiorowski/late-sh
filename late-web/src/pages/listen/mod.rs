use anyhow::Context;
use askama::Template;
use axum::{
    Json, Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use late_core::telemetry::TracedExt;

use crate::{AppState, error::AppError, metrics};

#[cfg(test)]
mod listen_test;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/listen", get(page_handler))
        .route("/listen/state", get(state_handler))
}

#[derive(Template)]
#[template(path = "pages/listen/page.html")]
struct Page;

async fn page_handler() -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("listen", false);
    Ok(Html(Page.render()?))
}

/// Same-origin proxy of late-ssh's `/api/listen`, so the page polls one URL on
/// its own origin and the late-ssh API never has to be reachable from a
/// browser. The body is forwarded verbatim: late-web adds nothing here, and
/// re-declaring the response types only to re-serialize them would duplicate
/// the contract in two crates with no validation gained.
async fn state_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let url = format!("{}/api/listen", state.config.ssh_internal_url);

    let response = state
        .http_client
        .get(&url)
        .send_traced()
        .await
        .map_err(|err| {
            late_core::error_span!("listen_fetch_failed", error = ?err, url = %url, "failed to fetch listen state");
            err
        })
        .context("failed to fetch listen state")?;

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| {
            late_core::error_span!("listen_parse_failed", error = ?err, "failed to parse listen state");
            err
        })
        .context("failed to parse listen state")?;

    Ok(Json(body))
}
