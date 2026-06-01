use askama::Template;
use axum::{Router, extract::State, response::Html, routing::get};

use crate::{AppState, error::AppError, metrics};

pub fn router() -> Router<AppState> {
    Router::new().route("/voice", get(handler))
}

impl Page {
    fn active_page(&self) -> &str {
        "voice"
    }
}

#[derive(Template)]
#[template(path = "pages/voice/page.html")]
struct Page {
    api_url: String,
}

async fn handler(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    metrics::record_page_view("voice", true);
    let page = Page {
        api_url: state.config.ssh_public_url.clone(),
    };
    Ok(Html(page.render()?))
}
