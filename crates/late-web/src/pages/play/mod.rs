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
        .route("/play", get(handler))
        .route("/play/listeners", get(listeners_handler))
}

#[derive(Template)]
#[template(path = "pages/play/page.html")]
struct Page {
    tunnel_url_json: String,
    listeners_count: usize,
}

async fn handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("play", false);
    let listeners_count = fetch_listeners_count(&state).await;
    let tunnel_url_json = serde_json::to_string(&tunnel_ws_url(
        &state.config.ssh_public_url,
        &state.config.web_tunnel_token,
    ))
    .unwrap_or_else(|_| "\"\"".to_string());
    let page = Page {
        tunnel_url_json,
        listeners_count,
    };
    Ok(Html(page.render()?))
}

async fn listeners_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    Ok(Html(fetch_listeners_count(&state).await.to_string()))
}

async fn fetch_listeners_count(state: &AppState) -> usize {
    now_playing::fetch(state)
        .await
        .unwrap_or_default()
        .listeners_count
        .unwrap_or_default()
}

fn tunnel_ws_url(public_url: &str, token: &str) -> String {
    let base = public_url.trim_end_matches('/');
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if is_local_host(base) {
        format!("ws://{base}")
    } else {
        format!("wss://{base}")
    };
    format!("{ws_base}/api/ws/tunnel?token={}", query_encode(token))
}

fn is_local_host(value: &str) -> bool {
    let host = value.split('/').next().unwrap_or(value);
    host.starts_with("localhost:")
        || host == "localhost"
        || host.starts_with("127.0.0.1:")
        || host == "127.0.0.1"
        || host.starts_with("[::1]:")
        || host == "[::1]"
}

fn query_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::tunnel_ws_url;

    #[test]
    fn tunnel_ws_url_uses_wss_for_https() {
        assert_eq!(
            tunnel_ws_url("https://api.late.sh/", "secret"),
            "wss://api.late.sh/api/ws/tunnel?token=secret"
        );
    }

    #[test]
    fn tunnel_ws_url_uses_ws_for_http() {
        assert_eq!(
            tunnel_ws_url("http://localhost:4000", "secret"),
            "ws://localhost:4000/api/ws/tunnel?token=secret"
        );
    }

    #[test]
    fn tunnel_ws_url_accepts_host_without_scheme() {
        assert_eq!(
            tunnel_ws_url("localhost:4000", "secret"),
            "ws://localhost:4000/api/ws/tunnel?token=secret"
        );
    }

    #[test]
    fn tunnel_ws_url_defaults_public_hosts_to_wss() {
        assert_eq!(
            tunnel_ws_url("api.late.sh", "secret"),
            "wss://api.late.sh/api/ws/tunnel?token=secret"
        );
    }

    #[test]
    fn tunnel_ws_url_escapes_token() {
        assert_eq!(
            tunnel_ws_url("https://api.late.sh", "a b&c"),
            "wss://api.late.sh/api/ws/tunnel?token=a%20b%26c"
        );
    }
}
