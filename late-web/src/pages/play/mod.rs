use askama::Template;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};

use crate::{AppState, error::AppError, metrics};

pub fn router() -> Router<AppState> {
    Router::new().route("/play", get(handler))
}

#[derive(Template)]
#[template(path = "pages/play/page.html")]
struct Page {
    tunnel_url_json: String,
    enabled: bool,
}

async fn handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("play", false);
    let tunnel_url_json = state
        .config
        .web_tunnel_token
        .as_ref()
        .map(|token| tunnel_ws_url(&state.config.ssh_public_url, token))
        .and_then(|url| serde_json::to_string(&url).ok())
        .unwrap_or_default();
    let page = Page {
        tunnel_url_json,
        enabled: state.config.web_tunnel_enabled && state.config.web_tunnel_token.is_some(),
    };
    Ok(Html(page.render()?))
}

fn tunnel_ws_url(public_url: &str, token: &str) -> String {
    let base = public_url.trim_end_matches('/');
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        format!("ws://{base}")
    };
    format!("{ws_base}/api/ws/tunnel?token={}", query_encode(token))
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
    fn tunnel_ws_url_escapes_token() {
        assert_eq!(
            tunnel_ws_url("https://api.late.sh", "a b&c"),
            "wss://api.late.sh/api/ws/tunnel?token=a%20b%26c"
        );
    }
}
