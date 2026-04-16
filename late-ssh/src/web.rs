use axum::{
    extract::{
        Query, State as AxumState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
};
use late_core::MutexRecover;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::{app::chat::svc::ChatEvent, state::State};

// ---------------------------------------------------------------------------
// WebChatRegistry — magic-link tokens for web/mobile chat
// ---------------------------------------------------------------------------

const TOKEN_TTL: Duration = Duration::from_secs(86400); // 24h

struct WebChatSession {
    user_id: Uuid,
    username: String,
    created_at: Instant,
}

#[derive(Clone, Default)]
pub struct WebChatRegistry {
    tokens: Arc<Mutex<HashMap<String, WebChatSession>>>,
}

impl WebChatRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_link(&self, user_id: Uuid, username: String) -> String {
        let token = crate::session::new_session_token();
        let mut tokens = self.tokens.lock_recover();
        tokens.retain(|_, s| s.created_at.elapsed() < TOKEN_TTL);
        tokens.insert(
            token.clone(),
            WebChatSession {
                user_id,
                username,
                created_at: Instant::now(),
            },
        );
        token
    }

    pub fn validate(&self, token: &str) -> Option<(Uuid, String)> {
        let tokens = self.tokens.lock_recover();
        tokens
            .get(token)
            .filter(|s| s.created_at.elapsed() < TOKEN_TTL)
            .map(|s| (s.user_id, s.username.clone()))
    }
}

// ---------------------------------------------------------------------------
// WebSocket chat types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ChatParams {
    pub token: String,
}

#[derive(Deserialize)]
#[serde(tag = "event")]
enum WsChatInbound {
    #[serde(rename = "send")]
    Send { body: String },
    #[serde(rename = "heartbeat")]
    Heartbeat {},
}

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum WsChatOutbound {
    Init {
        messages: Vec<WsChatMsg>,
        username: String,
        user_id: String,
    },
    Message(WsChatMsg),
    MessageDeleted {
        message_id: String,
    },
}

#[derive(Serialize, Clone)]
struct WsChatMsg {
    id: String,
    user_id: String,
    username: String,
    body: String,
    created: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn ws_chat_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<ChatParams>,
    AxumState(state): AxumState<State>,
) -> impl IntoResponse {
    let Some((user_id, username)) = state.web_chat_registry.validate(&params.token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    ws.on_upgrade(move |socket| handle_chat_socket(socket, user_id, username, state))
}

async fn handle_chat_socket(mut socket: WebSocket, user_id: Uuid, username: String, state: State) {
    // Load general room + recent messages
    let Ok(client) = state.db.get().await else {
        return;
    };
    let Some(room) = late_core::models::chat_room::ChatRoom::find_general(&client)
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let room_id = room.id;
    let messages = late_core::models::chat_message::ChatMessage::list_recent(&client, room_id, 100)
        .await
        .unwrap_or_default();
    let author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
    let mut usernames = late_core::models::user::User::list_usernames_by_ids(&client, &author_ids)
        .await
        .unwrap_or_default();
    drop(client);

    // Send init payload (messages are DESC from DB, reverse for chronological)
    let msgs: Vec<WsChatMsg> = messages
        .iter()
        .rev()
        .map(|m| WsChatMsg {
            id: m.id.to_string(),
            user_id: m.user_id.to_string(),
            username: usernames.get(&m.user_id).cloned().unwrap_or_default(),
            body: m.body.clone(),
            created: m.created.to_rfc3339(),
        })
        .collect();
    let init = serde_json::to_string(&WsChatOutbound::Init {
        messages: msgs,
        username: username.clone(),
        user_id: user_id.to_string(),
    })
    .unwrap();
    if socket.send(Message::Text(init.into())).await.is_err() {
        return;
    }

    // Subscribe to chat events and loop
    let mut event_rx = state.chat_service.subscribe_events();

    loop {
        tokio::select! {
            maybe_msg = socket.recv() => {
                let Some(Ok(Message::Text(text))) = maybe_msg else { break };
                let Ok(payload) = serde_json::from_str::<WsChatInbound>(&text) else { continue };
                match payload {
                    WsChatInbound::Send { body } => {
                        state.chat_service.send_message_task(
                            user_id,
                            room_id,
                            Some("general".to_string()),
                            body,
                            Uuid::now_v7(),
                            false,
                        );
                    }
                    WsChatInbound::Heartbeat {} => {}
                }
            }
            Ok(event) = event_rx.recv() => {
                match event {
                    ChatEvent::MessageCreated { message, .. } if message.room_id == room_id => {
                        let author = if let Some(name) = usernames.get(&message.user_id) {
                            name.clone()
                        } else if let Ok(c) = state.db.get().await {
                            let names = late_core::models::user::User::list_usernames_by_ids(
                                &c,
                                &[message.user_id],
                            )
                            .await
                            .unwrap_or_default();
                            let name = names.get(&message.user_id).cloned().unwrap_or_default();
                            usernames.insert(message.user_id, name.clone());
                            name
                        } else {
                            String::new()
                        };
                        let out = WsChatOutbound::Message(WsChatMsg {
                            id: message.id.to_string(),
                            user_id: message.user_id.to_string(),
                            username: author,
                            body: message.body.clone(),
                            created: message.created.to_rfc3339(),
                        });
                        if let Ok(json) = serde_json::to_string(&out)
                            && socket.send(Message::Text(json.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    ChatEvent::MessageDeleted { room_id: rid, message_id, .. } if rid == room_id => {
                        let out = WsChatOutbound::MessageDeleted {
                            message_id: message_id.to_string(),
                        };
                        if let Ok(json) = serde_json::to_string(&out)
                            && socket.send(Message::Text(json.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_link_returns_compact_token() {
        let registry = WebChatRegistry::new();
        let token = registry.create_link(Uuid::now_v7(), "alice".to_string());
        assert_eq!(token.len(), 22); // compact base64url, same as session tokens
    }

    #[test]
    fn validate_returns_user_for_valid_token() {
        let registry = WebChatRegistry::new();
        let uid = Uuid::now_v7();
        let token = registry.create_link(uid, "bob".to_string());
        let result = registry.validate(&token);
        assert_eq!(result, Some((uid, "bob".to_string())));
    }

    #[test]
    fn validate_returns_none_for_unknown_token() {
        let registry = WebChatRegistry::new();
        assert!(registry.validate("nonexistent").is_none());
    }

    #[test]
    fn expired_tokens_are_rejected() {
        let registry = WebChatRegistry::new();
        let uid = Uuid::now_v7();
        let token = "expired-token".to_string();
        {
            let mut tokens = registry.tokens.lock().unwrap();
            tokens.insert(
                token.clone(),
                WebChatSession {
                    user_id: uid,
                    username: "old".to_string(),
                    created_at: Instant::now() - Duration::from_secs(86401),
                },
            );
        }
        assert!(registry.validate(&token).is_none());
    }

    #[test]
    fn create_link_cleans_expired_entries() {
        let registry = WebChatRegistry::new();
        {
            let mut tokens = registry.tokens.lock().unwrap();
            tokens.insert(
                "stale".to_string(),
                WebChatSession {
                    user_id: Uuid::now_v7(),
                    username: "ghost".to_string(),
                    created_at: Instant::now() - Duration::from_secs(86401),
                },
            );
        }
        let _ = registry.create_link(Uuid::now_v7(), "new".to_string());
        let tokens = registry.tokens.lock().unwrap();
        assert!(!tokens.contains_key("stale"));
        assert_eq!(tokens.len(), 1);
    }
}
