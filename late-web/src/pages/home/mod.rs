use askama::Template;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};

use crate::{AppState, error::AppError, metrics, pages::shared::now_playing};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root_handler))
        .route("/status", get(status_handler))
}

impl Home {
    fn active_page(&self) -> &str {
        "/"
    }
}

#[derive(Template)]
#[template(path = "pages/home/page.html")]
struct Home;

#[derive(Template)]
#[template(path = "pages/home/status.html")]
struct HomeStatus {
    now_playing_title: Option<String>,
    listeners_count: Option<usize>,
}

async fn root_handler() -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("home", false);
    Ok(Html(Home.render()?))
}

async fn status_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let np = now_playing::fetch(&state).await?;
    let status = HomeStatus {
        now_playing_title: np.title,
        listeners_count: np.listeners_count,
    };
    Ok(Html(status.render()?))
}
