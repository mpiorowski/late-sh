use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, Query, State as AxumState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, HeaderValue},
    http::StatusCode,
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
    if is_trusted_proxy_peer(peer_addr.ip(), state) {
        if let Some(ip) = forwarded_for_ip(headers) {
            return ip;
        }
    }

    peer_addr.ip()
}

fn is_trusted_proxy_peer(ip: IpAddr, state: &State) -> bool {
    state
        .config
        .ssh_proxy_trusted_cidrs
        .iter()
        .any(|cidr| cidr.contains(&ip))
}

fn forwarded_for_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = value.split(',').next()?.trim();
    first.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{AiConfig, Config},
        state::ActiveUser,
    };
    use ipnet::IpNet;
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Instant,
    };
    use late_core::db::DbConfig;
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

        assert_eq!(forwarded_for_ip(&headers), Some("203.0.113.10".parse().unwrap()));
    }

    #[test]
    fn effective_client_ip_uses_forwarded_header_for_trusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
        );
        let state = test_state_with_trusted_cidrs(vec!["10.42.0.0/16"]);
        let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

        assert_eq!(
            effective_client_ip(&headers, peer_addr, &state),
            "203.0.113.10".parse().unwrap()
        );
    }

    #[test]
    fn effective_client_ip_falls_back_for_untrusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 10.42.0.89"),
        );
        let state = test_state_with_trusted_cidrs(vec!["192.168.0.0/16"]);
        let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

        assert_eq!(
            effective_client_ip(&headers, peer_addr, &state),
            "10.42.0.89".parse().unwrap()
        );
    }

    #[test]
    fn effective_client_ip_falls_back_when_header_missing() {
        let headers = HeaderMap::new();
        let state = test_state_with_trusted_cidrs(vec!["10.42.0.0/16"]);
        let peer_addr: SocketAddr = "10.42.0.89:12345".parse().unwrap();

        assert_eq!(
            effective_client_ip(&headers, peer_addr, &state),
            "10.42.0.89".parse().unwrap()
        );
    }

    fn test_state_with_trusted_cidrs(cidr_strings: Vec<&str>) -> State {
        let ssh_proxy_trusted_cidrs = cidr_strings
            .into_iter()
            .map(|s| s.parse::<IpNet>().unwrap())
            .collect();

        State {
            config: Config {
                ssh_port: 2222,
                api_port: 4000,
                icecast_url: "http://icecast".to_string(),
                web_url: "https://late.sh".to_string(),
                open_access: true,
                force_admin: false,
                db: DbConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    user: "user".to_string(),
                    password: "password".to_string(),
                    dbname: "late".to_string(),
                    max_pool_size: 16,
                },
                max_conns_global: 100,
                max_conns_per_ip: 3,
                ssh_idle_timeout: 3600,
                server_key_path: PathBuf::from("/tmp/server_key"),
                allowed_origins: vec!["https://late.sh".to_string()],
                liquidsoap_addr: "liquidsoap:1234".to_string(),
                frame_drop_log_every: 100,
                vote_switch_interval_secs: 3600,
                ssh_max_attempts_per_ip: 30,
                ssh_rate_limit_window_secs: 60,
                ssh_proxy_protocol: true,
                ssh_proxy_trusted_cidrs,
                ws_pair_max_attempts_per_ip: 30,
                ws_pair_rate_limit_window_secs: 60,
                ai: AiConfig {
                    enabled: false,
                    api_key: None,
                    model: "test-model".to_string(),
                },
            },
            db: panic_unreachable("db"),
            ai_service: panic_unreachable("ai_service"),
            vote_service: panic_unreachable("vote_service"),
            chat_service: panic_unreachable("chat_service"),
            notification_service: panic_unreachable("notification_service"),
            article_service: panic_unreachable("article_service"),
            profile_service: panic_unreachable("profile_service"),
            twenty_forty_eight_service: panic_unreachable("twenty_forty_eight_service"),
            tetris_service: panic_unreachable("tetris_service"),
            sudoku_service: panic_unreachable("sudoku_service"),
            nonogram_service: panic_unreachable("nonogram_service"),
            solitaire_service: panic_unreachable("solitaire_service"),
            minesweeper_service: panic_unreachable("minesweeper_service"),
            bonsai_service: panic_unreachable("bonsai_service"),
            nonogram_library: panic_unreachable("nonogram_library"),
            chip_service: panic_unreachable("chip_service"),
            blackjack_service: panic_unreachable("blackjack_service"),
            leaderboard_service: panic_unreachable("leaderboard_service"),
            conn_limit: panic_unreachable("conn_limit"),
            conn_counts: panic_unreachable("conn_counts"),
            active_users: panic_unreachable("active_users"),
            activity_feed: panic_unreachable("activity_feed"),
            now_playing_rx: panic_unreachable("now_playing_rx"),
            session_registry: panic_unreachable("session_registry"),
            paired_client_registry: panic_unreachable("paired_client_registry"),
            web_chat_registry: panic_unreachable("web_chat_registry"),
            ssh_attempt_limiter: panic_unreachable("ssh_attempt_limiter"),
            ws_pair_limiter: panic_unreachable("ws_pair_limiter"),
            is_draining: panic_unreachable("is_draining"),
        }
    }

    fn panic_unreachable<T>(field: &str) -> T {
        panic!("{field} should not be used in this unit test")
    }
}
