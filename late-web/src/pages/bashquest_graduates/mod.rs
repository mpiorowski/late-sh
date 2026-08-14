// Public, read-only JSON listing of verified BashQuest graduates. Consumed
// by a scheduled GitHub Actions job in the separate hardlygospel/bashquest-
// graduates repo, which mirrors this list into a public GitHub Pages gallery
// (one page per graduate). Every row this returns was written by late-ssh
// only after late-bashquest independently confirmed a real certificate file
// on its own PVC -- see `late_core::models::bashquest_graduate` and
// `late-ssh/src/app/door/bashquest/CONTEXT.md` for the full chain of trust.
// This endpoint adds no trust of its own; it just republishes the DB.

use anyhow::Context;
use axum::{Json, Router, extract::State, response::IntoResponse, routing::get};
use late_core::models::bashquest_graduate::BashquestGraduate;
use serde::Serialize;

use crate::{AppState, error::AppError, metrics};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/bashquest/graduates", get(handler))
}

#[derive(Serialize)]
struct GraduatePayload {
    handle: String,
    certificate: String,
    certificate_digest: String,
    graduated_at: String,
}

impl From<BashquestGraduate> for GraduatePayload {
    fn from(g: BashquestGraduate) -> Self {
        Self {
            handle: g.handle,
            certificate: g.certificate,
            certificate_digest: g.certificate_digest,
            graduated_at: g.created.to_rfc3339(),
        }
    }
}

#[tracing::instrument(skip_all)]
async fn handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("bashquest_graduates", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for bashquest graduates")?;
    let graduates = BashquestGraduate::list_all(&client)
        .await
        .context("failed to load bashquest graduates")?;

    let payload: Vec<GraduatePayload> = graduates.into_iter().map(Into::into).collect();
    Ok(Json(payload))
}

#[cfg(test)]
mod bashquest_graduates_test;
