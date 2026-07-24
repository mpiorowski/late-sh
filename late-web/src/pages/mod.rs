use crate::AppState;
use axum::Router;

pub(crate) mod connect;
pub(crate) mod gallery;
pub(crate) mod home;
pub(crate) mod profiles;
pub(crate) mod shared;
pub(crate) mod stream;

#[cfg(test)]
mod stream_test;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .merge(home::router())
        .merge(connect::router())
        .merge(gallery::router())
        .merge(profiles::router())
        .merge(stream::router())
}
