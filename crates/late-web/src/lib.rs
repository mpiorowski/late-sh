pub mod config;
pub mod error;
mod metrics;
mod pages;

use axum::{
    Router,
    middleware::{self},
    response::IntoResponse,
    routing::get,
};
use late_core::telemetry::http_telemetry_middleware;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub config: config::Config,
    pub db: late_core::db::Db,
    pub http_client: reqwest::Client,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(pages::router())
        .route("/test", get(test_handler))
        .nest_service("/static", ServeDir::new("late-web/static"))
        .fallback(get(fallback_handler))
        .layer(middleware::from_fn(http_telemetry_middleware))
        .with_state(state)
}

async fn fallback_handler() -> impl IntoResponse {
    axum::response::Redirect::temporary("/")
}

async fn test_handler() -> Result<axum::response::Response, error::AppError> {
    tracing::error!(user_id = 123, "simulated error for testing");
    Err(anyhow::anyhow!("simulated error for testing").into())
}
