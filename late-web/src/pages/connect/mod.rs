use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
};

use crate::{AppState, error::AppError, metrics};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/{token}", get(token_handler))
}

impl Page {
    fn active_page(&self) -> &str {
        "connect"
    }
}

#[derive(Template)]
#[template(path = "pages/connect/page.html")]
struct Page {
    token: String,
    api_url: String,
    audio_url: String,
}

async fn token_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("connect", !token.is_empty());
    let page = Page {
        token,
        api_url: state.config.ssh_public_url.clone(),
        audio_url: "/stream".to_string(),
    };
    Ok(Html(page.render()?))
}
