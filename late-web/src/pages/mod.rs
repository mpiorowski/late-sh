use crate::AppState;
use axum::Router;

pub mod connect;
pub mod dashboard;
pub mod gallery;
pub mod home;
pub mod play;
pub mod profiles;
pub mod shared;
pub mod stream;
pub mod voice;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(home::router())
        .merge(connect::router())
        .merge(gallery::router())
        .merge(play::router())
        .merge(profiles::router())
        .merge(stream::router())
        .merge(voice::router())
        .nest("/dashboard", dashboard::router())
}
