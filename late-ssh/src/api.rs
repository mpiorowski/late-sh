use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, Query, State as AxumState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    http::{HeaderMap, HeaderValue},
    middleware::{self},
    response::IntoResponse,
    routing::get,
};
use late_core::MutexRecover;
use late_core::api_types::{NowPlayingResponse, StatusResponse, Track};
use late_core::telemetry::http_telemetry_middleware;
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

use crate::{
    metrics,
    session::{BrowserVizFrame, ClientAudioState, SessionMessage},
    state::{ActiveUsers, State},
};

#[derive(Deserialize)]
struct PairParams {
    token: String,
}

#[derive(Deserialize)]
#[serde(tag = "event")]
enum WsPayload {
    #[serde(rename = "heartbeat")]
    Heartbeat {},
    #[serde(rename = "viz")]
    Viz {
        position_ms: u64,
        bands: [f32; 8],
        rms: f32,
    },
    #[serde(rename = "client_state")]
    ClientState {
        client_kind: crate::session::ClientKind,
        muted: bool,
        volume_percent: u8,
    },
}

pub async fn run_api_server(
    port: u16,
    state: State,
    shutdown: Option<late_core::shutdown::CancellationToken>,
) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .context("failed to bind API server")?;
    tracing::info!(address = %addr, "api server listening");

    run_api_server_with_listener(listener, state, shutdown).await
}

pub async fn run_api_server_with_listener(
    listener: TcpListener,
    state: State,
    shutdown: Option<late_core::shutdown::CancellationToken>,
) -> Result<()> {
    let origins = state.config.allowed_origins.clone();
    let cors = CorsLayer::new()
        .allow_origin(
            origins
                .iter()
                .map(|s| parse_allowed_origin(s))
                .collect::<Vec<_>>(),
        )
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/health", get(get_health))
        .route("/api/now-playing", get(get_now_playing))
        .route("/api/status", get(get_status))
        .route("/api/ws/pair", get(ws_handler))
        .route("/api/ws/chat", get(crate::web::ws_chat_handler))
        .layer(cors)
        .layer(middleware::from_fn(http_telemetry_middleware))
        .with_state(state);

    let shutdown = shutdown.unwrap_or_default();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown.cancelled().await;
    })
    .await
    .context("API server failed")?;

    Ok(())
}

fn parse_allowed_origin(origin: &str) -> HeaderValue {
    origin.parse::<HeaderValue>().unwrap_or_else(|err| {
        panic!("invalid LATE_ALLOWED_ORIGINS entry '{origin}': {err}");
    })
}

async fn get_now_playing(AxumState(state): AxumState<State>) -> Json<NowPlayingResponse> {
    tracing::debug!("received request for now playing");
    let now_playing = state.now_playing_rx.borrow().clone();
    let listeners_count = active_user_count(&state.active_users);

    let (current_track, started_at_ts) = match now_playing {
        Some(np) => {
            let elapsed = np.started_at.elapsed().as_secs() as i64;
            let started_at_ts = chrono::Utc::now().timestamp() - elapsed;
            (np.track, started_at_ts)
        }
        None => (
            Track {
                title: "Unknown".to_string(),
                artist: None,
                duration_seconds: None,
            },
            chrono::Utc::now().timestamp(),
        ),
    };

    Json(NowPlayingResponse {
        current_track,
        listeners_count,
        started_at_ts,
    })
}

async fn get_health(AxumState(state): AxumState<State>) -> (StatusCode, &'static str) {
    if state.is_draining.load(std::sync::atomic::Ordering::Relaxed) {
        return (StatusCode::SERVICE_UNAVAILABLE, "draining");
    }

    // Short timeout so pool starvation fails fast instead of hanging k8s probes
    match tokio::time::timeout(std::time::Duration::from_secs(3), state.db.health()).await {
        Ok(Ok(())) => (StatusCode::OK, "ok"),
        Ok(Err(err)) => {
            tracing::warn!(error = ?err, "health check failed");
            (StatusCode::SERVICE_UNAVAILABLE, "db unavailable")
        }
        Err(_) => {
            tracing::warn!("health check timed out (pool likely exhausted)");
            (StatusCode::SERVICE_UNAVAILABLE, "db timeout")
        }
    }
}

async fn get_status(AxumState(state): AxumState<State>) -> Json<StatusResponse> {
    tracing::info!("received request for status");
    let active = active_user_count(&state.active_users);
    Json(StatusResponse {
        online: true,
        message: format!("{} users online", active),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

fn active_user_count(active_users: &ActiveUsers) -> usize {
    let users = active_users.lock_recover();
    users.len()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<PairParams>,
    AxumState(state): AxumState<State>,
    headers: HeaderMap,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let client_ip = effective_client_ip(&headers, peer_addr, &state);
    let token_hint = token_hint(&params.token);
    tracing::info!(
        ip = %client_ip,
        peer_ip = %peer_addr.ip(),
        token_hint = %token_hint,
        "ws pair request received"
    );
    if !state.ws_pair_limiter.allow(client_ip) {
        tracing::warn!(
            ip = %client_ip,
            peer_ip = %peer_addr.ip(),
            max_attempts = state.ws_pair_limiter.max_attempts(),
            window_secs = state.ws_pair_limiter.window_secs(),
            "ws pair rate limit exceeded for peer ip"
        );
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    ws.on_upgrade(move |socket| async move { handle_socket(socket, params.token, state).await })
}

async fn handle_socket(mut socket: WebSocket, token: String, state: State) {
    let token_hint = token_hint(&token);
    let (control_tx, mut control_rx) = tokio::sync::mpsc::unbounded_channel();
    let registration_id = state
        .paired_client_registry
        .register(token.clone(), control_tx);
    metrics::record_ws_pair_success();
    tracing::info!(token_hint = %token_hint, "ws pair websocket established");

    loop {
        tokio::select! {
            maybe_msg = socket.recv() => {
                let Some(msg) = maybe_msg else {
                    break;
                };

                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(token_hint = %token_hint, error = ?e, "websocket dirty close or error");
                        break;
                    }
                };

                match msg {
                    Message::Text(text) => {
                        let payload = match serde_json::from_str::<WsPayload>(&text) {
                            Ok(payload) => payload,
                            Err(e) => {
                                tracing::error!(
                                    token_hint = %token_hint,
                                    error = ?e,
                                    "failed to parse ws payload"
                                );
                                continue;
                            }
                        };

                        let msg = match payload {
                            WsPayload::Heartbeat { .. } => SessionMessage::Heartbeat,
                            WsPayload::Viz {
                                position_ms,
                                bands,
                                rms,
                            } => SessionMessage::Viz(BrowserVizFrame {
                                position_ms,
                                bands,
                                rms,
                            }),
                            WsPayload::ClientState {
                                client_kind,
                                muted,
                                volume_percent,
                            } => {
                                state.paired_client_registry.update_state(
                                    &token,
                                    registration_id,
                                    ClientAudioState {
                                        client_kind,
                                        muted,
                                        volume_percent,
                                    },
                                );
                                continue;
                            }
                        };

                        if !state.session_registry.send_message(&token, msg).await {
                            tracing::warn!(
                                token_hint = %token_hint,
                                "ws pair message could not be routed to a live session"
                            );
                            break;
                        }
                    }
                    Message::Close(_) => {
                        tracing::info!(token_hint = %token_hint, "websocket close received");
                        break;
                    }
                    _ => {}
                }
            }
            maybe_control = control_rx.recv() => {
                let Some(control) = maybe_control else {
                    break;
                };

                let payload = match serde_json::to_string(&control) {
                    Ok(payload) => payload,
                    Err(err) => {
                        tracing::error!(token_hint = %token_hint, error = ?err, "failed to serialize browser control payload");
                        continue;
                    }
                };

                if let Err(err) = socket.send(Message::Text(payload.into())).await {
                    tracing::warn!(token_hint = %token_hint, error = ?err, "failed to send browser control payload");
                    break;
                }
            }
        }
    }

    state
        .paired_client_registry
        .unregister_if_match(&token, registration_id);
    tracing::info!(token_hint = %token_hint, "websocket connection closed");
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

fn effective_client_ip(headers: &HeaderMap, peer_addr: SocketAddr, state: &State) -> IpAddr {
    if is_trusted_proxy_peer(peer_addr.ip(), &state.config.ssh_proxy_trusted_cidrs)
        && let Some(ip) = forwarded_for_ip(headers)
    {
        return ip;
    }

    peer_addr.ip()
}

fn is_trusted_proxy_peer(ip: IpAddr, trusted_cidrs: &[ipnet::IpNet]) -> bool {
    trusted_cidrs.iter().any(|cidr| cidr.contains(&ip))
}

fn forwarded_for_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = value.split(',').next()?.trim();
    first.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ActiveUser;
    use ipnet::IpNet;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Instant,
    };
    use uuid::Uuid;

    #[test]
    fn parse_allowed_origin_accepts_valid_origin() {
        let value = parse_allowed_origin("https://late.sh");
        assert_eq!(value, HeaderValue::from_static("https://late.sh"));
    }

    #[test]
    #[should_panic(expected = "invalid LATE_ALLOWED_ORIGINS entry")]
    fn parse_allowed_origin_panics_for_invalid_origin() {
        let _ = parse_allowed_origin("bad\norigin");
    }

    #[test]
    fn ws_payload_heartbeat_parses() {
        let json = r#"{"event": "heartbeat"}"#;
        let payload: WsPayload = serde_json::from_str(json).unwrap();
        assert!(matches!(payload, WsPayload::Heartbeat { .. }));
    }

    #[test]
    fn ws_payload_viz_parses() {
        let json = r#"{
            "event": "viz",
            "position_ms": 1500,
            "bands": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            "rms": 0.42
        }"#;
        let payload: WsPayload = serde_json::from_str(json).unwrap();
        match payload {
            WsPayload::Viz {
                position_ms,
                bands,
                rms,
            } => {
                assert_eq!(position_ms, 1500);
                assert_eq!(bands.len(), 8);
                assert!((rms - 0.42).abs() < f32::EPSILON);
            }
            _ => panic!("expected Viz"),
        }
    }

    #[test]
    fn ws_payload_client_state_parses() {
        let json = r#"{
            "event": "client_state",
            "client_kind": "cli",
            "muted": true,
            "volume_percent": 35
        }"#;
        let payload: WsPayload = serde_json::from_str(json).unwrap();
        match payload {
            WsPayload::ClientState {
                client_kind,
                muted,
                volume_percent,
            } => {
                assert_eq!(client_kind, crate::session::ClientKind::Cli);
                assert!(muted);
                assert_eq!(volume_percent, 35);
            }
            _ => panic!("expected ClientState"),
        }
    }

    #[test]
    fn ws_payload_unknown_event_fails() {
        let json = r#"{"event": "unknown"}"#;
        assert!(serde_json::from_str::<WsPayload>(json).is_err());
    }

    #[test]
    fn ws_payload_viz_missing_fields_fails() {
        let json = r#"{"event": "viz", "position_ms": 1000}"#;
        assert!(serde_json::from_str::<WsPayload>(json).is_err());
    }

    #[test]
    fn ws_payload_viz_wrong_bands_count_fails() {
        let json = r#"{
            "event": "viz",
            "position_ms": 1000,
            "bands": [0.1, 0.2],
            "rms": 0.5
        }"#;
        assert!(serde_json::from_str::<WsPayload>(json).is_err());
    }

    #[test]
    fn token_hint_redacts_full_value() {
        let hint = token_hint("12345678-abcd-efgh");
        assert_eq!(hint, "12345678..(18)");
    }

    #[test]
    fn active_user_count_uses_unique_user_entries() {
        let active_users: ActiveUsers = Arc::new(Mutex::new(HashMap::new()));
        let mut users = active_users.lock().unwrap();
        users.insert(
            Uuid::now_v7(),
            ActiveUser {
                username: "alice".to_string(),
                connection_count: 2,
                last_login_at: Instant::now(),
            },
        );
        users.insert(
            Uuid::now_v7(),
            ActiveUser {
                username: "bob".to_string(),
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
        drop(users);

        assert_eq!(active_user_count(&active_users), 2);
    }

    #[test]
    fn forwarded_for_ip_uses_first_entry() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
        );

        assert_eq!(
            forwarded_for_ip(&headers),
            Some("203.0.113.10".parse().unwrap())
        );
    }

    #[test]
    fn effective_client_ip_uses_forwarded_header_for_trusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
        );
        let trusted_cidrs = test_trusted_cidrs(vec!["10.42.0.0/16"]);
        let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

        assert_eq!(
            if is_trusted_proxy_peer(peer_addr.ip(), &trusted_cidrs)
                && let Some(ip) = forwarded_for_ip(&headers)
            {
                ip
            } else {
                peer_addr.ip()
            },
            "203.0.113.10".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn effective_client_ip_falls_back_for_untrusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
        );
        let trusted_cidrs = test_trusted_cidrs(vec!["192.168.0.0/16"]);
        let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

        assert_eq!(
            if is_trusted_proxy_peer(peer_addr.ip(), &trusted_cidrs)
                && let Some(ip) = forwarded_for_ip(&headers)
            {
                ip
            } else {
                peer_addr.ip()
            },
            "10.42.0.89".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn effective_client_ip_falls_back_when_header_missing() {
        let headers = HeaderMap::new();
        let trusted_cidrs = test_trusted_cidrs(vec!["10.42.0.0/16"]);
        let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

        assert_eq!(
            if is_trusted_proxy_peer(peer_addr.ip(), &trusted_cidrs)
                && let Some(ip) = forwarded_for_ip(&headers)
            {
                ip
            } else {
                peer_addr.ip()
            },
            "10.42.0.89".parse::<IpAddr>().unwrap()
        );
    }

    fn test_trusted_cidrs(cidr_strings: Vec<&str>) -> Vec<IpNet> {
        cidr_strings
            .into_iter()
            .map(|s| s.parse::<IpNet>().unwrap())
            .collect()
    }
}
