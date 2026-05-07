use axum::{
    Json, Router,
    extract::{
        FromRequestParts, Path, Query, State as AxumState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{Duration, Utc};
use late_core::models::{
    bonsai::Tree,
    chat_message::ChatMessage,
    chat_room::ChatRoom,
    native_token::NativeToken,
    user::User,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app::{
        bonsai::{state::stage_for, ui::tree_ascii},
        chat::svc::ChatEvent,
        vote::svc::{Genre, VoteSnapshot},
    },
    state::{ActiveUsers, State},
};

// ── Token lifetime ────────────────────────────────────────────────────────────

const TOKEN_DAYS: i64 = 30;

// ── Auth extractor ────────────────────────────────────────────────────────────

pub struct NativeAuthUser {
    pub user_id: Uuid,
    pub username: String,
}

fn api_error(status: StatusCode, msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

impl FromRequestParts<State> for NativeAuthUser {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "missing bearer token"))?
            .to_owned();

        let client = state
            .db
            .get()
            .await
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

        let (user_id, username) = NativeToken::find_user_by_token(&client, &token)
            .await
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?
            .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "invalid or expired token"))?;

        Ok(NativeAuthUser { user_id, username })
    }
}

// ── Route builder ────────────────────────────────────────────────────────────

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/challenge", get(get_challenge))
        .route("/api/native/token", post(post_token))
        .route("/api/native/me", get(get_me))
        .route("/api/native/rooms", get(get_rooms))
        .route("/api/native/rooms/{room}/history", get(get_room_history))
        .route("/api/native/users/online", get(get_online_users))
        .route("/api/native/now-playing", get(get_now_playing))
        .route("/api/native/status", get(get_native_status))
        .route("/api/native/vote", post(post_vote))
        .route("/api/native/bonsai", get(get_bonsai))
        .route("/api/native/bonsai/water", post(post_bonsai_water))
        .route("/api/ws/native", get(ws_native_handler))
}

// ── Challenge / token ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChallengeResponse {
    nonce: String,
    expires_in: u32,
}

async fn get_challenge(AxumState(state): AxumState<State>) -> Json<ChallengeResponse> {
    let nonce = crate::session::new_session_token();
    state.native_challenges.issue(nonce.clone());
    Json(ChallengeResponse { nonce, expires_in: 60 })
}

#[derive(Deserialize)]
struct TokenRequest {
    /// SHA-256 fingerprint in `SHA256:xxxx` format (e.g. from `ssh-keygen -lf`).
    public_key_fingerprint: String,
    /// OpenSSH public key string, e.g. `"ssh-ed25519 AAAA... comment"`.
    public_key: String,
    /// Nonce from `GET /api/native/challenge`.
    nonce: String,
    /// Full PEM text of the SSH signature produced by `ssh-keygen -Y sign -n late.sh`.
    signature_pem: String,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
    expires_at: String,
}

async fn post_token(
    AxumState(state): AxumState<State>,
    Json(body): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    if !state.native_challenges.consume(&body.nonce) {
        return Err(api_error(StatusCode::UNAUTHORIZED, "nonce invalid or expired"));
    }

    // Verify the SSH signature before touching the DB.
    verify_ssh_sig(&body.public_key, &body.public_key_fingerprint, &body.nonce, &body.signature_pem)
        .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "signature verification failed"))?;

    let client = state
        .db
        .get()
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let user = User::find_by_fingerprint(&client, &body.public_key_fingerprint)
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "no user with that fingerprint"))?;

    let token = crate::session::new_session_token();
    let expires_at = Utc::now() + Duration::days(TOKEN_DAYS);
    NativeToken::create(&client, &token, user.id, expires_at)
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to create token"))?;

    Ok(Json(TokenResponse { token, expires_at: expires_at.to_rfc3339() }))
}

// ── REST handlers ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct MeResponse {
    user_id: String,
    username: String,
}

async fn get_me(auth: NativeAuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        user_id: auth.user_id.to_string(),
        username: auth.username,
    })
}

#[derive(Serialize)]
struct RoomInfo {
    id: String,
    name: String,
    slug: String,
    member_count: i64,
}

async fn get_rooms(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<RoomInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let _ = auth;
    let client = state
        .db
        .get()
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let Some(room) = ChatRoom::find_general(&client)
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?
    else {
        return Ok(Json(vec![]));
    };

    let count = online_user_count(&state.active_users) as i64;
    Ok(Json(vec![RoomInfo {
        id: room.id.to_string(),
        name: "General".to_string(),
        slug: room.slug.unwrap_or_else(|| "general".to_string()),
        member_count: count,
    }]))
}

#[derive(Deserialize)]
struct HistoryParams {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct MessageItem {
    id: String,
    user_id: String,
    username: String,
    body: String,
    timestamp: String,
    reactions: Vec<ReactionItem>,
}

#[derive(Serialize)]
struct ReactionItem {
    emoji: String,
    count: i64,
}

async fn get_room_history(
    auth: NativeAuthUser,
    Path(room): Path<String>,
    Query(params): Query<HistoryParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<MessageItem>>, (StatusCode, Json<serde_json::Value>)> {
    let _ = auth;
    let limit = params.limit.unwrap_or(50).min(200);
    let client = state
        .db
        .get()
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let room_id = resolve_room_id(&client, &room).await?;
    let messages = ChatMessage::list_recent(&client, room_id, limit)
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
    let usernames = User::list_usernames_by_ids(&client, &author_ids)
        .await
        .unwrap_or_default();

    let message_ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
    let reactions_map = late_core::models::chat_message_reaction::ChatMessageReaction::list_summaries_for_messages(&client, &message_ids)
        .await
        .unwrap_or_default();

    let items: Vec<MessageItem> = messages
        .iter()
        .rev()
        .map(|m| MessageItem {
            id: m.id.to_string(),
            user_id: m.user_id.to_string(),
            username: usernames.get(&m.user_id).cloned().unwrap_or_default(),
            body: m.body.clone(),
            timestamp: m.created.to_rfc3339(),
            reactions: reactions_map
                .get(&m.id)
                .map(|rs| {
                    rs.iter()
                        .map(|r| ReactionItem {
                            emoji: reaction_emoji(r.kind).to_string(),
                            count: r.count,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();

    Ok(Json(items))
}

#[derive(Serialize)]
struct OnlineUser {
    user_id: String,
    username: String,
}

async fn get_online_users(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<Vec<OnlineUser>> {
    let _ = auth;
    let users = state.active_users.lock().unwrap_or_else(|e| e.into_inner());
    let list = users
        .iter()
        .map(|(id, u)| OnlineUser {
            user_id: id.to_string(),
            username: u.username.clone(),
        })
        .collect();
    Json(list)
}

#[derive(Serialize)]
struct NowPlayingResponse {
    track: String,
    artist: String,
    album: String,
    progress_sec: u64,
    duration_sec: u64,
    volume_pct: u32,
}

async fn get_now_playing(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<NowPlayingResponse> {
    let _ = auth;
    Json(build_now_playing_response(&state))
}

#[derive(Serialize)]
struct NativeStatusResponse {
    connected: bool,
    online_users: usize,
    now_playing: NowPlayingResponse,
    votes: VotesResponse,
}

async fn get_native_status(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<NativeStatusResponse> {
    let _ = auth;
    let online_users = online_user_count(&state.active_users);
    let now_playing = build_now_playing_response(&state);
    let votes = build_votes_response(&state);
    Json(NativeStatusResponse {
        connected: true,
        online_users,
        now_playing,
        votes,
    })
}

#[derive(Deserialize)]
struct VoteBody {
    genre: String,
}

async fn post_vote(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<VoteBody>,
) -> Result<Json<VotesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let genre = Genre::try_from(body.genre.as_str())
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "unknown genre"))?;
    state.vote_service.cast_vote_task(auth.user_id, genre);
    Ok(Json(build_votes_response(&state)))
}

#[derive(Serialize)]
struct BonsaiResponse {
    growth_points: i32,
    is_alive: bool,
    last_watered: Option<String>,
    /// ASCII art lines for the bonsai at its current growth stage.
    art: Vec<String>,
}

async fn get_bonsai(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<BonsaiResponse>, (StatusCode, Json<serde_json::Value>)> {
    let client = state
        .db
        .get()
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let tree = Tree::ensure(&client, auth.user_id, rand_seed())
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let art = tree_ascii(stage_for(tree.is_alive, tree.growth_points), tree.seed, false);
    Ok(Json(BonsaiResponse {
        growth_points: tree.growth_points,
        is_alive: tree.is_alive,
        last_watered: tree.last_watered.map(|d| d.to_string()),
        art,
    }))
}

async fn post_bonsai_water(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<BonsaiResponse>, (StatusCode, Json<serde_json::Value>)> {
    state.bonsai_service.water_task(auth.user_id, false);

    let client = state
        .db
        .get()
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let tree = Tree::ensure(&client, auth.user_id, rand_seed())
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?;

    let art = tree_ascii(stage_for(tree.is_alive, tree.growth_points), tree.seed, false);
    Ok(Json(BonsaiResponse {
        growth_points: tree.growth_points,
        is_alive: tree.is_alive,
        last_watered: tree.last_watered.map(|d| d.to_string()),
        art,
    }))
}

// ── WebSocket ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WsNativeParams {
    token: String,
}

async fn ws_native_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsNativeParams>,
    AxumState(state): AxumState<State>,
) -> impl IntoResponse {
    let Ok(client) = state.db.get().await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(Some((user_id, username))) =
        NativeToken::find_user_by_token(&client, &params.token).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    drop(client);
    ws.on_upgrade(move |socket| handle_native_socket(socket, user_id, username, state))
}

// ── Outbound WS types ─────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
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
    Presence {
        event: String,
        username: String,
    },
    NowPlaying(NowPlayingResponse),
    Votes(VotesResponse),
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

// ── Inbound WS types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WsInAny {
    #[serde(rename = "type")]
    kind: String,
    body: Option<String>,
    genre: Option<String>,
    #[allow(dead_code)]
    room_id: Option<serde_json::Value>,
}

// ── Socket loop ───────────────────────────────────────────────────────────────

async fn handle_native_socket(
    mut socket: WebSocket,
    user_id: Uuid,
    _username: String,
    state: State,
) {
    let Ok(client) = state.db.get().await else {
        return;
    };
    let Some(room) = ChatRoom::find_general(&client).await.ok().flatten() else {
        return;
    };
    let room_id = room.id;

    let messages = ChatMessage::list_recent(&client, room_id, 50)
        .await
        .unwrap_or_default();
    let author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
    let mut usernames = User::list_usernames_by_ids(&client, &author_ids)
        .await
        .unwrap_or_default();
    let msg_ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
    let reactions_map =
        late_core::models::chat_message_reaction::ChatMessageReaction::list_summaries_for_messages(
            &client, &msg_ids,
        )
        .await
        .unwrap_or_default();
    drop(client);

    let msg_items: Vec<MessageItem> = messages
        .iter()
        .rev()
        .map(|m| MessageItem {
            id: m.id.to_string(),
            user_id: m.user_id.to_string(),
            username: usernames.get(&m.user_id).cloned().unwrap_or_default(),
            body: m.body.clone(),
            timestamp: m.created.to_rfc3339(),
            reactions: reactions_map
                .get(&m.id)
                .map(|rs| {
                    rs.iter()
                        .map(|r| ReactionItem {
                            emoji: reaction_emoji(r.kind).to_string(),
                            count: r.count,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();

    let online = {
        let users = state.active_users.lock().unwrap_or_else(|e| e.into_inner());
        users.values().map(|u| WsUser { username: u.username.clone() }).collect()
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
                            // Resolve the slug for DM/non-general rooms if needed.
                            let slug = if active_room_id == room_id { Some("general".to_string()) } else { None };
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
                    "subscribe" => {
                        if let Some(new_id) = payload.room_id.as_ref().and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
                            active_room_id = new_id;
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
                            usernames.insert(message.user_id, name.clone());
                            name
                        } else if let Some(name) = usernames.get(&message.user_id).cloned() {
                            name
                        } else if let Ok(c) = state.db.get().await {
                            let names = User::list_usernames_by_ids(&c, &[message.user_id])
                                .await
                                .unwrap_or_default();
                            let name = names.get(&message.user_id).cloned().unwrap_or_default();
                            usernames.insert(message.user_id, name.clone());
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
                if send_json(&mut socket, &out).await.is_err() {
                    break;
                }
            }
            Ok(()) = np_rx.changed() => {
                let out = WsOut::NowPlaying(build_now_playing_from_value(&np_rx.borrow_and_update()));
                if send_json(&mut socket, &out).await.is_err() {
                    break;
                }
            }
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn send_json<T: Serialize>(socket: &mut WebSocket, val: &T) -> Result<(), ()> {
    let json = serde_json::to_string(val).map_err(|_| ())?;
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| ())
}

/// Resolve a room id from a path segment that is either a UUID or "general".
async fn resolve_room_id(
    client: &deadpool_postgres::Client,
    room: &str,
) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    if room == "general" {
        return ChatRoom::find_general(client)
            .await
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "db error"))?
            .map(|r| r.id)
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "room not found"));
    }
    Uuid::parse_str(room).map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid room id"))
}

fn online_user_count(active_users: &ActiveUsers) -> usize {
    active_users.lock().unwrap_or_else(|e| e.into_inner()).len()
}

#[derive(Serialize)]
pub struct VotesResponse {
    lofi: i64,
    ambient: i64,
    classic: i64,
    jazz: i64,
    next_vote_at: String,
}

fn build_votes_response(state: &State) -> VotesResponse {
    build_votes_response_from_snapshot(&state.vote_service.subscribe_state().borrow().clone())
}

fn build_votes_response_from_snapshot(snap: &VoteSnapshot) -> VotesResponse {
    let next_vote_at =
        (Utc::now() + chrono::Duration::from_std(snap.next_switch_in).unwrap_or_default())
            .to_rfc3339();
    VotesResponse {
        lofi: snap.counts.lofi,
        ambient: snap.counts.ambient,
        classic: snap.counts.classic,
        jazz: snap.counts.jazz,
        next_vote_at,
    }
}

fn build_now_playing_response(state: &State) -> NowPlayingResponse {
    build_now_playing_from_value(&state.now_playing_rx.borrow().clone())
}

fn build_now_playing_from_value(np: &Option<late_core::api_types::NowPlaying>) -> NowPlayingResponse {
    match np {
        Some(np) => {
            let elapsed = np.started_at.elapsed().as_secs();
            let duration = np.track.duration_seconds.unwrap_or(1);
            NowPlayingResponse {
                track: np.track.title.clone(),
                artist: np.track.artist.clone().unwrap_or_default(),
                album: String::new(),
                progress_sec: elapsed,
                duration_sec: duration,
                volume_pct: 0,
            }
        }
        None => NowPlayingResponse {
            track: String::new(),
            artist: String::new(),
            album: String::new(),
            progress_sec: 0,
            duration_sec: 1,
            volume_pct: 0,
        },
    }
}

fn reaction_emoji(kind: i16) -> &'static str {
    match kind {
        1 => "👍",
        2 => "🧡",
        3 => "😂",
        4 => "👀",
        5 => "🔥",
        6 => "🙌",
        7 => "🚀",
        8 => "🤔",
        _ => "?",
    }
}

fn rand_seed() -> i64 {
    use rand_core::{OsRng, RngCore};
    OsRng.next_u64() as i64
}

/// Verify an SSH signature produced by `ssh-keygen -Y sign -n late.sh`.
///
/// Checks that:
///  1. The provided public key parses and its SHA-256 fingerprint matches `expected_fingerprint`.
///  2. The PEM signature is valid over `nonce` bytes with namespace `"late.sh"`.
fn verify_ssh_sig(
    public_key_openssh: &str,
    expected_fingerprint: &str,
    nonce: &str,
    signature_pem: &str,
) -> anyhow::Result<()> {
    use russh::keys::{
        PublicKey,
        ssh_key::{HashAlg, SshSig},
    };

    let pk = PublicKey::from_openssh(public_key_openssh)
        .map_err(|e| anyhow::anyhow!("invalid public key: {e}"))?;

    let computed_fp = pk.fingerprint(HashAlg::Sha256).to_string();
    if computed_fp != expected_fingerprint {
        anyhow::bail!("fingerprint mismatch: expected {expected_fingerprint}, got {computed_fp}");
    }

    let sig = SshSig::from_pem(signature_pem)
        .map_err(|e| anyhow::anyhow!("invalid SSH signature: {e}"))?;

    pk.verify("late.sh", nonce.as_bytes(), &sig)
        .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))?;

    Ok(())
}
