use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
};

use crate::{AppState, error::AppError, metrics, pages::shared::now_playing};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(token_handler))
        .route("/pair/status", get(status_handler))
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

#[derive(Template)]
#[template(path = "pages/connect/status.html")]
struct Status {
    now_playing_title: Option<String>,
    now_playing_artist: Option<String>,
    listeners_count: Option<usize>,
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

async fn status_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let np = now_playing::fetch(&state).await?;
    let status = Status {
        now_playing_title: np.title,
        now_playing_artist: np.artist.or(Some("Unknown".to_string())),
        listeners_count: np.listeners_count,
    };
    Ok(Html(status.render()?))
}
