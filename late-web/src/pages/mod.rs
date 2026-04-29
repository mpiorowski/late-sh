use crate::AppState;
use axum::Router;

pub mod chat;
pub mod connect;
pub mod dashboard;
pub mod gallery;
pub mod shared;
pub mod spectate;
pub mod stream;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/chat", chat::router())
        .merge(connect::router())
        .merge(gallery::router())
        .merge(spectate::router())
        .merge(stream::router())
        .nest("/dashboard", dashboard::router())
}
