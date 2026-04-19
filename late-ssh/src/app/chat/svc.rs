use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use late_core::{
    db::Db,
    models::{
        bonsai::Tree,
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        user::User,
    },
};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};

use crate::app::bonsai::state::stage_for;
use crate::metrics;

const HISTORY_LIMIT: i64 = 1000;
const DELTA_LIMIT: i64 = 256;

#[derive(Clone)]
pub struct ChatService {
    db: Db,
    snapshot_tx: watch::Sender<ChatSnapshot>,
    snapshot_rx: watch::Receiver<ChatSnapshot>,
    evt_tx: broadcast::Sender<ChatEvent>,
    notification_svc: super::notifications::svc::NotificationService,
}

#[derive(Clone, Default)]
pub struct ChatSnapshot {
    pub user_id: Option<Uuid>,
    pub chat_rooms: Vec<(ChatRoom, Vec<ChatMessage>)>,
    pub general_room_id: Option<Uuid>,
    pub usernames: HashMap<Uuid, String>,
    pub countries: HashMap<Uuid, String>,
    pub unread_counts: HashMap<Uuid, i64>,
    pub all_usernames: Vec<String>,
    pub bonsai_glyphs: HashMap<Uuid, String>,
    pub ignored_user_ids: Vec<Uuid>,
}

#[derive(Clone, Debug)]
pub enum ChatEvent {
    MessageCreated {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
    },
    MessageEdited {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
    },
    SendSucceeded {
        user_id: Uuid,
        request_id: Uuid,
    },
    SendFailed {
        user_id: Uuid,
        request_id: Uuid,
        message: String,
    },
    EditSucceeded {
        user_id: Uuid,
        request_id: Uuid,
    },
    EditFailed {
        user_id: Uuid,
        request_id: Uuid,
        message: String,
    },
    DeltaSynced {
        user_id: Uuid,
        room_id: Uuid,
        messages: Vec<ChatMessage>,
    },
    DmOpened {
        user_id: Uuid,
        room_id: Uuid,
    },
    DmFailed {
        user_id: Uuid,
        message: String,
    },
    RoomJoined {
        user_id: Uuid,
        room_id: Uuid,
        slug: String,
    },
    RoomFailed {
        user_id: Uuid,
        message: String,
    },
    RoomLeft {
        user_id: Uuid,
        slug: String,
    },
    LeaveFailed {
        user_id: Uuid,
        message: String,
    },
    RoomCreated {
        user_id: Uuid,
        slug: String,
    },
    RoomCreateFailed {
        user_id: Uuid,
        message: String,
    },
    PermanentRoomCreated {
        user_id: Uuid,
        slug: String,
    },
    PermanentRoomDeleted {
        user_id: Uuid,
        slug: String,
    },
    AdminFailed {
        user_id: Uuid,
        message: String,
    },
    MessageDeleted {
        user_id: Uuid,
        room_id: Uuid,
        message_id: Uuid,
    },
    DeleteFailed {
        user_id: Uuid,
        message: String,
    },
    IgnoreListUpdated {
        user_id: Uuid,
        ignored_user_ids: Vec<Uuid>,
        message: String,
    },
    RoomMembersListed {
        user_id: Uuid,
        title: String,
        members: Vec<String>,
    },
    IgnoreFailed {
        user_id: Uuid,
        message: String,
    },
    RoomMembersListFailed {
        user_id: Uuid,
        message: String,
    },
}

impl ChatService {
    pub fn new(db: Db, notification_svc: super::notifications::svc::NotificationService) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(ChatSnapshot::default());
        let (evt_tx, _) = broadcast::channel(512);

        Self {
            db,
            snapshot_tx,
            snapshot_rx,
            evt_tx,
            notification_svc,
        }
    }
    pub fn subscribe_state(&self) -> watch::Receiver<ChatSnapshot> {
        self.snapshot_rx.clone()
    }
    pub fn subscribe_events(&self) -> broadcast::Receiver<ChatEvent> {
        self.evt_tx.subscribe()
    }

    pub fn publish_snapshot(&self, snapshot: ChatSnapshot) -> Result<()> {
        self.snapshot_tx.send(snapshot)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, selected_room_id = ?selected_room_id))]
    async fn list_chat_rooms(&self, user_id: Uuid, selected_room_id: Option<Uuid>) -> Result<()> {
        let client = &self.db.get().await?;
        let rooms = ChatRoom::list_for_user(client, user_id).await?;
        let unread_counts = ChatRoomMember::unread_counts_for_user(client, user_id).await?;
        let general_room_id = rooms
            .iter()
            .find(|room| room.kind == "general" && room.slug.as_deref() == Some("general"))
            .map(|room| room.id);
        let active_room_id = selected_room_id
            .filter(|selected| rooms.iter().any(|room| room.id == *selected))
            .or_else(|| rooms.first().map(|room| room.id));

        let selected_messages = if let Some(room_id) = active_room_id {
            ChatMessage::list_recent(client, room_id, HISTORY_LIMIT).await?
        } else {
            Vec::new()
        };
        let general_messages = if let Some(room_id) = general_room_id {
            if Some(room_id) == active_room_id {
                selected_messages.clone()
            } else {
                ChatMessage::list_recent(client, room_id, HISTORY_LIMIT).await?
            }
        } else {
            Vec::new()
        };
        // General is the dashboard's permanent room — it must always carry
        // its tail in the snapshot so the dashboard card stays warm even when
        // the chat page has another room selected. Other non-selected rooms
        // ride on broadcasts + a backfill on first open per session.
        let usernames = User::list_all_username_map(client).await?;
        let countries = User::list_all_country_map(client).await?;
        let mut all_usernames: Vec<String> = usernames.values().cloned().collect();
        all_usernames.sort();
        let ignored_user_ids = User::ignored_user_ids(client, user_id).await?;
        let bonsai_glyphs: HashMap<Uuid, String> = Tree::list_all(client)
            .await?
            .into_iter()
            .filter_map(|t| {
                let glyph = stage_for(t.is_alive, t.growth_points).glyph();
                if glyph.is_empty() {
                    None
                } else {
                    Some((t.user_id, glyph.to_string()))
                }
            })
            .collect();

        let rooms = rooms
            .into_iter()
            .map(|chat| {
                let messages = if Some(chat.id) == active_room_id {
                    selected_messages.clone()
                } else if Some(chat.id) == general_room_id {
                    general_messages.clone()
                } else {
                    Vec::new()
                };
                (chat, messages)
            })
            .collect();

        self.publish_snapshot(ChatSnapshot {
            user_id: Some(user_id),
            chat_rooms: rooms,
            general_room_id,
            usernames,
            countries,
            unread_counts,
            all_usernames,
            bonsai_glyphs,
            ignored_user_ids,
        })
    }

    pub fn start_user_refresh_task(
        &self,
        user_id: Uuid,
        room_rx: watch::Receiver<Option<Uuid>>,
    ) -> tokio::task::AbortHandle {
        let service = self.clone();
        let handle = tokio::spawn(
            async move {
                loop {
                    let room_id = *room_rx.borrow();
                    if let Err(e) = service.list_chat_rooms(user_id, room_id).await {
                        late_core::error_span!(
                            "chat_refresh_failed",
                            error = ?e,
                            "chat service refresh failed"
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
            .instrument(info_span!("chat.refresh_loop", user_id = %user_id)),
        );
        handle.abort_handle()
    }

    pub fn list_chats_task(&self, user_id: Uuid, selected_room_id: Option<Uuid>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.list_chat_rooms(user_id, selected_room_id).await {
                    late_core::error_span!("chat_list_failed", error = ?e, "failed to list chats");
                }
            }
            .instrument(info_span!(
                "chat.list_task",
                user_id = %user_id,
                selected_room_id = ?selected_room_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    pub async fn auto_join_public_rooms(&self, user_id: Uuid) -> Result<u64> {
        let client = self.db.get().await?;
        let joined = ChatRoomMember::auto_join_public_rooms(&client, user_id).await?;
        Ok(joined)
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id))]
    async fn mark_room_read(&self, user_id: Uuid, room_id: Uuid) -> Result<()> {
        let client = &self.db.get().await?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }
        ChatRoomMember::mark_read_now(client, room_id, user_id).await?;
        Ok(())
    }

    pub fn mark_room_read_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.mark_room_read(user_id, room_id).await {
                    late_core::error_span!(
                        "chat_mark_read_failed",
                        error = ?e,
                        "failed to mark room read"
                    );
                }
            }
            .instrument(info_span!(
                "chat.mark_room_read_task",
                user_id = %user_id,
                room_id = %room_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id, after_created = %after_created, after_id = %after_id))]
    async fn sync_room_after(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        after_created: DateTime<Utc>,
        after_id: Uuid,
    ) -> Result<()> {
        let client = &self.db.get().await?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        let messages =
            ChatMessage::list_after(client, room_id, after_created, after_id, DELTA_LIMIT).await?;
        if !messages.is_empty() {
            let _ = self.evt_tx.send(ChatEvent::DeltaSynced {
                user_id,
                room_id,
                messages,
            });
        }
        Ok(())
    }

    pub fn sync_room_after_task(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        after_created: DateTime<Utc>,
        after_id: Uuid,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .sync_room_after(user_id, room_id, after_created, after_id)
                    .await
                {
                    late_core::error_span!(
                        "chat_sync_failed",
                        error = ?e,
                        "failed to sync chat room delta"
                    );
                }
            }
            .instrument(info_span!(
                "chat.sync_room_after_task",
                user_id = %user_id,
                room_id = %room_id,
                after_created = %after_created,
                after_id = %after_id
            )),
        );
    }

    pub fn send_message_task(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        room_slug: Option<String>,
        body: String,
        request_id: Uuid,
        is_admin: bool,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .send_message(user_id, room_id, room_slug, body, is_admin)
                    .await
                {
                    Err(e) => {
                        let message = if e.to_string().contains("not a member") {
                            "You are not a member of this room."
                        } else if e.to_string().contains("admin-only") {
                            "Only admins can post in #announcements."
                        } else {
                            "Could not send message. Please try again."
                        };
                        let _ = service.evt_tx.send(ChatEvent::SendFailed {
                            user_id,
                            request_id,
                            message: message.to_string(),
                        });
                        late_core::error_span!(
                            "chat_send_failed",
                            error = ?e,
                            "failed to send message"
                        );
                    }
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::SendSucceeded {
                            user_id,
                            request_id,
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.send_message_task",
                user_id = %user_id,
                room_id = %room_id,
                request_id = %request_id
            )),
        );
    }

    #[tracing::instrument(skip(self, body), fields(user_id = %user_id, room_id = %room_id, body_len = body.len()))]
    async fn send_message(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        room_slug: Option<String>,
        body: String,
        is_admin: bool,
    ) -> Result<()> {
        let body = body.trim_start_matches('\n').trim_end();
        if body.is_empty() {
            return Ok(());
        }

        if room_slug.as_deref() == Some("announcements") && !is_admin {
            anyhow::bail!("announcements is admin-only");
        }

        let client = &self.db.get().await?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        let message = ChatMessageParams {
            room_id,
            user_id,
            body: body.to_string(),
        };
        let chat = ChatMessage::create(client, message).await?;
        ChatRoom::touch_updated(client, room_id).await?;
        ChatRoomMember::mark_read_now(client, room_id, user_id).await?;
        let target_user_ids = ChatRoom::get_target_user_ids(client, room_id).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageCreated {
            message: chat.clone(),
            target_user_ids,
        });
        metrics::record_chat_message_sent();
        self.notification_svc
            .create_mentions_task(user_id, chat.id, room_id, body.to_string());
        tracing::info!(chat_id = %chat.id, "message sent");
        Ok(())
    }

    pub fn edit_message_task(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        new_body: String,
        request_id: Uuid,
        is_admin: bool,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .edit_message(user_id, message_id, new_body, is_admin)
                    .await
                {
                    Err(e) => {
                        let message = if e.to_string().contains("Cannot edit") {
                            "You can only edit your own messages."
                        } else if e.to_string().contains("empty") {
                            "Edited message cannot be empty."
                        } else {
                            "Could not edit message. Please try again."
                        };
                        let _ = service.evt_tx.send(ChatEvent::EditFailed {
                            user_id,
                            request_id,
                            message: message.to_string(),
                        });
                    }
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::EditSucceeded {
                            user_id,
                            request_id,
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.edit_message_task",
                user_id = %user_id,
                message_id = %message_id,
                request_id = %request_id
            )),
        );
    }

    #[tracing::instrument(skip(self, new_body), fields(user_id = %user_id, message_id = %message_id, body_len = new_body.len()))]
    async fn edit_message(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        new_body: String,
        is_admin: bool,
    ) -> Result<()> {
        let new_body = new_body.trim_start_matches('\n').trim_end();
        if new_body.is_empty() {
            anyhow::bail!("edited body is empty");
        }

        let client = &self.db.get().await?;
        let existing = ChatMessage::get(client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("message not found"))?;
        if existing.user_id != user_id && !is_admin {
            anyhow::bail!("cannot edit this message");
        }

        let params = ChatMessageParams {
            room_id: existing.room_id,
            user_id: existing.user_id,
            body: new_body.to_string(),
        };
        let updated = ChatMessage::update(client, message_id, params).await?;
        let target_user_ids = ChatRoom::get_target_user_ids(client, existing.room_id).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageEdited {
            message: updated,
            target_user_ids,
        });
        metrics::record_chat_message_edited();
        Ok(())
    }

    pub fn start_dm_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span = info_span!("chat.start_dm_task", user_id = %user_id, target = %target_username);
        tokio::spawn(
            async move {
                match service.open_dm(user_id, &target_username).await {
                    Ok(room_id) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::DmOpened { user_id, room_id });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::DmFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn open_dm(&self, user_id: Uuid, target_username: &str) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot DM yourself");
        }
        let room = ChatRoom::get_or_create_dm(client, user_id, target.id).await?;
        ChatRoomMember::join(client, room.id, user_id).await?;
        ChatRoomMember::join(client, room.id, target.id).await?;
        Ok(room.id)
    }

    pub fn list_room_members_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        let span = info_span!(
            "chat.list_room_members_task",
            user_id = %user_id,
            room_id = %room_id
        );
        tokio::spawn(
            async move {
                let event = match service.list_room_members(user_id, room_id).await {
                    Ok((title, members)) => ChatEvent::RoomMembersListed {
                        user_id,
                        title,
                        members,
                    },
                    Err(e) => ChatEvent::RoomMembersListFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_room_members(
        &self,
        user_id: Uuid,
        room_id: Uuid,
    ) -> Result<(String, Vec<String>)> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }

        let user_ids = ChatRoomMember::list_user_ids(client, room_id).await?;
        let usernames = User::list_usernames_by_ids(client, &user_ids).await?;
        let members = user_ids
            .into_iter()
            .map(|id| {
                usernames
                    .get(&id)
                    .map(|username| format!("@{username}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(id)))
            })
            .collect();
        let title = room
            .slug
            .as_deref()
            .map(|slug| format!("#{slug} Members"))
            .unwrap_or_else(|| "Room Members".to_string());

        Ok((title, members))
    }

    pub fn ignore_user_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span =
            info_span!("chat.ignore_user_task", user_id = %user_id, target = %target_username);
        tokio::spawn(
            async move {
                let event = match service.ignore_user(user_id, &target_username).await {
                    Ok((ignored_user_ids, message)) => ChatEvent::IgnoreListUpdated {
                        user_id,
                        ignored_user_ids,
                        message,
                    },
                    Err(e) => ChatEvent::IgnoreFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn ignore_user(
        &self,
        user_id: Uuid,
        target_username: &str,
    ) -> Result<(Vec<Uuid>, String)> {
        let client = &self.db.get().await?;
        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot ignore yourself");
        }
        let (changed, ids) = User::add_ignored_user_id(client, user_id, target.id).await?;
        if !changed {
            anyhow::bail!("@{} is already ignored", target.username);
        }
        Ok((ids, format!("Ignored @{}", target.username)))
    }

    pub fn unignore_user_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span =
            info_span!("chat.unignore_user_task", user_id = %user_id, target = %target_username);
        tokio::spawn(
            async move {
                let event = match service.unignore_user(user_id, &target_username).await {
                    Ok((ignored_user_ids, message)) => ChatEvent::IgnoreListUpdated {
                        user_id,
                        ignored_user_ids,
                        message,
                    },
                    Err(e) => ChatEvent::IgnoreFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn unignore_user(
        &self,
        user_id: Uuid,
        target_username: &str,
    ) -> Result<(Vec<Uuid>, String)> {
        let client = &self.db.get().await?;
        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot unignore yourself");
        }
        let (changed, ids) = User::remove_ignored_user_id(client, user_id, target.id).await?;
        if !changed {
            anyhow::bail!("@{} is not ignored", target.username);
        }
        Ok((ids, format!("Unignored @{}", target.username)))
    }

    pub fn join_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.join_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.join_room(user_id, &slug).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomJoined {
                            user_id,
                            room_id,
                            slug,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn join_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get_or_create_room(client, slug).await?;
        ChatRoomMember::join(client, room.id, user_id).await?;
        Ok(room.id)
    }

    pub fn leave_room_task(&self, user_id: Uuid, room_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.leave_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.leave_room(user_id, room_id).await {
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomLeft { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::LeaveFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn leave_room(&self, user_id: Uuid, room_id: Uuid) -> Result<()> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.permanent {
            let name = room.slug.as_deref().unwrap_or("this room");
            anyhow::bail!("Cannot leave #{name} (permanent room)");
        }
        ChatRoomMember::leave(client, room_id, user_id).await?;
        Ok(())
    }

    pub fn create_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_room(&slug).await {
                    Ok(_) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::RoomCreated { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreateFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn create_room(&self, slug: &str) -> Result<()> {
        let client = &self.db.get().await?;
        let room = ChatRoom::ensure_auto_join(client, slug).await?;
        let added = ChatRoom::add_all_users(client, room.id).await?;
        tracing::info!(slug = %slug, room_id = %room.id, users_added = added, "room created");
        Ok(())
    }

    pub fn create_permanent_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_permanent_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_permanent_room(&slug).await {
                    Ok(_) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::PermanentRoomCreated { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::AdminFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn create_permanent_room(&self, slug: &str) -> Result<()> {
        let client = &self.db.get().await?;
        let room = ChatRoom::ensure_permanent(client, slug).await?;
        let added = ChatRoom::add_all_users(client, room.id).await?;
        tracing::info!(slug = %slug, room_id = %room.id, users_added = added, "permanent room created");
        Ok(())
    }

    pub fn delete_permanent_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.delete_permanent_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.delete_permanent_room(&slug).await {
                    Ok(_) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::PermanentRoomDeleted { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::AdminFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn delete_permanent_room(&self, slug: &str) -> Result<()> {
        let client = &self.db.get().await?;
        let count = ChatRoom::delete_permanent(client, slug).await?;
        if count == 0 {
            anyhow::bail!("Permanent room #{slug} not found");
        }
        tracing::info!(slug = %slug, "permanent room deleted");
        Ok(())
    }

    pub fn delete_message_task(&self, user_id: Uuid, message_id: Uuid, is_admin: bool) {
        let service = self.clone();
        let span = info_span!("chat.delete_message", user_id = %user_id, message_id = %message_id);
        tokio::spawn(
            async move {
                match service.delete_message(user_id, message_id, is_admin).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::MessageDeleted {
                            user_id,
                            room_id,
                            message_id,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::DeleteFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn delete_message(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        is_admin: bool,
    ) -> Result<Uuid> {
        let client = &self.db.get().await?;
        // Look up the message to get room_id
        let msg = ChatMessage::get(client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Message not found"))?;
        let count = if is_admin {
            ChatMessage::delete_by_admin(client, message_id).await?
        } else {
            ChatMessage::delete_by_author(client, message_id, user_id).await?
        };
        if count == 0 {
            anyhow::bail!("Cannot delete this message");
        }
        tracing::info!(message_id = %message_id, "message deleted");
        Ok(msg.room_id)
    }
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}
