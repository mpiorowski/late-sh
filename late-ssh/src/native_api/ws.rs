use axum::{
    Router,
    extract::{ConnectInfo, Query, State as AxumState, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
    routing::get,
};
use late_core::models::{
    chat_message::ChatMessage,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    native_token::NativeToken,
    user::User,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

use crate::app::{chat::svc::ChatEvent, vote::svc::Genre};
use crate::state::State;

use super::chat::{MessageItem, build_message_items};
use super::media::{
    NowPlayingResponse, VotesResponse, build_now_playing_from_value, build_now_playing_response,
    build_votes_response, build_votes_response_from_snapshot,
};

pub fn router() -> Router<State> {
    Router::new().route("/api/ws/native", get(ws_native_handler))
}

// ── Auth params ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct WsNativeParams {
    /// Short-lived one-time ticket from `GET /api/native/ws-ticket` (preferred).
    ticket: Option<String>,
    /// Long-lived bearer token fallback for clients that cannot set headers.
    token: Option<String>,
}

// ── Outbound message types ────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsOut {
    Init {
        rooms: Vec<WsRoom>,
        online_users: Vec<WsUser>,
        now_playing: NowPlayingResponse,
        votes: VotesResponse,
        messages: Vec<MessageItem>,
    },
    Message {
        room_id: String,
        msg: MessageItem,
    },
    #[allow(dead_code)]
    Presence {
        event: String,
        username: String,
    },
    NowPlaying(NowPlayingResponse),
    Votes(VotesResponse),
    #[allow(dead_code)]
    Ping,
}

#[derive(Serialize)]
struct WsRoom {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct WsUser {
    username: String,
}

// ── Inbound message types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WsInAny {
    #[serde(rename = "type")]
    kind: String,
    body: Option<String>,
    genre: Option<String>,
    #[allow(dead_code)]
    room_id: Option<serde_json::Value>,
}

// ── Handler ───────────────────────────────────────────────────────────────────

async fn ws_native_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(params): Query<WsNativeParams>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    AxumState(state): AxumState<State>,
) -> impl IntoResponse {
    let client_ip = crate::api::effective_client_ip(&headers, peer_addr, &state);
    if !state.native_ws_limiter.allow(client_ip) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    // Auth priority: short-lived ticket → Authorization header → token query param.
    let identity: Option<(Uuid, String)> = if let Some(ticket) = params.ticket {
        state.native_ws_tickets.consume(&ticket)
    } else {
        let raw_token = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.trim().to_owned())
            .or(params.token);

        if let Some(raw_token) = raw_token {
            let Ok(client) = state.db.get().await else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            NativeToken::find_user_by_token(&client, &raw_token).await.ok().flatten()
        } else {
            None
        }
    };

    let Some((user_id, username)) = identity else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    ws.on_upgrade(move |socket| handle_native_socket(socket, user_id, username, state))
}

// ── Socket loop ───────────────────────────────────────────────────────────────

async fn handle_native_socket(mut socket: WebSocket, user_id: Uuid, _username: String, state: State) {
    let Ok(client) = state.db.get().await else { return };
    let Some(room) = ChatRoom::find_general(&client).await.ok().flatten() else { return };
    let room_id = room.id;

    let messages = ChatMessage::list_recent(&client, room_id, 50).await.unwrap_or_default();
    // Seed the username cache from the initial message batch before consuming it.
    let init_author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
    let mut username_cache = User::list_usernames_by_ids(&client, &init_author_ids)
        .await
        .unwrap_or_default();
    let msg_items = build_message_items(&client, messages).await;
    drop(client);

    let online = {
        let users = state.active_users.lock().unwrap_or_else(|e| e.into_inner());
        users.values().map(|u| WsUser { username: u.username.clone() }).collect::<Vec<_>>()
    };

    let init = WsOut::Init {
        rooms: vec![WsRoom { id: room_id.to_string(), name: "General".to_string() }],
        online_users: online,
        now_playing: build_now_playing_response(&state),
        votes: build_votes_response(&state),
        messages: msg_items,
    };
    if send_json(&mut socket, &init).await.is_err() {
        return;
    }

    let mut chat_rx = state.chat_service.subscribe_events();
    let mut vote_rx = state.vote_service.subscribe_state();
    let mut np_rx = state.now_playing_rx.clone();
    let mut active_room_id = room_id;

    loop {
        tokio::select! {
            maybe_msg = socket.recv() => {
                let Some(Ok(Message::Text(text))) = maybe_msg else { break };
                let Ok(payload) = serde_json::from_str::<WsInAny>(&text) else { continue };
                match payload.kind.as_str() {
                    "send" => {
                        if let Some(body) = payload.body.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
                            if body.len() <= 4000 {
                                let slug = if active_room_id == room_id {
                                    Some("general".to_string())
                                } else {
                                    // Look up the slug for the current room for permission checks
                                    if let Ok(c) = state.db.get().await {
                                        ChatRoom::get(&c, active_room_id).await.ok().flatten().and_then(|r| r.slug)
                                    } else {
                                        None
                                    }
                                };
                                state.chat_service.send_message_task(
                                    user_id,
                                    active_room_id,
                                    slug,
                                    body.to_string(),
                                    Uuid::now_v7(),
                                    false,
                                );
                            }
                        }
                    }
                    "subscribe" => {
                        if let Some(new_id) = payload.room_id.as_ref().and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
                            if let Ok(client) = state.db.get().await {
                                if ChatRoomMember::is_member(&client, new_id, user_id).await.unwrap_or(false) {
                                    active_room_id = new_id;
                                }
                            }
                        }
                    }
                    "vote" => {
                        if let Some(genre_str) = &payload.genre {
                            if let Ok(genre) = Genre::try_from(genre_str.as_str()) {
                                state.vote_service.cast_vote_task(user_id, genre);
                            }
                        }
                    }
                    "pong" => {}
                    _ => {}
                }
            }
            Ok(event) = chat_rx.recv() => {
                match event {
                    ChatEvent::MessageCreated { message, author_username, .. }
                        if message.room_id == active_room_id =>
                    {
                        let author = if let Some(name) = author_username {
                            username_cache.insert(message.user_id, name.clone());
                            name
                        } else if let Some(name) = username_cache.get(&message.user_id).cloned() {
                            name
                        } else if let Ok(c) = state.db.get().await {
                            let names = User::list_usernames_by_ids(&c, &[message.user_id])
                                .await
                                .unwrap_or_default();
                            let name = names.get(&message.user_id).cloned().unwrap_or_default();
                            username_cache.insert(message.user_id, name.clone());
                            name
                        } else {
                            String::new()
                        };
                        let out = WsOut::Message {
                            room_id: active_room_id.to_string(),
                            msg: MessageItem {
                                id: message.id.to_string(),
                                user_id: message.user_id.to_string(),
                                username: author,
                                body: message.body.clone(),
                                timestamp: message.created.to_rfc3339(),
                                reactions: vec![],
                            },
                        };
                        if send_json(&mut socket, &out).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            Ok(()) = vote_rx.changed() => {
                let out = WsOut::Votes(build_votes_response_from_snapshot(&vote_rx.borrow_and_update()));
                if send_json(&mut socket, &out).await.is_err() { break; }
            }
            Ok(()) = np_rx.changed() => {
                let out = WsOut::NowPlaying(build_now_playing_from_value(&np_rx.borrow_and_update()));
                if send_json(&mut socket, &out).await.is_err() { break; }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn send_json<T: Serialize>(socket: &mut WebSocket, val: &T) -> Result<(), ()> {
    let json = serde_json::to_string(val).map_err(|_| ())?;
    socket.send(Message::Text(json.into())).await.map_err(|_| ())
}
