use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

pub enum AppError {
    Internal(anyhow::Error),
    Render(askama::Error),
}

#[derive(Template)]
#[template(path = "pages/error.html")]
struct ErrorPage<'a> {
    message: &'a str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Internal(err) => {
                late_core::error_span!("web_internal_error", error = ?err, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Internal server error: {}", err),
                )
            }
            AppError::Render(err) => {
                late_core::error_span!("web_render_error", error = ?err, "template render error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Could not render template: {}", err),
                )
            }
        };
        let page = ErrorPage { message: &message };
        match page.render() {
            Ok(body) => (status, Html(body)).into_response(),
            Err(_) => (status, message).into_response(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

impl From<askama::Error> for AppError {
    fn from(err: askama::Error) -> Self {
        AppError::Render(err)
    }
}
