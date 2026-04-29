use std::net::{IpAddr, Ipv4Addr};

use askama::Template;
use axum::{
    Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::HeaderMap,
    response::{Html, IntoResponse},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use late_core::tunnel_protocol::{HandshakeContext, build_request};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::protocol::Message as UpstreamMessage;

use crate::{AppState, error::AppError, metrics};

const OUTBOUND_QUEUE_CAP: usize = 64;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spectate", get(page_handler))
        .route("/ws/spectate", get(ws_handler))
}

#[derive(Template)]
#[template(path = "pages/spectate/page.html")]
struct Page;

#[derive(Debug, Deserialize)]
struct SpectateParams {
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalSize {
    cols: u16,
    rows: u16,
}

impl TerminalSize {
    fn from_params(params: &SpectateParams, state: &AppState) -> Self {
        Self {
            cols: clamp_dimension(
                params.cols,
                state.config.spectator_default_cols,
                state.config.spectator_max_cols,
            ),
            rows: clamp_dimension(
                params.rows,
                state.config.spectator_default_rows,
                state.config.spectator_max_rows,
            ),
        }
    }
}

fn clamp_dimension(value: Option<u16>, default: u16, max: u16) -> u16 {
    value.unwrap_or(default).clamp(1, max.max(1))
}

async fn page_handler() -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("spectate", false);
    Ok(Html(Page.render()?))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<SpectateParams>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let size = TerminalSize::from_params(&params, &state);
    let client_ip = effective_client_ip(&headers).unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    ws.on_upgrade(move |socket| handle_socket(socket, state, size, client_ip))
}

async fn handle_socket(socket: WebSocket, state: AppState, size: TerminalSize, client_ip: IpAddr) {
    let session_id = uuid::Uuid::now_v7().to_string();
    let span = tracing::info_span!(
        "web.spectate.handshake",
        session_id = %session_id,
        client_ip = %client_ip,
        cols = size.cols,
        rows = size.rows
    );
    let guard = span.enter();

    let ctx = HandshakeContext {
        fingerprint: state.config.spectator_fingerprint.clone(),
        username: state.config.spectator_username.clone(),
        peer_ip: client_ip,
        term: "xterm-256color".to_string(),
        cols: size.cols,
        rows: size.rows,
        reconnect: false,
        session_id: session_id.clone(),
        view_only: true,
    };
    let req = match build_request(
        &state.config.tunnel_url,
        &state.config.tunnel_shared_secret,
        &ctx,
    ) {
        Ok(req) => req,
        Err(err) => {
            tracing::warn!(error = ?err, "failed to build spectate tunnel request");
            return;
        }
    };
    drop(guard);

    let upstream = match tokio_tungstenite::connect_async(req).await {
        Ok((upstream, response)) => {
            tracing::info!(
                session_id = %session_id,
                status = %response.status(),
                "web.spectate.upstream connected"
            );
            upstream
        }
        Err(err) => {
            tracing::warn!(session_id = %session_id, error = ?err, "web.spectate.upstream dial failed");
            return;
        }
    };

    proxy_websockets(socket, upstream, session_id).await;
}

async fn proxy_websockets(
    browser: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    session_id: String,
) {
    let (mut browser_sink, mut browser_stream) = browser.split();
    let (mut upstream_sink, mut upstream_stream) = upstream.split();
    let (browser_tx, mut browser_rx) = mpsc::channel::<Message>(OUTBOUND_QUEUE_CAP);
    let (upstream_tx, mut upstream_rx) = mpsc::channel::<UpstreamMessage>(OUTBOUND_QUEUE_CAP);

    let browser_writer = {
        let session_id = session_id.clone();
        tokio::spawn(async move {
            while let Some(msg) = browser_rx.recv().await {
                let close = matches!(msg, Message::Close(_));
                if let Err(err) = browser_sink.send(msg).await {
                    tracing::debug!(session_id = %session_id, error = ?err, "web.spectate.client send failed");
                    break;
                }
                if close {
                    break;
                }
            }
            let _ = browser_sink.close().await;
        })
    };

    let upstream_writer = {
        let session_id = session_id.clone();
        tokio::spawn(async move {
            while let Some(msg) = upstream_rx.recv().await {
                let close = matches!(msg, UpstreamMessage::Close(_));
                if let Err(err) = upstream_sink.send(msg).await {
                    tracing::debug!(session_id = %session_id, error = ?err, "web.spectate.upstream send failed");
                    break;
                }
                if close {
                    break;
                }
            }
            let _ = upstream_sink.close().await;
        })
    };

    loop {
        tokio::select! {
            msg = upstream_stream.next() => {
                let Some(msg) = msg else { break; };
                match msg {
                    Ok(UpstreamMessage::Binary(bytes)) => {
                        if browser_tx.send(Message::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Ok(UpstreamMessage::Text(text)) => {
                        tracing::debug!(session_id = %session_id, payload = %text.as_str(), "web.spectate.upstream text forwarded");
                        if browser_tx
                            .send(Message::Text(text.as_str().to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(UpstreamMessage::Close(frame)) => {
                        let _ = browser_tx.send(Message::Close(frame.map(to_browser_close))).await;
                        break;
                    }
                    Ok(UpstreamMessage::Ping(bytes)) => {
                        let _ = upstream_tx.send(UpstreamMessage::Pong(bytes)).await;
                    }
                    Ok(UpstreamMessage::Pong(_) | UpstreamMessage::Frame(_)) => {}
                    Err(err) => {
                        tracing::debug!(session_id = %session_id, error = ?err, "web.spectate.upstream recv failed");
                        break;
                    }
                }
            }
            msg = browser_stream.next() => {
                let Some(msg) = msg else { break; };
                match msg {
                    Ok(Message::Binary(bytes)) => {
                        if upstream_tx.send(UpstreamMessage::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Text(text)) => {
                        if upstream_tx
                            .send(UpstreamMessage::Text(text.as_str().to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        let _ = upstream_tx.send(UpstreamMessage::Close(frame.map(to_upstream_close))).await;
                        break;
                    }
                    Ok(Message::Ping(bytes)) => {
                        let _ = browser_tx.send(Message::Pong(bytes)).await;
                    }
                    Ok(Message::Pong(_)) => {}
                    Err(err) => {
                        tracing::debug!(session_id = %session_id, error = ?err, "web.spectate.client recv failed");
                        break;
                    }
                }
            }
        }
    }

    drop(browser_tx);
    drop(upstream_tx);
    let _ = browser_writer.await;
    let _ = upstream_writer.await;
    tracing::info!(session_id = %session_id, "web.spectate session ended");
}

fn to_browser_close(frame: tungstenite::protocol::CloseFrame) -> CloseFrame {
    CloseFrame {
        code: frame.code.into(),
        reason: frame.reason.to_string().into(),
    }
}

fn to_upstream_close(frame: CloseFrame) -> tungstenite::protocol::CloseFrame {
    tungstenite::protocol::CloseFrame {
        code: frame.code.into(),
        reason: frame.reason.to_string().into(),
    }
}

fn effective_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use late_core::db::{Db, DbConfig};

    fn state() -> AppState {
        AppState {
            config: crate::config::Config {
                port: 0,
                ssh_internal_url: "http://127.0.0.1:9".to_string(),
                ssh_public_url: "localhost:4000".to_string(),
                audio_base_url: "http://127.0.0.1:9".to_string(),
                tunnel_url: "ws://127.0.0.1:4001/tunnel".to_string(),
                tunnel_shared_secret: "secret".to_string(),
                spectator_username: "spectator".to_string(),
                spectator_fingerprint: "web-spectator:v1".to_string(),
                spectator_default_cols: 120,
                spectator_default_rows: 40,
                spectator_max_cols: 300,
                spectator_max_rows: 100,
            },
            db: Db::new(&DbConfig::default()).expect("lazy db"),
            http_client: reqwest::Client::new(),
        }
    }

    #[test]
    fn query_params_default_and_clamp() {
        let state = state();
        assert_eq!(
            TerminalSize::from_params(
                &SpectateParams {
                    cols: None,
                    rows: None
                },
                &state
            ),
            TerminalSize {
                cols: 120,
                rows: 40
            }
        );
        assert_eq!(
            TerminalSize::from_params(
                &SpectateParams {
                    cols: Some(999),
                    rows: Some(0)
                },
                &state
            ),
            TerminalSize { cols: 300, rows: 1 }
        );
    }

    #[test]
    fn x_forwarded_for_uses_first_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.7, 10.0.0.1"),
        );
        assert_eq!(
            effective_client_ip(&headers),
            Some("203.0.113.7".parse().unwrap())
        );
    }

    #[test]
    fn x_forwarded_for_malformed_is_none() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("not an ip"));
        assert_eq!(effective_client_ip(&headers), None);
    }
}
