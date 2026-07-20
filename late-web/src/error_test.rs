use crate::error::*;
use axum::http::StatusCode;
use axum::response::IntoResponse;

#[test]
fn from_anyhow_error() {
    let err = anyhow::anyhow!("test error");
    let app_err: AppError = err.into();
    assert!(matches!(app_err, AppError::Internal(_)));
}

#[test]
fn internal_error_returns_500() {
    let err = AppError::Internal(anyhow::anyhow!("something went wrong"));
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn render_error_returns_500() {
    // Create a render error using fmt error
    let err = AppError::Render(askama::Error::from(std::fmt::Error));
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn from_askama_error() {
    let err = askama::Error::from(std::fmt::Error);
    let app_err: AppError = err.into();
    assert!(matches!(app_err, AppError::Render(_)));
}
