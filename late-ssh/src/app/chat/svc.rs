use anyhow::Result;
use chrono::{DateTime, Utc};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

use late_core::{
    MutexRecover,
    db::Db,
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_reaction::{
            ChatMessageReaction, ChatMessageReactionOwners, ChatMessageReactionSummary,
        },
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        room_ban::RoomBan,
        user::User,
    },
};
use serde_json::json;
use tokio::sync::{Semaphore, broadcast, mpsc, watch};
use tracing::{Instrument, info_span};

use crate::app::bonsai::state::stage_for;
use crate::authz::{Caps, Permissions, Tier};
use crate::metrics;
use crate::moderation::event::ModerationEvent;
use crate::moderation::service::{
    ModAuditRecord, ModerationService, ensure_message_permission, record_mod_audit,
    target_tier_for_user_id,
};
use crate::moderation::session_effects::ModerationSessionEffects;
use crate::session::SessionRegistry;
use crate::state::ActiveUsers;

const HISTORY_LIMIT: i64 = 500;
const DELTA_LIMIT: i64 = 256;
const PINNED_MESSAGES_LIMIT: i64 = 100;
const CHAT_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const USERNAME_DIRECTORY_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct ChatService {
    db: Db,
    username_tx: watch::Sender<Arc<Vec<String>>>,
    username_rx: watch::Receiver<Arc<Vec<String>>>,
    evt_tx: broadcast::Sender<ChatEvent>,
    moderation_event_tx: broadcast::Sender<ModerationEvent>,
    notification_svc: super::notifications::svc::NotificationService,
    active_users: Option<ActiveUsers>,
    session_registry: Option<SessionRegistry>,
    force_admin: bool,
    username_refresh_started: Arc<AtomicBool>,
    refresh_sessions: Arc<Mutex<HashMap<Uuid, ChatRefreshSession>>>,
    refresh_scheduler_started: Arc<AtomicBool>,
    refresh_signal_tx: mpsc::UnboundedSender<Uuid>,
    refresh_signal_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<Uuid>>>>,
    read_permits: Arc<Semaphore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverRoomItem {
    pub room_id: Uuid,
    pub slug: String,
    pub member_count: i64,
    pub message_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
}

pub struct SendMessageTask {
    pub user_id: Uuid,
    pub room_id: Uuid,
    pub room_slug: Option<String>,
    pub body: String,
    pub reply_to_message_id: Option<Uuid>,
    pub request_id: Uuid,
    pub is_admin: bool,
}

#[derive(Clone)]
struct ChatRefreshSession {
    user_id: Uuid,
    snapshot_tx: watch::Sender<ChatSnapshot>,
}

struct ChatRefreshSessionGuard {
    sessions: Arc<Mutex<HashMap<Uuid, ChatRefreshSession>>>,
    session_id: Uuid,
}

impl Drop for ChatRefreshSessionGuard {
    fn drop(&mut self) {
        self.sessions.lock_recover().remove(&self.session_id);
    }
}

#[derive(Clone, Default)]
pub struct ChatSnapshot {
    pub user_id: Option<Uuid>,
    pub chat_rooms: Vec<(ChatRoom, Vec<ChatMessage>)>,
    pub message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub general_room_id: Option<Uuid>,
    pub usernames: HashMap<Uuid, String>,
    pub countries: HashMap<Uuid, String>,
    pub unread_counts: HashMap<Uuid, i64>,
    pub bonsai_glyphs: HashMap<Uuid, String>,
    pub ignored_user_ids: Vec<Uuid>,
}

#[derive(Clone, Debug)]
pub enum ChatEvent {
    MessageCreated {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
        author_username: Option<String>,
        author_bonsai_glyph: Option<String>,
    },
    MessageEdited {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
        author_username: Option<String>,
        author_bonsai_glyph: Option<String>,
    },
    RoomTailLoaded {
        user_id: Uuid,
        room_id: Uuid,
        messages: Vec<ChatMessage>,
        message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
        usernames: HashMap<Uuid, String>,
        bonsai_glyphs: HashMap<Uuid, String>,
    },
    RoomTailLoadFailed {
        user_id: Uuid,
        room_id: Uuid,
    },
    DiscoverRoomsLoaded {
        user_id: Uuid,
        rooms: Vec<DiscoverRoomItem>,
    },
    DiscoverRoomsFailed {
        user_id: Uuid,
        message: String,
    },
    MessageReactionsUpdated {
        room_id: Uuid,
        message_id: Uuid,
        reactions: Vec<ChatMessageReactionSummary>,
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
    GameRoomJoined {
        user_id: Uuid,
        room_id: Uuid,
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
        room_id: Uuid,
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
    RoomFilled {
        user_id: Uuid,
        slug: String,
        users_added: u64,
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
    PublicRoomsListed {
        user_id: Uuid,
        title: String,
        rooms: Vec<String>,
    },
    InviteSucceeded {
        user_id: Uuid,
        room_id: Uuid,
        room_slug: String,
        username: String,
    },
    IgnoreFailed {
        user_id: Uuid,
        message: String,
    },
    RoomMembersListFailed {
        user_id: Uuid,
        message: String,
    },
    ReactionOwnersListed {
        user_id: Uuid,
        message_id: Uuid,
        owners: Vec<ChatMessageReactionOwners>,
        usernames: HashMap<Uuid, String>,
    },
    ReactionOwnersListFailed {
        user_id: Uuid,
        message: String,
    },
    PublicRoomsListFailed {
        user_id: Uuid,
        message: String,
    },
    InviteFailed {
        user_id: Uuid,
        message: String,
    },
    ModCommandOutput {
        user_id: Uuid,
        request_id: Uuid,
        lines: Vec<String>,
        success: bool,
    },
}

impl ChatService {
    pub fn new(db: Db, notification_svc: super::notifications::svc::NotificationService) -> Self {
        let (username_tx, username_rx) = watch::channel(Arc::new(Vec::new()));
        let (evt_tx, _) = broadcast::channel(512);
        let (moderation_event_tx, _) = broadcast::channel(256);
        let (refresh_signal_tx, refresh_signal_rx) = mpsc::unbounded_channel();

        Self {
            db,
            username_tx,
            username_rx,
            evt_tx,
            moderation_event_tx,
            notification_svc,
            active_users: None,
            session_registry: None,
            force_admin: false,
            username_refresh_started: Arc::new(AtomicBool::new(false)),
            refresh_sessions: Arc::new(Mutex::new(HashMap::new())),
            refresh_scheduler_started: Arc::new(AtomicBool::new(false)),
            refresh_signal_tx,
            refresh_signal_rx: Arc::new(Mutex::new(Some(refresh_signal_rx))),
            read_permits: Arc::new(Semaphore::new(8)),
        }
    }

    pub fn new_with_active_users(
        db: Db,
        notification_svc: super::notifications::svc::NotificationService,
        active_users: ActiveUsers,
    ) -> Self {
        let mut service = Self::new(db, notification_svc);
        service.active_users = Some(active_users);
        service
    }

    pub fn with_session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.session_registry = Some(session_registry);
        self
    }

    pub fn with_force_admin(mut self, force_admin: bool) -> Self {
        self.force_admin = force_admin;
        self
    }

    pub fn subscribe_usernames(&self) -> watch::Receiver<Arc<Vec<String>>> {
        self.ensure_username_refresh_task();
        self.username_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ChatEvent> {
        self.evt_tx.subscribe()
    }

    pub fn subscribe_moderation_events(&self) -> broadcast::Receiver<ModerationEvent> {
        self.moderation_event_tx.subscribe()
    }

    fn moderation_session_effects(&self) -> ModerationSessionEffects {
        ModerationSessionEffects::new(self.active_users.clone(), self.session_registry.clone())
    }

    pub fn run_mod_command_task(
        &self,
        user_id: Uuid,
        permissions: Permissions,
        request_id: Uuid,
        command: String,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.run_mod_command_task",
            user_id = %user_id,
            request_id = %request_id
        );
        tokio::spawn(
            async move {
                let moderation = service.moderation_service();
                let (success, lines) =
                    match moderation.run_command(user_id, permissions, &command).await {
                        Ok(lines) => (true, lines),
                        Err(e) => (false, vec![format!("error: {e}")]),
                    };
                let _ = service.evt_tx.send(ChatEvent::ModCommandOutput {
                    user_id,
                    request_id,
                    lines,
                    success,
                });
            }
            .instrument(span),
        );
    }

    fn moderation_service(&self) -> ModerationService {
        ModerationService::new(
            self.db.clone(),
            self.moderation_session_effects(),
            self.moderation_event_tx.clone(),
            self.force_admin,
        )
    }

    async fn refresh_username_directory(&self) -> Result<()> {
        let client = self.db.get().await?;
        let usernames = User::list_all_usernames(&client).await?;
        let _ = self.username_tx.send(Arc::new(usernames));
        Ok(())
    }

    fn ensure_username_refresh_task(&self) {
        if self
            .username_refresh_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.refresh_username_directory().await {
                    late_core::error_span!(
                        "chat_username_directory_refresh_failed",
                        error = ?e,
                        "chat username directory refresh failed"
                    );
                }

                let mut interval = tokio::time::interval(USERNAME_DIRECTORY_TTL);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;

                loop {
                    interval.tick().await;
                    if let Err(e) = service.refresh_username_directory().await {
                        late_core::error_span!(
                            "chat_username_directory_refresh_failed",
                            error = ?e,
                            "chat username directory refresh failed"
                        );
                    }
                }
            }
            .instrument(info_span!("chat.username_directory_refresh_loop")),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    async fn build_chat_snapshot(&self, user_id: Uuid) -> Result<ChatSnapshot> {
        let _permit = self.read_permits.acquire().await?;
        let client = self.db.get().await?;
        let rooms = ChatRoom::list_for_user(&client, user_id).await?;
        let unread_counts = ChatRoomMember::unread_counts_for_user(&client, user_id).await?;
        let general_room_id = rooms
            .iter()
            .find(|room| room.kind == "general" && room.slug.as_deref() == Some("general"))
            .map(|room| room.id);

        let mut visible_user_ids = vec![user_id];
        for room in &rooms {
            if room.kind == "dm" {
                if let Some(id) = room.dm_user_a {
                    visible_user_ids.push(id);
                }
                if let Some(id) = room.dm_user_b {
                    visible_user_ids.push(id);
                }
            }
        }
        visible_user_ids.sort();
        visible_user_ids.dedup();
        let (usernames, bonsai_glyphs) =
            Self::load_chat_author_metadata(&client, &visible_user_ids).await?;
        let ignored_user_ids = User::ignored_user_ids(&client, user_id).await?;

        let rooms = rooms.into_iter().map(|chat| (chat, Vec::new())).collect();

        Ok(ChatSnapshot {
            user_id: Some(user_id),
            chat_rooms: rooms,
            message_reactions: HashMap::new(),
            general_room_id,
            usernames,
            countries: HashMap::new(),
            unread_counts,
            bonsai_glyphs,
            ignored_user_ids,
        })
    }

    async fn load_chat_author_metadata(
        client: &tokio_postgres::Client,
        user_ids: &[Uuid],
    ) -> Result<(HashMap<Uuid, String>, HashMap<Uuid, String>)> {
        if user_ids.is_empty() {
            return Ok((HashMap::new(), HashMap::new()));
        }

        let rows = client
            .query(
                "SELECT u.id,
                        u.username,
                        t.is_alive,
                        t.growth_points
                 FROM users u
                 LEFT JOIN bonsai_trees t ON t.user_id = u.id
                 WHERE u.id = ANY($1)",
                &[&user_ids],
            )
            .await?;

        let mut usernames = HashMap::with_capacity(rows.len());
        let mut bonsai_glyphs = HashMap::new();
        for row in rows {
            let user_id: Uuid = row.get("id");
            let username: String = row.get("username");
            if !username.trim().is_empty() {
                usernames.insert(user_id, username);
            }

            let is_alive: Option<bool> = row.get("is_alive");
            let growth_points: Option<i32> = row.get("growth_points");
            if let (Some(is_alive), Some(growth_points)) = (is_alive, growth_points) {
                let glyph = stage_for(is_alive, growth_points).glyph();
                if !glyph.is_empty() {
                    bonsai_glyphs.insert(user_id, glyph.to_string());
                }
            }
        }

        Ok((usernames, bonsai_glyphs))
    }

    async fn list_all_discover_rooms(
        client: &tokio_postgres::Client,
    ) -> Result<Vec<DiscoverRoomItem>> {
        let rows = client
            .query(
                "SELECT r.id,
                        r.slug,
                        COUNT(DISTINCT m.user_id)::bigint AS member_count,
                        COUNT(DISTINCT msg.id)::bigint AS message_count,
                        MAX(msg.created) AS last_message_at
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 LEFT JOIN chat_messages msg ON msg.room_id = r.id
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                 GROUP BY r.id, r.slug
                 ORDER BY
                    COALESCE(MAX(msg.created), r.created) DESC,
                    message_count DESC,
                    member_count DESC,
                    r.slug ASC",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let slug: Option<String> = row.get("slug");
                slug.map(|slug| DiscoverRoomItem {
                    room_id: row.get("id"),
                    slug,
                    member_count: row.get("member_count"),
                    message_count: row.get("message_count"),
                    last_message_at: row.get("last_message_at"),
                })
            })
            .collect())
    }

    fn ensure_refresh_scheduler(&self) {
        if self
            .refresh_scheduler_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let service = self.clone();
        let mut refresh_signal_rx = self
            .refresh_signal_rx
            .lock_recover()
            .take()
            .expect("chat refresh scheduler receiver missing");
        tokio::spawn(
            async move {
                let mut interval = tokio::time::interval(CHAT_REFRESH_INTERVAL);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            service.refresh_registered_sessions().await;
                        }
                        Some(session_id) = refresh_signal_rx.recv() => {
                            service.refresh_registered_session(session_id).await;
                        }
                    }
                }
            }
            .instrument(info_span!("chat.refresh_scheduler")),
        );
    }

    async fn refresh_registered_sessions(&self) {
        let sessions: Vec<ChatRefreshSession> = self
            .refresh_sessions
            .lock_recover()
            .values()
            .cloned()
            .collect();

        for session in sessions {
            self.refresh_session(session).await;
        }
    }

    async fn refresh_registered_session(&self, session_id: Uuid) {
        let session = self
            .refresh_sessions
            .lock_recover()
            .get(&session_id)
            .cloned();
        if let Some(session) = session {
            self.refresh_session(session).await;
        }
    }

    async fn refresh_session(&self, session: ChatRefreshSession) {
        match self.build_chat_snapshot(session.user_id).await {
            Ok(snapshot) => {
                let _ = session.snapshot_tx.send(snapshot);
            }
            Err(e) => {
                late_core::error_span!(
                    "chat_refresh_failed",
                    user_id = %session.user_id,
                    error = ?e,
                    "chat service refresh failed"
                );
            }
        }
    }

    pub fn start_user_refresh_task(
        &self,
        user_id: Uuid,
        room_rx: watch::Receiver<Option<Uuid>>,
    ) -> (
        watch::Receiver<ChatSnapshot>,
        mpsc::UnboundedSender<()>,
        tokio::task::AbortHandle,
    ) {
        self.ensure_refresh_scheduler();

        let session_id = Uuid::now_v7();
        let (snapshot_tx, snapshot_rx) = watch::channel(ChatSnapshot::default());
        let (force_refresh_tx, mut force_refresh_rx) = mpsc::unbounded_channel();
        let initial_room_id = *room_rx.borrow();
        self.refresh_sessions.lock_recover().insert(
            session_id,
            ChatRefreshSession {
                user_id,
                snapshot_tx,
            },
        );
        let _ = self.refresh_signal_tx.send(session_id);

        let sessions = self.refresh_sessions.clone();
        let refresh_signal_tx = self.refresh_signal_tx.clone();
        let mut room_rx = room_rx;
        let handle = tokio::spawn(
            async move {
                let _guard = ChatRefreshSessionGuard {
                    sessions: sessions.clone(),
                    session_id,
                };
                let mut last_selected_room_id = initial_room_id;

                loop {
                    tokio::select! {
                        changed = room_rx.changed() => {
                            if changed.is_err() {
                                break;
                            }

                            let selected_room_id = *room_rx.borrow_and_update();
                            if selected_room_id == last_selected_room_id {
                                continue;
                            }
                            last_selected_room_id = selected_room_id;
                            let _ = refresh_signal_tx.send(session_id);
                        }
                        Some(()) = force_refresh_rx.recv() => {
                            let _ = refresh_signal_tx.send(session_id);
                        }
                    }
                }
            }
            .instrument(info_span!("chat.refresh_registration", user_id = %user_id, session_id = %session_id)),
        );
        (snapshot_rx, force_refresh_tx, handle.abort_handle())
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    pub async fn auto_join_public_rooms(&self, user_id: Uuid) -> Result<u64> {
        let client = self.db.get().await?;
        let joined = ChatRoomMember::auto_join_public_rooms(&client, user_id).await?;
        Ok(joined)
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id))]
    async fn mark_room_read(&self, user_id: Uuid, room_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let is_member = ChatRoomMember::is_member(&client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }
        ChatRoomMember::mark_read_now(&client, room_id, user_id).await?;
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
        let client = self.db.get().await?;
        let is_member = ChatRoomMember::is_member(&client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        let messages =
            ChatMessage::list_after(&client, room_id, after_created, after_id, DELTA_LIMIT).await?;
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

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id))]
    async fn load_room_tail(&self, user_id: Uuid, room_id: Uuid) -> Result<()> {
        let _permit = self.read_permits.acquire().await?;
        let client = self.db.get().await?;
        let is_member = ChatRoomMember::is_member(&client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        let messages = ChatMessage::list_recent(&client, room_id, HISTORY_LIMIT).await?;
        let message_ids: Vec<Uuid> = messages.iter().map(|message| message.id).collect();
        let author_ids: Vec<Uuid> = messages.iter().map(|message| message.user_id).collect();
        let message_reactions =
            ChatMessageReaction::list_summaries_for_messages(&client, &message_ids).await?;
        let (usernames, bonsai_glyphs) =
            Self::load_chat_author_metadata(&client, &author_ids).await?;

        let _ = self.evt_tx.send(ChatEvent::RoomTailLoaded {
            user_id,
            room_id,
            messages,
            message_reactions,
            usernames,
            bonsai_glyphs,
        });
        Ok(())
    }

    pub fn load_room_tail_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.load_room_tail(user_id, room_id).await {
                    let _ = service
                        .evt_tx
                        .send(ChatEvent::RoomTailLoadFailed { user_id, room_id });
                    late_core::error_span!(
                        "chat_load_room_tail_failed",
                        error = ?e,
                        "failed to load chat room tail"
                    );
                }
            }
            .instrument(info_span!(
                "chat.load_room_tail_task",
                user_id = %user_id,
                room_id = %room_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    async fn list_discover_rooms(&self, user_id: Uuid) -> Result<Vec<DiscoverRoomItem>> {
        let _permit = self.read_permits.acquire().await?;
        let client = self.db.get().await?;
        let joined_ids: HashSet<Uuid> = ChatRoom::list_for_user(&client, user_id)
            .await?
            .into_iter()
            .map(|room| room.id)
            .collect();
        Ok(Self::list_all_discover_rooms(&client)
            .await?
            .into_iter()
            .filter(|room| !joined_ids.contains(&room.room_id))
            .collect())
    }

    pub fn list_discover_rooms_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service.list_discover_rooms(user_id).await {
                    Ok(rooms) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::DiscoverRoomsLoaded { user_id, rooms });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::DiscoverRoomsFailed {
                            user_id,
                            message: "Could not load public rooms.".to_string(),
                        });
                        late_core::error_span!(
                            "chat_discover_rooms_failed",
                            error = ?e,
                            "failed to list discover rooms"
                        );
                    }
                }
            }
            .instrument(info_span!("chat.list_discover_rooms_task", user_id = %user_id)),
        );
    }

    pub fn load_pinned_messages_task(&self, pinned_tx: watch::Sender<Vec<ChatMessage>>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let _permit = service.read_permits.acquire().await?;
                    let client = service.db.get().await?;
                    ChatMessage::list_pinned(&client, PINNED_MESSAGES_LIMIT).await
                }
                .await;
                match result {
                    Ok(messages) => {
                        let _ = pinned_tx.send(messages);
                    }
                    Err(e) => late_core::error_span!(
                        "chat_load_pinned_messages_failed",
                        error = ?e,
                        "failed to load pinned chat messages"
                    ),
                }
            }
            .instrument(info_span!("chat.load_pinned_messages_task")),
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
        self.send_message_with_reply_task(SendMessageTask {
            user_id,
            room_id,
            room_slug,
            body,
            reply_to_message_id: None,
            request_id,
            is_admin,
        });
    }

    pub fn send_message_with_reply_task(&self, task: SendMessageTask) {
        let SendMessageTask {
            user_id,
            room_id,
            room_slug,
            body,
            reply_to_message_id,
            request_id,
            is_admin,
        } = task;
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .send_message(
                        user_id,
                        room_id,
                        room_slug,
                        body,
                        reply_to_message_id,
                        is_admin,
                    )
                    .await
                {
                    Err(e) => {
                        let message = if e.to_string().contains("not a member") {
                            "You are not a member of this room."
                        } else if e.to_string().contains("banned from this room") {
                            "You are banned from this room."
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
        reply_to_message_id: Option<Uuid>,
        is_admin: bool,
    ) -> Result<()> {
        let body = body.trim_start_matches('\n').trim_end();
        if body.is_empty() {
            return Ok(());
        }

        if room_slug.as_deref() == Some("announcements") && !is_admin {
            anyhow::bail!("announcements is admin-only");
        }

        let client = self.db.get().await?;
        let is_member = ChatRoomMember::is_member(&client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }
        if RoomBan::is_active_for_room_and_user(&client, room_id, user_id).await? {
            anyhow::bail!("user is banned from this room");
        }
        if let Some(reply_to_message_id) = reply_to_message_id {
            let reply_target = ChatMessage::get(&client, reply_to_message_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("reply target not found"))?;
            if reply_target.room_id != room_id {
                anyhow::bail!("reply target is not in this room");
            }
        }
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("room not found"))?;
        if room.kind == "dm" {
            let user_a = room
                .dm_user_a
                .ok_or_else(|| anyhow::anyhow!("dm room is missing first participant"))?;
            let user_b = room
                .dm_user_b
                .ok_or_else(|| anyhow::anyhow!("dm room is missing second participant"))?;
            ChatRoomMember::join(&client, room_id, user_a).await?;
            ChatRoomMember::join(&client, room_id, user_b).await?;
        }

        let message = ChatMessageParams {
            room_id,
            user_id,
            body: body.to_string(),
        };
        let chat = ChatMessage::create_with_reply_to(&client, message, reply_to_message_id).await?;
        ChatRoom::touch_updated(&client, room_id).await?;
        ChatRoomMember::mark_read_now(&client, room_id, user_id).await?;
        let target_user_ids = ChatRoom::get_target_user_ids(&client, room_id).await?;
        let (mut usernames, mut bonsai_glyphs) =
            Self::load_chat_author_metadata(&client, &[user_id]).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageCreated {
            message: chat.clone(),
            target_user_ids,
            author_username: usernames.remove(&user_id),
            author_bonsai_glyph: bonsai_glyphs.remove(&user_id),
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
        permissions: Permissions,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .edit_message(user_id, message_id, new_body, permissions)
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
        permissions: Permissions,
    ) -> Result<()> {
        let new_body = new_body.trim_start_matches('\n').trim_end();
        if new_body.is_empty() {
            anyhow::bail!("edited body is empty");
        }

        let mut client = self.db.get().await?;
        let existing = ChatMessage::get(&client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("message not found"))?;
        let is_owner = existing.user_id == user_id;
        let target_tier = if is_owner {
            Tier::Regular
        } else {
            target_tier_for_user_id(&client, existing.user_id).await?
        };
        ensure_message_permission(permissions, is_owner, Caps::EDIT_OTHER_MESSAGE, target_tier)?;

        let tx = client.transaction().await?;
        let row = tx
            .query_one(
                "UPDATE chat_messages
                 SET body = $1, updated = current_timestamp
                 WHERE id = $2
                 RETURNING *",
                &[&new_body, &message_id],
            )
            .await?;
        let updated = ChatMessage::from(row);
        record_mod_audit(
            &tx,
            user_id,
            ModAuditRecord {
                permissions,
                target_is_self: is_owner,
                audit_action: "message_edit",
                target_kind: "message",
                target_id: Some(message_id),
                metadata: json!({ "room_id": existing.room_id }),
            },
        )
        .await?;
        tx.commit().await?;
        let target_user_ids = ChatRoom::get_target_user_ids(&client, existing.room_id).await?;
        let (mut usernames, mut bonsai_glyphs) =
            Self::load_chat_author_metadata(&client, &[existing.user_id]).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageEdited {
            message: updated,
            target_user_ids,
            author_username: usernames.remove(&existing.user_id),
            author_bonsai_glyph: bonsai_glyphs.remove(&existing.user_id),
        });
        metrics::record_chat_message_edited();
        Ok(())
    }

    pub fn toggle_message_reaction_task(&self, user_id: Uuid, message_id: Uuid, kind: i16) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .toggle_message_reaction(user_id, message_id, kind)
                    .await
                {
                    late_core::error_span!(
                        "chat_toggle_reaction_failed",
                        error = ?e,
                        "failed to toggle message reaction"
                    );
                }
            }
            .instrument(info_span!(
                "chat.toggle_message_reaction_task",
                user_id = %user_id,
                message_id = %message_id,
                kind = kind
            )),
        );
    }

    pub fn toggle_message_pin_task(&self, message_id: Uuid, is_admin: bool) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result: Result<()> = async {
                    if !is_admin {
                        anyhow::bail!("admin-only");
                    }
                    let client = service.db.get().await?;
                    let message = ChatMessage::get(&client, message_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("message not found"))?;
                    ChatMessage::set_pinned(&client, message_id, !message.pinned).await?;
                    Ok(())
                }
                .await;
                if let Err(e) = result {
                    late_core::error_span!(
                        "chat_pin_failed",
                        error = ?e,
                        "failed to toggle message pin"
                    );
                }
            }
            .instrument(info_span!(
                "chat.toggle_message_pin_task",
                message_id = %message_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, message_id = %message_id, kind = kind))]
    async fn toggle_message_reaction(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        kind: i16,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let message = ChatMessage::get(&client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("message not found"))?;
        let is_member = ChatRoomMember::is_member(&client, message.room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        ChatMessageReaction::toggle(&client, message_id, user_id, kind).await?;
        let reactions = ChatMessageReaction::list_summaries_for_messages(&client, &[message_id])
            .await?
            .remove(&message_id)
            .unwrap_or_default();
        let target_user_ids = ChatRoom::get_target_user_ids(&client, message.room_id).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageReactionsUpdated {
            room_id: message.room_id,
            message_id,
            reactions,
            target_user_ids,
        });
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
        let client = self.db.get().await?;
        let target = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot DM yourself");
        }
        let room = ChatRoom::get_or_create_dm(&client, user_id, target.id).await?;
        ChatRoomMember::join(&client, room.id, user_id).await?;
        ChatRoomMember::join(&client, room.id, target.id).await?;
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
        let client = self.db.get().await?;
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        let is_member = ChatRoomMember::is_member(&client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }

        let user_ids = ChatRoomMember::list_user_ids(&client, room_id).await?;
        let usernames = User::list_usernames_by_ids(&client, &user_ids).await?;
        let members = user_ids
            .into_iter()
            .map(|id| {
                usernames
                    .get(&id)
                    .map(|username| format!("@{username}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(id)))
            })
            .collect();
        let title = if room.kind == "dm" {
            "DM Members".to_string()
        } else {
            room.slug
                .as_deref()
                .map(|slug| format!("#{slug} Members"))
                .unwrap_or_else(|| "Room Members".to_string())
        };

        Ok((title, members))
    }

    pub fn list_reaction_owners_task(&self, user_id: Uuid, message_id: Uuid) {
        let service = self.clone();
        let span = info_span!(
            "chat.list_reaction_owners_task",
            user_id = %user_id,
            message_id = %message_id
        );
        tokio::spawn(
            async move {
                let event = match service.list_reaction_owners(user_id, message_id).await {
                    Ok((owners, usernames)) => ChatEvent::ReactionOwnersListed {
                        user_id,
                        message_id,
                        owners,
                        usernames,
                    },
                    Err(e) => ChatEvent::ReactionOwnersListFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_reaction_owners(
        &self,
        user_id: Uuid,
        message_id: Uuid,
    ) -> Result<(Vec<ChatMessageReactionOwners>, HashMap<Uuid, String>)> {
        let client = self.db.get().await?;
        let message = ChatMessage::get(&client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Message not found"))?;
        let is_member = ChatRoomMember::is_member(&client, message.room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }
        let owners = ChatMessageReaction::list_owners_for_message(&client, message_id).await?;
        let mut owner_ids: Vec<Uuid> = owners
            .iter()
            .flat_map(|reaction| reaction.user_ids.iter().copied())
            .collect();
        owner_ids.sort();
        owner_ids.dedup();
        let usernames = User::list_usernames_by_ids(&client, &owner_ids).await?;
        Ok((owners, usernames))
    }

    pub fn list_public_rooms_task(&self, user_id: Uuid) {
        let service = self.clone();
        let span = info_span!("chat.list_public_rooms_task", user_id = %user_id);
        tokio::spawn(
            async move {
                let event = match service.list_public_rooms().await {
                    Ok((title, rooms)) => ChatEvent::PublicRoomsListed {
                        user_id,
                        title,
                        rooms,
                    },
                    Err(e) => ChatEvent::PublicRoomsListFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_public_rooms(&self) -> Result<(String, Vec<String>)> {
        let client = self.db.get().await?;
        let rows = client
            .query(
                "SELECT r.kind,
                        r.slug,
                        r.language_code,
                        COUNT(m.user_id)::bigint AS member_count
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                 GROUP BY r.id, r.kind, r.slug, r.language_code, r.created
                 ORDER BY
                    member_count DESC,
                    COALESCE(r.slug, COALESCE(r.language_code, '')) ASC,
                    r.created ASC,
                    r.id ASC",
                &[],
            )
            .await?;

        let rooms: Vec<String> = rows
            .into_iter()
            .map(|row| {
                let kind: String = row.get("kind");
                let slug: Option<String> = row.get("slug");
                let language_code: Option<String> = row.get("language_code");
                let member_count: i64 = row.get("member_count");
                let label = slug
                    .map(|slug| format!("#{slug}"))
                    .or_else(|| language_code.map(|code| format!("language:{code}")))
                    .unwrap_or(kind);
                let noun = if member_count == 1 {
                    "member"
                } else {
                    "members"
                };
                format!("{label} ({member_count} {noun})")
            })
            .collect();
        let rooms = if rooms.is_empty() {
            vec!["No public rooms".to_string()]
        } else {
            rooms
        };

        Ok(("Public Rooms".to_string(), rooms))
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
        let client = self.db.get().await?;
        let target = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot ignore yourself");
        }
        let (changed, ids) = User::add_ignored_user_id(&client, user_id, target.id).await?;
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
        let client = self.db.get().await?;
        let target = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot unignore yourself");
        }
        let (changed, ids) = User::remove_ignored_user_id(&client, user_id, target.id).await?;
        if !changed {
            anyhow::bail!("@{} is not ignored", target.username);
        }
        Ok((ids, format!("Unignored @{}", target.username)))
    }

    pub fn open_public_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.open_public_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.open_public_room(user_id, &slug).await {
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

    pub fn join_public_room_task(&self, user_id: Uuid, room_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.join_public_room_task", user_id = %user_id, room_id = %room_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.join_public_room(user_id, room_id).await {
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

    pub fn join_game_room_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        let span = info_span!("chat.join_game_room_task", user_id = %user_id, room_id = %room_id);
        tokio::spawn(
            async move {
                match service.join_game_room(user_id, room_id).await {
                    Ok(room_id) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::GameRoomJoined { user_id, room_id });
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

    async fn join_public_room(&self, user_id: Uuid, room_id: Uuid) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.kind != "topic" || room.visibility != "public" {
            anyhow::bail!("Only public rooms can be joined from discover");
        }
        ChatRoomMember::join(&client, room.id, user_id).await?;
        Ok(room.id)
    }

    async fn join_game_room(&self, user_id: Uuid, room_id: Uuid) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.kind != "game" {
            anyhow::bail!("Only game rooms can be joined here");
        }
        ChatRoomMember::join(&client, room.id, user_id).await?;
        Ok(room.id)
    }

    async fn open_public_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::get_or_create_public_room(&client, slug).await?;
        ChatRoom::set_auto_join(&client, room.id, true).await?;
        let users_added = ChatRoom::add_all_users(&client, room.id).await?;
        tracing::info!(
            slug = %slug,
            room_id = %room.id,
            users_added,
            "public room opened and auto-join enabled"
        );
        ChatRoomMember::join(&client, room.id, user_id).await?;
        Ok(room.id)
    }

    pub fn create_private_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_private_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_private_room(user_id, &slug).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreated {
                            user_id,
                            room_id,
                            slug,
                        });
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

    async fn create_private_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::create_private_room(&client, slug).await?;
        ChatRoomMember::join(&client, room.id, user_id).await?;
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
        let client = self.db.get().await?;
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.permanent {
            let name = room.slug.as_deref().unwrap_or("this room");
            anyhow::bail!("Cannot leave #{name} (permanent room)");
        }
        ChatRoomMember::leave(&client, room_id, user_id).await?;
        Ok(())
    }

    pub fn create_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_room(&slug).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreated {
                            user_id,
                            room_id,
                            slug,
                        });
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

    async fn create_room(&self, slug: &str) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::ensure_auto_join(&client, slug).await?;
        let added = ChatRoom::add_all_users(&client, room.id).await?;
        tracing::info!(slug = %slug, room_id = %room.id, users_added = added, "room created");
        Ok(room.id)
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
        let client = self.db.get().await?;
        let room = ChatRoom::ensure_permanent(&client, slug).await?;
        let added = ChatRoom::add_all_users(&client, room.id).await?;
        tracing::info!(slug = %slug, room_id = %room.id, users_added = added, "permanent room created");
        Ok(())
    }

    pub fn fill_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.fill_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.fill_room(&slug).await {
                    Ok(users_added) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomFilled {
                            user_id,
                            slug,
                            users_added,
                        });
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

    async fn fill_room(&self, slug: &str) -> Result<u64> {
        let client = self.db.get().await?;
        if let Some(room) = ChatRoom::find_topic_room(&client, "public", slug).await? {
            ChatRoom::set_auto_join(&client, room.id, true).await?;
            let users_added = ChatRoom::add_all_users(&client, room.id).await?;
            tracing::info!(slug = %slug, room_id = %room.id, users_added, "room filled and auto-join enabled");
            return Ok(users_added);
        }
        if ChatRoom::find_topic_room(&client, "private", slug)
            .await?
            .is_some()
        {
            anyhow::bail!("Only public rooms can be filled");
        }
        anyhow::bail!("Public room #{slug} not found")
    }

    pub fn invite_user_to_room_task(&self, user_id: Uuid, room_id: Uuid, target_username: String) {
        let service = self.clone();
        let span = info_span!(
            "chat.invite_user_to_room_task",
            user_id = %user_id,
            room_id = %room_id,
            target = %target_username
        );
        tokio::spawn(
            async move {
                let event = match service
                    .invite_user_to_room(user_id, room_id, &target_username)
                    .await
                {
                    Ok((room_slug, username)) => ChatEvent::InviteSucceeded {
                        user_id,
                        room_id,
                        room_slug,
                        username,
                    },
                    Err(e) => ChatEvent::InviteFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn invite_user_to_room(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        target_username: &str,
    ) -> Result<(String, String)> {
        let client = self.db.get().await?;
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.kind == "dm" {
            anyhow::bail!("Cannot invite users to a DM");
        }
        let is_member = ChatRoomMember::is_member(&client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }

        let target = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot invite yourself");
        }

        ChatRoomMember::join(&client, room_id, target.id).await?;
        let room_slug = room.slug.clone().unwrap_or_else(|| room.kind.clone());
        Ok((room_slug, target.username))
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
        let client = self.db.get().await?;
        let count = ChatRoom::delete_permanent(&client, slug).await?;
        if count == 0 {
            anyhow::bail!("Permanent room #{slug} not found");
        }
        tracing::info!(slug = %slug, "permanent room deleted");
        Ok(())
    }

    pub fn delete_message_task(&self, user_id: Uuid, message_id: Uuid, permissions: Permissions) {
        let service = self.clone();
        let span = info_span!("chat.delete_message", user_id = %user_id, message_id = %message_id);
        tokio::spawn(
            async move {
                match service
                    .delete_message(user_id, message_id, permissions)
                    .await
                {
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
        permissions: Permissions,
    ) -> Result<Uuid> {
        let mut client = self.db.get().await?;
        // Look up the message to get room_id
        let msg = ChatMessage::get(&client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Message not found"))?;
        let is_owner = msg.user_id == user_id;
        let target_tier = if is_owner {
            Tier::Regular
        } else {
            target_tier_for_user_id(&client, msg.user_id).await?
        };
        ensure_message_permission(
            permissions,
            is_owner,
            Caps::DELETE_OTHER_MESSAGE,
            target_tier,
        )?;
        let tx = client.transaction().await?;
        let count = if is_owner {
            tx.execute(
                "DELETE FROM chat_messages WHERE id = $1 AND user_id = $2",
                &[&message_id, &user_id],
            )
            .await?
        } else {
            tx.execute("DELETE FROM chat_messages WHERE id = $1", &[&message_id])
                .await?
        };
        if count == 0 {
            anyhow::bail!("Cannot delete this message");
        }
        record_mod_audit(
            &tx,
            user_id,
            ModAuditRecord {
                permissions,
                target_is_self: is_owner,
                audit_action: "message_delete",
                target_kind: "message",
                target_id: Some(message_id),
                metadata: json!({ "room_id": msg.room_id }),
            },
        )
        .await?;
        tx.commit().await?;
        tracing::info!(message_id = %message_id, "message deleted");
        Ok(msg.room_id)
    }
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}
