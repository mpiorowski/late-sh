use anyhow::Result;
use chrono::{DateTime, Utc};
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

use deadpool_postgres::GenericClient;
use late_core::{
    MutexRecover,
    db::Db,
    models::{
        artboard_ban::ArtboardBan,
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_reaction::{
            ChatMessageReaction, ChatMessageReactionOwners, ChatMessageReactionSummary,
        },
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        room_ban::RoomBan,
        server_ban::ServerBan,
        user::User,
    },
};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, broadcast, mpsc, watch};
use tracing::{Instrument, info_span};

use crate::app::bonsai::state::stage_for;
use crate::authz::{Action, Permissions, TargetTier};
use crate::metrics;
use crate::session::{SessionMessage, SessionRegistry};
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
    pub permissions: Permissions,
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

struct RoomModRequest {
    action: RoomModAction,
    slug: String,
    username: String,
    duration: Option<chrono::Duration>,
    reason: String,
}

struct ModAuditRecord {
    permissions: Permissions,
    matrix_action: Action,
    target_tier: TargetTier,
    audit_action: &'static str,
    target_kind: &'static str,
    target_id: Option<Uuid>,
    metadata: Value,
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
        let (refresh_signal_tx, refresh_signal_rx) = mpsc::unbounded_channel();

        Self {
            db,
            username_tx,
            username_rx,
            evt_tx,
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
                let (success, lines) = match service
                    .run_mod_command(user_id, permissions, &command)
                    .await
                {
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

    async fn run_mod_command(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        input: &str,
    ) -> Result<Vec<String>> {
        let command = parse_mod_command(input)?;
        match command {
            ModCommand::Help { topic } => Ok(mod_help_lines(topic.as_deref())),
            ModCommand::Status => Ok(vec![format!(
                "status: tier={} mod_surface={}",
                tier_label(permissions),
                permissions.can_access_mod_surface()
            )]),
            ModCommand::Whoami => {
                let client = self.db.get().await?;
                let actor = User::get(&client, actor_user_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("actor user not found"))?;
                Ok(vec![format!(
                    "@{} admin={} mod={}",
                    actor.username,
                    actor.is_admin || permissions.is_admin(),
                    actor.is_moderator || permissions.is_moderator()
                )])
            }
            ModCommand::Users { filter } => self.mod_list_users(permissions, filter).await,
            ModCommand::User { username } => self.mod_user_detail(permissions, &username).await,
            ModCommand::Rooms { filter } => self.mod_list_rooms(permissions, filter).await,
            ModCommand::Room { slug } => self.mod_room_detail(permissions, &slug).await,
            ModCommand::RoomBans { slug } => self.mod_room_bans(permissions, &slug).await,
            ModCommand::Audit { filter } => self.mod_audit(permissions, filter).await,
            ModCommand::RoomAction {
                action,
                slug,
                username,
                duration,
                reason,
            } => {
                self.mod_room_action(
                    actor_user_id,
                    permissions,
                    RoomModRequest {
                        action,
                        slug,
                        username,
                        duration,
                        reason,
                    },
                )
                .await
            }
            ModCommand::RoomAdmin {
                action,
                slug,
                value,
            } => {
                self.mod_room_admin(actor_user_id, permissions, action, &slug, value)
                    .await
            }
            ModCommand::ServerUser {
                action,
                username,
                duration,
                reason,
            } => {
                self.mod_server_user(
                    actor_user_id,
                    permissions,
                    action,
                    &username,
                    duration,
                    reason,
                )
                .await
            }
            ModCommand::ServerIp {
                action,
                ip_address,
                duration,
                reason,
            } => {
                self.mod_server_ip(
                    actor_user_id,
                    permissions,
                    action,
                    &ip_address,
                    duration,
                    reason,
                )
                .await
            }
            ModCommand::ServerBans => self.mod_server_bans(permissions).await,
            ModCommand::Artboard {
                action,
                username,
                duration,
                reason,
            } => {
                self.mod_artboard(
                    actor_user_id,
                    permissions,
                    action,
                    &username,
                    duration,
                    reason,
                )
                .await
            }
            ModCommand::ArtboardBans => self.mod_artboard_bans(permissions).await,
            ModCommand::Role { action, username } => {
                self.mod_role(actor_user_id, permissions, action, &username)
                    .await
            }
            ModCommand::Sessions { .. } => Ok(vec![
                "sessions: not wired in this PR slice yet".to_string(),
                "live session details will use SessionRegistry from the modal/app layer"
                    .to_string(),
            ]),
        }
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
        permissions: Permissions,
    ) {
        self.send_message_with_reply_task(SendMessageTask {
            user_id,
            room_id,
            room_slug,
            body,
            reply_to_message_id: None,
            request_id,
            permissions,
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
            permissions,
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
                        permissions,
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
        permissions: Permissions,
    ) -> Result<()> {
        let body = body.trim_start_matches('\n').trim_end();
        if body.is_empty() {
            return Ok(());
        }

        if room_slug.as_deref() == Some("announcements") && !permissions.can_post_announcements() {
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
        let target_tier = if existing.user_id == user_id {
            TargetTier::Own
        } else {
            let author = User::get(&client, existing.user_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("message author not found"))?;
            TargetTier::from_user_flags(author.is_admin, author.is_moderator)
        };
        ensure_decision(permissions, Action::EditMessage, target_tier)?;

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
                matrix_action: Action::EditMessage,
                target_tier,
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

    async fn open_public_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::get_or_create_public_room(&client, slug).await?;
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
        let target_tier = if msg.user_id == user_id {
            TargetTier::Own
        } else {
            let author = User::get(&client, msg.user_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("message author not found"))?;
            TargetTier::from_user_flags(author.is_admin, author.is_moderator)
        };
        ensure_decision(permissions, Action::DeleteMessage, target_tier)?;
        let tx = client.transaction().await?;
        let count = if target_tier == TargetTier::Own {
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
                matrix_action: Action::DeleteMessage,
                target_tier,
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

    async fn mod_list_users(
        &self,
        permissions: Permissions,
        filter: Option<String>,
    ) -> Result<Vec<String>> {
        ensure_mod_surface(permissions)?;
        let client = self.db.get().await?;
        let needle = filter.unwrap_or_default().to_ascii_lowercase();
        let users = User::all(&client).await?;
        let mut lines: Vec<String> = users
            .into_iter()
            .filter(|user| {
                needle.is_empty() || user.username.to_ascii_lowercase().contains(&needle)
            })
            .map(|user| {
                let mut tags = Vec::new();
                if user.is_admin {
                    tags.push("admin");
                }
                if user.is_moderator {
                    tags.push("mod");
                }
                if tags.is_empty() {
                    format!("@{}", user.username)
                } else {
                    format!("@{} [{}]", user.username, tags.join(","))
                }
            })
            .collect();
        lines.sort();
        if lines.is_empty() {
            lines.push("no matching users".to_string());
        }
        Ok(lines)
    }

    async fn mod_user_detail(
        &self,
        permissions: Permissions,
        username: &str,
    ) -> Result<Vec<String>> {
        ensure_mod_surface(permissions)?;
        let client = self.db.get().await?;
        let user = find_user_by_mod_name(&client, username).await?;
        let server_ban = ServerBan::find_active_for_user_id(&client, user.id).await?;
        let artboard_ban = ArtboardBan::find_active_for_user(&client, user.id).await?;
        Ok(vec![
            format!("@{}", user.username),
            format!("id: {}", user.id),
            format!("admin: {}", user.is_admin),
            format!("moderator: {}", user.is_moderator),
            format!("created: {}", user.created.format("%Y-%m-%d %H:%M UTC")),
            format!("last_seen: {}", user.last_seen.format("%Y-%m-%d %H:%M UTC")),
            format!("server_banned: {}", server_ban.is_some()),
            format!("artboard_banned: {}", artboard_ban.is_some()),
        ])
    }

    async fn mod_list_rooms(
        &self,
        permissions: Permissions,
        filter: Option<String>,
    ) -> Result<Vec<String>> {
        ensure_mod_surface(permissions)?;
        let client = self.db.get().await?;
        let needle = filter.unwrap_or_default().to_ascii_lowercase();
        let rows = client
            .query(
                "SELECT r.id, r.kind, r.visibility, r.permanent, r.slug, r.language_code,
                        COUNT(DISTINCT m.user_id)::bigint AS member_count,
                        COUNT(DISTINCT b.id)::bigint AS active_ban_count
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 LEFT JOIN room_bans b
                   ON b.room_id = r.id
                  AND (b.expires_at IS NULL OR b.expires_at > current_timestamp)
                 GROUP BY r.id
                 ORDER BY COALESCE(r.slug, COALESCE(r.language_code, r.kind)), r.created",
                &[],
            )
            .await?;
        let mut lines = Vec::new();
        for row in rows {
            let kind: String = row.get("kind");
            let visibility: String = row.get("visibility");
            let permanent: bool = row.get("permanent");
            let slug: Option<String> = row.get("slug");
            let language_code: Option<String> = row.get("language_code");
            let member_count: i64 = row.get("member_count");
            let active_ban_count: i64 = row.get("active_ban_count");
            let label = slug
                .map(|slug| format!("#{slug}"))
                .or_else(|| language_code.map(|code| format!("language:{code}")))
                .unwrap_or(kind);
            if !needle.is_empty() && !label.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            lines.push(format!(
                "{label} visibility={visibility} permanent={permanent} members={member_count} bans={active_ban_count}"
            ));
        }
        if lines.is_empty() {
            lines.push("no matching rooms".to_string());
        }
        Ok(lines)
    }

    async fn mod_room_detail(&self, permissions: Permissions, slug: &str) -> Result<Vec<String>> {
        ensure_mod_surface(permissions)?;
        let client = self.db.get().await?;
        let room = find_room_by_mod_slug(&client, slug).await?;
        let members = ChatRoomMember::list_user_ids(&client, room.id).await?;
        Ok(vec![
            format!(
                "#{}",
                room.slug.clone().unwrap_or_else(|| room.kind.clone())
            ),
            format!("id: {}", room.id),
            format!("kind: {}", room.kind),
            format!("visibility: {}", room.visibility),
            format!("permanent: {}", room.permanent),
            format!("auto_join: {}", room.auto_join),
            format!("members: {}", members.len()),
        ])
    }

    async fn mod_room_bans(&self, permissions: Permissions, slug: &str) -> Result<Vec<String>> {
        ensure_decision(
            permissions,
            Action::ViewUserSanctionHistory,
            TargetTier::NotApplicable,
        )?;
        let client = self.db.get().await?;
        let room = find_room_by_mod_slug(&client, slug).await?;
        let rows = client
            .query(
                "SELECT b.target_user_id, b.reason, b.expires_at,
                        target.username AS target_username,
                        actor.username AS actor_username
                 FROM room_bans b
                 LEFT JOIN users target ON target.id = b.target_user_id
                 LEFT JOIN users actor ON actor.id = b.actor_user_id
                 WHERE b.room_id = $1
                   AND (b.expires_at IS NULL OR b.expires_at > current_timestamp)
                 ORDER BY b.created DESC
                 LIMIT 50",
                &[&room.id],
            )
            .await?;
        let mut lines = Vec::new();
        for row in rows {
            let target_user_id: Uuid = row.get("target_user_id");
            let target_username: Option<String> = row.get("target_username");
            let actor_username: Option<String> = row.get("actor_username");
            let reason: String = row.get("reason");
            let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
            lines.push(format!(
                "@{} actor=@{} expires={} reason={}",
                target_username.unwrap_or_else(|| short_user_id(target_user_id)),
                actor_username.unwrap_or_else(|| "unknown".to_string()),
                format_ban_expiry(expires_at),
                format_ban_reason(&reason)
            ));
        }
        if lines.is_empty() {
            lines.push(format!("no active bans for #{}", room_label(&room)));
        }
        Ok(lines)
    }

    async fn mod_server_bans(&self, permissions: Permissions) -> Result<Vec<String>> {
        ensure_decision(
            permissions,
            Action::ViewUserSanctionHistory,
            TargetTier::NotApplicable,
        )?;
        let client = self.db.get().await?;
        let rows = client
            .query(
                "SELECT b.ban_type, b.target_user_id, b.fingerprint, b.ip_address,
                        b.snapshot_username, b.reason, b.expires_at,
                        target.username AS target_username,
                        actor.username AS actor_username
                 FROM server_bans b
                 LEFT JOIN users target ON target.id = b.target_user_id
                 LEFT JOIN users actor ON actor.id = b.actor_user_id
                 WHERE b.expires_at IS NULL OR b.expires_at > current_timestamp
                 ORDER BY b.created DESC
                 LIMIT 50",
                &[],
            )
            .await?;
        let mut lines = Vec::new();
        for row in rows {
            let ban_type: String = row.get("ban_type");
            let target_user_id: Option<Uuid> = row.get("target_user_id");
            let target_username: Option<String> = row.get("target_username");
            let fingerprint: Option<String> = row.get("fingerprint");
            let ip_address: Option<String> = row.get("ip_address");
            let snapshot_username: Option<String> = row.get("snapshot_username");
            let actor_username: Option<String> = row.get("actor_username");
            let reason: String = row.get("reason");
            let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
            let target = match ban_type.as_str() {
                "user" => target_username
                    .map(|username| format!("@{username}"))
                    .or_else(|| target_user_id.map(short_user_id))
                    .unwrap_or_else(|| "-".to_string()),
                "ip" => {
                    let mut target = ip_address.unwrap_or_else(|| "-".to_string());
                    if let Some(username) = snapshot_username {
                        target.push_str(&format!(" snapshot=@{username}"));
                    }
                    target
                }
                "fingerprint" => fingerprint
                    .as_deref()
                    .map(short_fingerprint)
                    .unwrap_or_else(|| "-".to_string()),
                _ => "-".to_string(),
            };
            lines.push(format!(
                "{} {} actor=@{} expires={} reason={}",
                ban_type,
                target,
                actor_username.unwrap_or_else(|| "unknown".to_string()),
                format_ban_expiry(expires_at),
                format_ban_reason(&reason)
            ));
        }
        if lines.is_empty() {
            lines.push("no active server bans".to_string());
        }
        Ok(lines)
    }

    async fn mod_artboard_bans(&self, permissions: Permissions) -> Result<Vec<String>> {
        ensure_decision(
            permissions,
            Action::ViewUserSanctionHistory,
            TargetTier::NotApplicable,
        )?;
        let client = self.db.get().await?;
        let rows = client
            .query(
                "SELECT b.target_user_id, b.reason, b.expires_at,
                        target.username AS target_username,
                        actor.username AS actor_username
                 FROM artboard_bans b
                 LEFT JOIN users target ON target.id = b.target_user_id
                 LEFT JOIN users actor ON actor.id = b.actor_user_id
                 WHERE b.expires_at IS NULL OR b.expires_at > current_timestamp
                 ORDER BY b.created DESC
                 LIMIT 50",
                &[],
            )
            .await?;
        let mut lines = Vec::new();
        for row in rows {
            let target_user_id: Uuid = row.get("target_user_id");
            let target_username: Option<String> = row.get("target_username");
            let actor_username: Option<String> = row.get("actor_username");
            let reason: String = row.get("reason");
            let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
            lines.push(format!(
                "@{} actor=@{} expires={} reason={}",
                target_username.unwrap_or_else(|| short_user_id(target_user_id)),
                actor_username.unwrap_or_else(|| "unknown".to_string()),
                format_ban_expiry(expires_at),
                format_ban_reason(&reason)
            ));
        }
        if lines.is_empty() {
            lines.push("no active artboard bans".to_string());
        }
        Ok(lines)
    }

    async fn mod_audit(
        &self,
        permissions: Permissions,
        filter: Option<String>,
    ) -> Result<Vec<String>> {
        ensure_decision(
            permissions,
            Action::ViewAuditLogOther,
            TargetTier::NotApplicable,
        )?;
        let client = self.db.get().await?;
        let rows = client
            .query(
                "SELECT log.created, log.action, log.target_kind, log.target_id,
                        actor.username AS actor_username,
                        target.username AS target_username
                 FROM moderation_audit_log log
                 LEFT JOIN users actor ON actor.id = log.actor_user_id
                 LEFT JOIN users target ON target.id = log.target_id
                 ORDER BY log.created DESC
                 LIMIT 50",
                &[],
            )
            .await?;
        let needle = filter.unwrap_or_default().to_ascii_lowercase();
        let mut lines = Vec::new();
        for row in rows {
            let action: String = row.get("action");
            let target_kind: String = row.get("target_kind");
            let target_id: Option<Uuid> = row.get("target_id");
            let actor_username: Option<String> = row.get("actor_username");
            let target_username: Option<String> = row.get("target_username");
            let created: DateTime<Utc> = row.get("created");
            let target = target_username
                .map(|name| format!("@{name}"))
                .or_else(|| target_id.map(|id| id.to_string()))
                .unwrap_or_else(|| "-".to_string());
            let line = format!(
                "{} {} actor=@{} target={}:{}",
                created.format("%Y-%m-%d %H:%M"),
                action,
                actor_username.unwrap_or_else(|| "unknown".to_string()),
                target_kind,
                target
            );
            if needle.is_empty() || line.to_ascii_lowercase().contains(&needle) {
                lines.push(line);
            }
        }
        if lines.is_empty() {
            lines.push("no audit entries".to_string());
        }
        Ok(lines)
    }

    async fn mod_room_action(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        request: RoomModRequest,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let room = find_room_by_mod_slug(&client, &request.slug).await?;
        let target = find_user_by_mod_name(&client, &request.username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = TargetTier::from_user_flags(target.is_admin, target.is_moderator);
        let matrix_action = match request.action {
            RoomModAction::Kick => Action::KickFromRoom,
            RoomModAction::Ban => Action::BanFromRoom,
            RoomModAction::Unban => Action::UnbanFromRoom,
        };
        ensure_decision(permissions, matrix_action, target_tier)?;
        let room_slug = room.slug.clone().unwrap_or_else(|| room.kind.clone());
        let tx = client.transaction().await?;
        match request.action {
            RoomModAction::Kick => {
                tx.execute(
                    "DELETE FROM chat_room_members WHERE room_id = $1 AND user_id = $2",
                    &[&room.id, &target.id],
                )
                .await?;
            }
            RoomModAction::Ban => {
                let expires_at = request.duration.map(|d| Utc::now() + d);
                tx.execute(
                    "INSERT INTO room_bans
                     (room_id, target_user_id, actor_user_id, reason, expires_at)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (room_id, target_user_id)
                     DO UPDATE SET actor_user_id = EXCLUDED.actor_user_id,
                                   reason = EXCLUDED.reason,
                                   expires_at = EXCLUDED.expires_at,
                                   updated = current_timestamp",
                    &[
                        &room.id,
                        &target.id,
                        &actor_user_id,
                        &request.reason,
                        &expires_at,
                    ],
                )
                .await?;
                tx.execute(
                    "DELETE FROM chat_room_members WHERE room_id = $1 AND user_id = $2",
                    &[&room.id, &target.id],
                )
                .await?;
            }
            RoomModAction::Unban => {
                tx.execute(
                    "DELETE FROM room_bans WHERE room_id = $1 AND target_user_id = $2",
                    &[&room.id, &target.id],
                )
                .await?;
            }
        }
        let audit_action = match request.action {
            RoomModAction::Kick => "room_kick",
            RoomModAction::Ban => "room_ban",
            RoomModAction::Unban => "room_unban",
        };
        record_mod_audit(
            &tx,
            actor_user_id,
            ModAuditRecord {
                permissions,
                matrix_action,
                target_tier,
                audit_action,
                target_kind: "user",
                target_id: Some(target.id),
                metadata: json!({ "room_id": room.id, "room_slug": room.slug, "reason": request.reason }),
            },
        )
        .await?;
        tx.commit().await?;
        if matches!(request.action, RoomModAction::Kick | RoomModAction::Ban) {
            let notified = self
                .notify_room_removed(
                    target.id,
                    room.id,
                    room_slug.clone(),
                    match request.action {
                        RoomModAction::Kick => "Removed from room".to_string(),
                        RoomModAction::Ban => "Banned from room".to_string(),
                        RoomModAction::Unban => unreachable!(),
                    },
                )
                .await;
            if notified > 0 {
                tracing::info!(
                    target_user_id = %target.id,
                    room_id = %room.id,
                    notified,
                    "room moderation command updated active sessions"
                );
            }
        }
        Ok(vec![format!(
            "{} @{} in #{}",
            request.action.past_tense(),
            target.username,
            room_slug
        )])
    }

    async fn notify_room_removed(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        slug: String,
        message: String,
    ) -> usize {
        let Some(registry) = self.session_registry.as_ref() else {
            return 0;
        };
        let mut notified = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if registry
                .send_message(
                    &token,
                    SessionMessage::RoomRemoved {
                        room_id,
                        slug: slug.clone(),
                        message: message.clone(),
                    },
                )
                .await
            {
                notified += 1;
            }
        }
        notified
    }

    async fn mod_room_admin(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: RoomAdminAction,
        slug: &str,
        value: Option<String>,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let room = find_room_by_mod_slug(&client, slug).await?;
        let is_system = matches!(room.slug.as_deref(), Some("general" | "announcements"));
        let target_tier = if is_system {
            TargetTier::System
        } else {
            TargetTier::NotApplicable
        };
        let matrix_action = match action {
            RoomAdminAction::Rename => Action::RenameRoom,
            RoomAdminAction::Public | RoomAdminAction::Private => Action::SetRoomVisibility,
            RoomAdminAction::Delete => Action::DeleteRoom,
        };
        ensure_decision(permissions, matrix_action, target_tier)?;
        let label = room.slug.clone().unwrap_or_else(|| room.kind.clone());
        match action {
            RoomAdminAction::Rename => {
                let Some(new_slug) = value else {
                    anyhow::bail!("usage: room rename #old #new");
                };
                let normalized = normalize_mod_slug(&new_slug)?;
                let tx = client.transaction().await?;
                tx.execute(
                    "UPDATE chat_rooms SET slug = $1, updated = current_timestamp WHERE id = $2",
                    &[&normalized, &room.id],
                )
                .await?;
                record_mod_audit(
                    &tx,
                    actor_user_id,
                    ModAuditRecord {
                        permissions,
                        matrix_action,
                        target_tier,
                        audit_action: "room_rename",
                        target_kind: "room",
                        target_id: Some(room.id),
                        metadata: json!({ "old_slug": label, "new_slug": normalized }),
                    },
                )
                .await?;
                tx.commit().await?;
                Ok(vec![format!("renamed #{label} to #{normalized}")])
            }
            RoomAdminAction::Public | RoomAdminAction::Private => {
                let visibility = match action {
                    RoomAdminAction::Public => "public",
                    RoomAdminAction::Private => "private",
                    RoomAdminAction::Rename | RoomAdminAction::Delete => unreachable!(),
                };
                let tx = client.transaction().await?;
                tx
                    .execute(
                        "UPDATE chat_rooms SET visibility = $1, updated = current_timestamp WHERE id = $2",
                        &[&visibility, &room.id],
                    )
                    .await?;
                record_mod_audit(
                    &tx,
                    actor_user_id,
                    ModAuditRecord {
                        permissions,
                        matrix_action,
                        target_tier,
                        audit_action: "room_visibility",
                        target_kind: "room",
                        target_id: Some(room.id),
                        metadata: json!({ "room_slug": label, "visibility": visibility }),
                    },
                )
                .await?;
                tx.commit().await?;
                Ok(vec![format!("set #{label} {visibility}")])
            }
            RoomAdminAction::Delete => {
                let tx = client.transaction().await?;
                tx.execute("DELETE FROM chat_rooms WHERE id = $1", &[&room.id])
                    .await?;
                record_mod_audit(
                    &tx,
                    actor_user_id,
                    ModAuditRecord {
                        permissions,
                        matrix_action,
                        target_tier,
                        audit_action: "room_delete",
                        target_kind: "room",
                        target_id: Some(room.id),
                        metadata: json!({ "room_slug": label }),
                    },
                )
                .await?;
                tx.commit().await?;
                Ok(vec![format!("deleted #{label}")])
            }
        }
    }

    async fn mod_server_user(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: ServerUserAction,
        username: &str,
        duration: Option<chrono::Duration>,
        reason: String,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target = find_user_by_mod_name(&client, username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = TargetTier::from_user_flags(target.is_admin, target.is_moderator);
        let matrix_action = match action {
            ServerUserAction::Kick => Action::KickUserSessions,
            ServerUserAction::Ban if duration.is_some() => Action::TempBanUser,
            ServerUserAction::Ban => Action::PermaBanUser,
            ServerUserAction::Unban => Action::UnbanUser,
        };
        ensure_decision(permissions, matrix_action, target_tier)?;
        let tx = client.transaction().await?;
        match action {
            ServerUserAction::Kick => {}
            ServerUserAction::Ban => {
                let expires_at = duration.map(|d| Utc::now() + d);
                tx.execute(
                    "INSERT INTO server_bans
                     (ban_type, target_user_id, fingerprint, ip_address, snapshot_username,
                      actor_user_id, reason, expires_at)
                     VALUES ('user', $1, $2, NULL, NULL, $3, $4, $5)",
                    &[
                        &target.id,
                        &target.fingerprint,
                        &actor_user_id,
                        &reason,
                        &expires_at,
                    ],
                )
                .await?;
            }
            ServerUserAction::Unban => {
                tx.execute(
                    "DELETE FROM server_bans
                     WHERE (
                           (ban_type = 'user' AND target_user_id = $1)
                           OR (ban_type = 'fingerprint' AND fingerprint = $2)
                       )
                       AND (expires_at IS NULL OR expires_at > current_timestamp)",
                    &[&target.id, &target.fingerprint],
                )
                .await?;
            }
        }
        let audit_action = match action {
            ServerUserAction::Kick => "server_kick",
            ServerUserAction::Ban => "server_ban",
            ServerUserAction::Unban => "server_unban",
        };
        record_mod_audit(
            &tx,
            actor_user_id,
            ModAuditRecord {
                permissions,
                matrix_action,
                target_tier,
                audit_action,
                target_kind: "user",
                target_id: Some(target.id),
                metadata: json!({ "reason": reason }),
            },
        )
        .await?;
        tx.commit().await?;
        if matches!(action, ServerUserAction::Kick | ServerUserAction::Ban) {
            let tokens = self.session_tokens_for_user_id(target.id);
            let terminated = self
                .terminate_session_tokens(tokens, action.termination_reason())
                .await;
            tracing::info!(
                target_user_id = %target.id,
                action = action.audit_name(),
                terminated,
                "server moderation command terminated active sessions"
            );
        }
        Ok(vec![format!(
            "{} @{}",
            action.past_tense(),
            target.username
        )])
    }

    async fn mod_server_ip(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: ServerIpAction,
        ip_address: &str,
        duration: Option<chrono::Duration>,
        reason: String,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target_tier = TargetTier::Regular;
        let matrix_action = match action {
            ServerIpAction::Ban if duration.is_some() => Action::TempBanUser,
            ServerIpAction::Ban => Action::PermaBanUser,
            ServerIpAction::Unban => Action::UnbanUser,
        };
        ensure_decision(permissions, matrix_action, target_tier)?;
        let snapshot = matches!(action, ServerIpAction::Ban)
            .then(|| self.snapshot_for_ip_ban(ip_address))
            .flatten();
        let tx = client.transaction().await?;
        match action {
            ServerIpAction::Ban => {
                let expires_at = duration.map(|d| Utc::now() + d);
                let snapshot_username =
                    snapshot.as_ref().map(|snapshot| snapshot.username.as_str());
                let snapshot_fingerprint = snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.fingerprint.as_deref());
                tx.execute(
                    "INSERT INTO server_bans
                     (ban_type, target_user_id, fingerprint, ip_address, snapshot_username,
                      actor_user_id, reason, expires_at)
                     VALUES ('ip', NULL, $1, $2, $3, $4, $5, $6)",
                    &[
                        &snapshot_fingerprint,
                        &ip_address,
                        &snapshot_username,
                        &actor_user_id,
                        &reason,
                        &expires_at,
                    ],
                )
                .await?;
            }
            ServerIpAction::Unban => {
                tx.execute(
                    "DELETE FROM server_bans
                     WHERE ip_address = $1
                       AND ban_type = 'ip'
                       AND (expires_at IS NULL OR expires_at > current_timestamp)",
                    &[&ip_address],
                )
                .await?;
            }
        }
        record_mod_audit(
            &tx,
            actor_user_id,
            ModAuditRecord {
                permissions,
                matrix_action,
                target_tier,
                audit_action: action.audit_name(),
                target_kind: "ip",
                target_id: None,
                metadata: json!({
                    "ip_address": ip_address,
                    "reason": reason,
                    "snapshot_username": snapshot.as_ref().map(|snapshot| snapshot.username.as_str()),
                    "snapshot_fingerprint": snapshot.as_ref().and_then(|snapshot| snapshot.fingerprint.as_deref()),
                }),
            },
        )
        .await?;
        tx.commit().await?;
        if matches!(action, ServerIpAction::Ban) {
            let tokens = self.session_tokens_for_ip(ip_address);
            let terminated = self.terminate_session_tokens(tokens, "server IP ban").await;
            tracing::info!(
                ip_address,
                terminated,
                "server IP ban terminated active sessions"
            );
        }
        Ok(vec![format!("{} ip {}", action.past_tense(), ip_address)])
    }

    async fn terminate_session_tokens(&self, tokens: Vec<String>, reason: &str) -> usize {
        let Some(registry) = self.session_registry.as_ref() else {
            return 0;
        };
        let mut terminated = 0;
        for token in tokens {
            if registry
                .send_message(
                    &token,
                    SessionMessage::Terminate {
                        reason: reason.to_string(),
                    },
                )
                .await
            {
                terminated += 1;
            }
        }
        terminated
    }

    async fn notify_artboard_ban_status(
        &self,
        user_id: Uuid,
        banned: bool,
        expires_at: Option<DateTime<Utc>>,
    ) -> usize {
        let Some(registry) = self.session_registry.as_ref() else {
            return 0;
        };
        let mut notified = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if registry
                .send_message(
                    &token,
                    SessionMessage::ArtboardBanChanged { banned, expires_at },
                )
                .await
            {
                notified += 1;
            }
        }
        notified
    }

    async fn notify_permissions_changed(&self, user_id: Uuid, permissions: Permissions) -> usize {
        let Some(registry) = self.session_registry.as_ref() else {
            return 0;
        };
        let mut notified = 0;
        for token in self.session_tokens_for_user_id(user_id) {
            if registry
                .send_message(&token, SessionMessage::PermissionsChanged { permissions })
                .await
            {
                notified += 1;
            }
        }
        notified
    }

    fn session_tokens_for_user_id(&self, user_id: Uuid) -> Vec<String> {
        let Some(active_users) = self.active_users.as_ref() else {
            return Vec::new();
        };
        let guard = active_users.lock_recover();
        guard
            .get(&user_id)
            .map(|user| unique_session_tokens(user.sessions.iter().map(|session| &session.token)))
            .unwrap_or_default()
    }

    fn session_tokens_for_ip(&self, ip_address: &str) -> Vec<String> {
        let Some(active_users) = self.active_users.as_ref() else {
            return Vec::new();
        };
        let Ok(ip_address) = ip_address.parse::<IpAddr>() else {
            return Vec::new();
        };
        let guard = active_users.lock_recover();
        unique_session_tokens(guard.values().flat_map(|user| {
            user.sessions
                .iter()
                .filter(move |session| session.peer_ip == Some(ip_address))
                .map(|session| &session.token)
        }))
    }

    fn snapshot_for_ip_ban(&self, ip_address: &str) -> Option<ServerIpBanSnapshot> {
        let active_users = self.active_users.as_ref()?;
        let guard = active_users.lock_recover();
        let ip_address = ip_address.parse::<IpAddr>().ok()?;
        let mut matches = guard
            .values()
            .flat_map(|user| {
                user.sessions
                    .iter()
                    .filter(move |session| session.peer_ip == Some(ip_address))
                    .map(|session| ServerIpBanSnapshot {
                        username: user.username.clone(),
                        fingerprint: session.fingerprint.clone(),
                    })
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|snapshot| snapshot.username.to_ascii_lowercase());
        matches.into_iter().next()
    }

    async fn mod_artboard(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: ArtboardAction,
        username: &str,
        duration: Option<chrono::Duration>,
        reason: String,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target = find_user_by_mod_name(&client, username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let target_tier = TargetTier::from_user_flags(target.is_admin, target.is_moderator);
        let matrix_action = match action {
            ArtboardAction::Ban => Action::BanFromArtboard,
            ArtboardAction::Unban => Action::UnbanFromArtboard,
        };
        ensure_decision(permissions, matrix_action, target_tier)?;
        let expires_at = matches!(action, ArtboardAction::Ban)
            .then(|| duration.map(|d| Utc::now() + d))
            .flatten();
        let tx = client.transaction().await?;
        match action {
            ArtboardAction::Ban => {
                tx.execute(
                    "INSERT INTO artboard_bans
                     (target_user_id, actor_user_id, reason, expires_at)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (target_user_id)
                     DO UPDATE SET actor_user_id = EXCLUDED.actor_user_id,
                                   reason = EXCLUDED.reason,
                                   expires_at = EXCLUDED.expires_at,
                                   updated = current_timestamp",
                    &[&target.id, &actor_user_id, &reason, &expires_at],
                )
                .await?;
            }
            ArtboardAction::Unban => {
                tx.execute(
                    "DELETE FROM artboard_bans WHERE target_user_id = $1",
                    &[&target.id],
                )
                .await?;
            }
        }
        record_mod_audit(
            &tx,
            actor_user_id,
            ModAuditRecord {
                permissions,
                matrix_action,
                target_tier,
                audit_action: action.audit_name(),
                target_kind: "user",
                target_id: Some(target.id),
                metadata: json!({ "reason": reason }),
            },
        )
        .await?;
        tx.commit().await?;
        let notified = self
            .notify_artboard_ban_status(
                target.id,
                matches!(action, ArtboardAction::Ban),
                expires_at,
            )
            .await;
        if notified > 0 {
            tracing::info!(
                target_user_id = %target.id,
                banned = matches!(action, ArtboardAction::Ban),
                notified,
                "artboard moderation command updated active sessions"
            );
        }
        Ok(vec![format!(
            "{} @{}",
            action.past_tense(),
            target.username
        )])
    }

    async fn mod_role(
        &self,
        actor_user_id: Uuid,
        permissions: Permissions,
        action: RoleAction,
        username: &str,
    ) -> Result<Vec<String>> {
        let mut client = self.db.get().await?;
        let target = find_user_by_mod_name(&client, username).await?;
        ensure_not_self(actor_user_id, target.id)?;
        let matrix_action = match action {
            RoleAction::GrantMod => Action::GrantModerator,
            RoleAction::RevokeMod => Action::RevokeModerator,
            RoleAction::GrantAdmin => Action::GrantAdmin,
        };
        ensure_decision(permissions, matrix_action, TargetTier::NotApplicable)?;
        let (column, value, label) = match action {
            RoleAction::GrantMod => ("is_moderator", true, "granted moderator to"),
            RoleAction::RevokeMod => ("is_moderator", false, "revoked moderator from"),
            RoleAction::GrantAdmin => ("is_admin", true, "granted admin to"),
        };
        let new_is_admin = if matches!(action, RoleAction::GrantAdmin) {
            true
        } else {
            target.is_admin
        };
        let new_is_moderator = match action {
            RoleAction::GrantMod => true,
            RoleAction::RevokeMod => false,
            RoleAction::GrantAdmin => target.is_moderator,
        };
        let tx = client.transaction().await?;
        let query =
            format!("UPDATE users SET {column} = $1, updated = current_timestamp WHERE id = $2");
        tx.execute(&query, &[&value, &target.id]).await?;
        record_mod_audit(
            &tx,
            actor_user_id,
            ModAuditRecord {
                permissions,
                matrix_action,
                target_tier: TargetTier::NotApplicable,
                audit_action: action.audit_name(),
                target_kind: "user",
                target_id: Some(target.id),
                metadata: json!({}),
            },
        )
        .await?;
        tx.commit().await?;
        let notified = self
            .notify_permissions_changed(
                target.id,
                Permissions::new(new_is_admin || self.force_admin, new_is_moderator),
            )
            .await;
        if notified > 0 {
            tracing::info!(
                target_user_id = %target.id,
                notified,
                "role moderation command updated active session permissions"
            );
        }
        Ok(vec![format!("{label} @{}", target.username)])
    }
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

fn room_label(room: &ChatRoom) -> String {
    room.slug.clone().unwrap_or_else(|| room.kind.clone())
}

fn short_fingerprint(fingerprint: &str) -> String {
    const SHORT_FINGERPRINT_LEN: usize = 16;
    let end = fingerprint.len().min(SHORT_FINGERPRINT_LEN);
    fingerprint[..end].to_string()
}

fn format_ban_expiry(expires_at: Option<DateTime<Utc>>) -> String {
    expires_at
        .map(|expires_at| expires_at.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "never".to_string())
}

fn format_ban_reason(reason: &str) -> &str {
    if reason.trim().is_empty() {
        "-"
    } else {
        reason.trim()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ModCommand {
    Help {
        topic: Option<String>,
    },
    Status,
    Whoami,
    Users {
        filter: Option<String>,
    },
    User {
        username: String,
    },
    Sessions {
        username: Option<String>,
    },
    Audit {
        filter: Option<String>,
    },
    Rooms {
        filter: Option<String>,
    },
    Room {
        slug: String,
    },
    RoomBans {
        slug: String,
    },
    RoomAction {
        action: RoomModAction,
        slug: String,
        username: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    RoomAdmin {
        action: RoomAdminAction,
        slug: String,
        value: Option<String>,
    },
    ServerUser {
        action: ServerUserAction,
        username: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    ServerIp {
        action: ServerIpAction,
        ip_address: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    ServerBans,
    Artboard {
        action: ArtboardAction,
        username: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    ArtboardBans,
    Role {
        action: RoleAction,
        username: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoomModAction {
    Kick,
    Ban,
    Unban,
}

impl RoomModAction {
    const fn past_tense(self) -> &'static str {
        match self {
            Self::Kick => "kicked",
            Self::Ban => "banned",
            Self::Unban => "unbanned",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoomAdminAction {
    Rename,
    Public,
    Private,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerUserAction {
    Kick,
    Ban,
    Unban,
}

impl ServerUserAction {
    const fn past_tense(self) -> &'static str {
        match self {
            Self::Kick => "kicked",
            Self::Ban => "banned",
            Self::Unban => "unbanned",
        }
    }

    const fn audit_name(self) -> &'static str {
        match self {
            Self::Kick => "server_kick",
            Self::Ban => "server_ban",
            Self::Unban => "server_unban",
        }
    }

    const fn termination_reason(self) -> &'static str {
        match self {
            Self::Kick => "server kick",
            Self::Ban => "server ban",
            Self::Unban => "server unban",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerIpAction {
    Ban,
    Unban,
}

impl ServerIpAction {
    const fn past_tense(self) -> &'static str {
        match self {
            Self::Ban => "banned",
            Self::Unban => "unbanned",
        }
    }

    const fn audit_name(self) -> &'static str {
        match self {
            Self::Ban => "server_ip_ban",
            Self::Unban => "server_ip_unban",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ServerIpBanSnapshot {
    username: String,
    fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtboardAction {
    Ban,
    Unban,
}

impl ArtboardAction {
    const fn past_tense(self) -> &'static str {
        match self {
            Self::Ban => "artboard-banned",
            Self::Unban => "removed artboard ban for",
        }
    }

    const fn audit_name(self) -> &'static str {
        match self {
            Self::Ban => "artboard_ban",
            Self::Unban => "artboard_unban",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoleAction {
    GrantMod,
    RevokeMod,
    GrantAdmin,
}

impl RoleAction {
    const fn audit_name(self) -> &'static str {
        match self {
            Self::GrantMod => "grant_moderator",
            Self::RevokeMod => "revoke_moderator",
            Self::GrantAdmin => "grant_admin",
        }
    }
}

fn parse_mod_command(input: &str) -> Result<ModCommand> {
    let input = input.trim();
    let input = if input == "/mod" {
        ""
    } else {
        input.strip_prefix("/mod ").map(str::trim).unwrap_or(input)
    };
    if input.is_empty() {
        return Ok(ModCommand::Help { topic: None });
    }

    let mut parts = input.split_whitespace();
    let Some(head) = parts.next() else {
        return Ok(ModCommand::Help { topic: None });
    };
    let rest = parts.collect::<Vec<_>>();

    match head {
        "help" => Ok(ModCommand::Help {
            topic: nonempty(rest.join(" ")),
        }),
        "status" => Ok(ModCommand::Status),
        "whoami" => Ok(ModCommand::Whoami),
        "users" => Ok(ModCommand::Users {
            filter: nonempty(rest.join(" ")),
        }),
        "user" => Ok(ModCommand::User {
            username: required_username(rest.first().copied(), "usage: user @name")?,
        }),
        "sessions" => Ok(ModCommand::Sessions {
            username: rest.first().map(|value| strip_user_prefix(value)),
        }),
        "audit" => Ok(ModCommand::Audit {
            filter: nonempty(rest.join(" ")),
        }),
        "rooms" => Ok(ModCommand::Rooms {
            filter: nonempty(rest.join(" ")),
        }),
        "room" => parse_room_mod_command(&rest),
        "server" => parse_server_mod_command(&rest),
        "artboard" => parse_artboard_mod_command(&rest),
        "grant" => parse_role_mod_command(RoleAction::GrantMod, RoleAction::GrantAdmin, &rest),
        "revoke" => parse_role_mod_command(RoleAction::RevokeMod, RoleAction::RevokeMod, &rest),
        _ => anyhow::bail!("unknown mod command: {head}"),
    }
}

fn parse_room_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: room #slug | room <action> ...");
    };
    match first {
        "bans" => Ok(ModCommand::RoomBans {
            slug: required_slug(parts.get(1).copied(), "usage: room bans #slug")?,
        }),
        "kick" | "ban" | "unban" => {
            let action = match first {
                "kick" => RoomModAction::Kick,
                "ban" => RoomModAction::Ban,
                "unban" => RoomModAction::Unban,
                _ => unreachable!(),
            };
            let slug = required_slug(parts.get(1).copied(), "usage: room kick #slug @name")?;
            let username =
                required_username(parts.get(2).copied(), "usage: room kick #slug @name")?;
            let (duration, reason_start) = if matches!(action, RoomModAction::Ban) {
                parse_optional_duration(parts.get(3).copied(), 3)?
            } else {
                (None, 3)
            };
            Ok(ModCommand::RoomAction {
                action,
                slug,
                username,
                duration,
                reason: parts.get(reason_start..).unwrap_or_default().join(" "),
            })
        }
        "rename" => Ok(ModCommand::RoomAdmin {
            action: RoomAdminAction::Rename,
            slug: required_slug(parts.get(1).copied(), "usage: room rename #old #new")?,
            value: Some(required_slug(
                parts.get(2).copied(),
                "usage: room rename #old #new",
            )?),
        }),
        "public" => Ok(ModCommand::RoomAdmin {
            action: RoomAdminAction::Public,
            slug: required_slug(parts.get(1).copied(), "usage: room public #slug")?,
            value: None,
        }),
        "private" => Ok(ModCommand::RoomAdmin {
            action: RoomAdminAction::Private,
            slug: required_slug(parts.get(1).copied(), "usage: room private #slug")?,
            value: None,
        }),
        "delete" => Ok(ModCommand::RoomAdmin {
            action: RoomAdminAction::Delete,
            slug: required_slug(parts.get(1).copied(), "usage: room delete #slug")?,
            value: None,
        }),
        _ => Ok(ModCommand::Room {
            slug: strip_slug_prefix(first),
        }),
    }
}

fn parse_server_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: server <kick|ban|unban> @name | server <ban-ip|unban-ip> <ip>");
    };
    if first == "bans" {
        return Ok(ModCommand::ServerBans);
    }
    if matches!(first, "ban-ip" | "unban-ip") {
        let ip_address = required_ip_address(
            parts.get(1).copied(),
            "usage: server <ban-ip|unban-ip> <ip>",
        )?;
        let action = match first {
            "ban-ip" => ServerIpAction::Ban,
            "unban-ip" => ServerIpAction::Unban,
            _ => unreachable!(),
        };
        let (duration, reason_start) = if matches!(action, ServerIpAction::Ban) {
            parse_optional_duration(parts.get(2).copied(), 2)?
        } else {
            (None, 2)
        };
        return Ok(ModCommand::ServerIp {
            action,
            ip_address,
            duration,
            reason: parts.get(reason_start..).unwrap_or_default().join(" "),
        });
    }
    let action = match first {
        "kick" => ServerUserAction::Kick,
        "ban" => ServerUserAction::Ban,
        "unban" => ServerUserAction::Unban,
        _ => anyhow::bail!("unknown server action: {first}"),
    };
    let username = required_username(parts.get(1).copied(), "usage: server <action> @name")?;
    let (duration, reason_start) = if matches!(action, ServerUserAction::Ban) {
        parse_optional_duration(parts.get(2).copied(), 2)?
    } else {
        (None, 2)
    };
    Ok(ModCommand::ServerUser {
        action,
        username,
        duration,
        reason: parts.get(reason_start..).unwrap_or_default().join(" "),
    })
}

fn parse_artboard_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: artboard <ban|unban> @name");
    };
    if first == "bans" {
        return Ok(ModCommand::ArtboardBans);
    }
    let action = match first {
        "ban" => ArtboardAction::Ban,
        "unban" => ArtboardAction::Unban,
        _ => anyhow::bail!("unknown artboard action: {first}"),
    };
    let username = required_username(parts.get(1).copied(), "usage: artboard <action> @name")?;
    let (duration, reason_start) = if matches!(action, ArtboardAction::Ban) {
        parse_optional_duration(parts.get(2).copied(), 2)?
    } else {
        (None, 2)
    };
    Ok(ModCommand::Artboard {
        action,
        username,
        duration,
        reason: parts.get(reason_start..).unwrap_or_default().join(" "),
    })
}

fn parse_role_mod_command(
    mod_action: RoleAction,
    admin_action: RoleAction,
    parts: &[&str],
) -> Result<ModCommand> {
    let Some(role) = parts.first().copied() else {
        anyhow::bail!("usage: grant mod @name | grant admin @name | revoke mod @name");
    };
    let action = match role {
        "mod" | "moderator" => mod_action,
        "admin" if matches!(admin_action, RoleAction::GrantAdmin) => admin_action,
        "admin" => anyhow::bail!("revoke admin is not implemented"),
        _ => anyhow::bail!("unknown role: {role}"),
    };
    Ok(ModCommand::Role {
        action,
        username: required_username(parts.get(1).copied(), "usage: grant mod @name")?,
    })
}

fn parse_optional_duration(
    value: Option<&str>,
    duration_index: usize,
) -> Result<(Option<chrono::Duration>, usize)> {
    let Some(value) = value else {
        return Ok((None, duration_index));
    };
    if let Some(duration) = parse_mod_duration(value)? {
        Ok((Some(duration), duration_index + 1))
    } else {
        Ok((None, duration_index))
    }
}

fn parse_mod_duration(value: &str) -> Result<Option<chrono::Duration>> {
    if value.is_empty() {
        return Ok(None);
    }
    let Some(unit) = value.chars().last() else {
        return Ok(None);
    };
    if !matches!(unit, 's' | 'm' | 'h' | 'd' | 'S' | 'M' | 'H' | 'D') {
        return Ok(None);
    }
    let amount_text = &value[..value.len() - unit.len_utf8()];
    let amount: i64 = amount_text
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid duration: {value}"))?;
    if amount <= 0 {
        anyhow::bail!("duration must be positive");
    }
    let duration = match unit.to_ascii_lowercase() {
        's' => chrono::Duration::seconds(amount),
        'm' => chrono::Duration::minutes(amount),
        'h' => chrono::Duration::hours(amount),
        'd' => chrono::Duration::days(amount),
        _ => unreachable!(),
    };
    Ok(Some(duration))
}

fn nonempty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn required_username(value: Option<&str>, usage: &str) -> Result<String> {
    let Some(value) = value else {
        anyhow::bail!("{usage}");
    };
    let username = strip_user_prefix(value);
    if username.is_empty() {
        anyhow::bail!("{usage}");
    }
    Ok(username)
}

fn required_slug(value: Option<&str>, usage: &str) -> Result<String> {
    let Some(value) = value else {
        anyhow::bail!("{usage}");
    };
    let slug = strip_slug_prefix(value);
    if slug.is_empty() {
        anyhow::bail!("{usage}");
    }
    Ok(slug)
}

fn required_ip_address(value: Option<&str>, usage: &str) -> Result<String> {
    let Some(value) = value else {
        anyhow::bail!("{usage}");
    };
    let ip_address: IpAddr = value
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid IP address: {value}"))?;
    Ok(ip_address.to_string())
}

fn unique_session_tokens<'a>(tokens: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tokens
        .filter(|token| seen.insert((*token).clone()))
        .cloned()
        .collect()
}

fn strip_user_prefix(value: &str) -> String {
    value.trim().trim_start_matches('@').to_string()
}

fn strip_slug_prefix(value: &str) -> String {
    value.trim().trim_start_matches('#').to_string()
}

fn normalize_mod_slug(slug: &str) -> Result<String> {
    let slug = strip_slug_prefix(slug).to_ascii_lowercase();
    let slug = slug.trim();
    if slug.is_empty() {
        anyhow::bail!("room slug cannot be empty");
    }

    let mut normalized = String::with_capacity(slug.len());
    let mut last_was_dash = false;
    for ch in slug.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            normalized.push(ch);
            last_was_dash = false;
        } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '.' | '/' | '\\') {
            if !normalized.is_empty() && !last_was_dash {
                normalized.push('-');
                last_was_dash = true;
            }
        } else if !normalized.is_empty() && !last_was_dash {
            normalized.push('-');
            last_was_dash = true;
        }
    }

    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        anyhow::bail!("room slug cannot be empty");
    }
    Ok(normalized)
}

async fn find_user_by_mod_name(client: &tokio_postgres::Client, username: &str) -> Result<User> {
    User::find_by_username(client, &strip_user_prefix(username))
        .await?
        .ok_or_else(|| anyhow::anyhow!("user not found: @{username}"))
}

async fn find_room_by_mod_slug(client: &tokio_postgres::Client, slug: &str) -> Result<ChatRoom> {
    let slug = normalize_mod_slug(slug)?;
    let row = client
        .query_opt(
            "SELECT * FROM chat_rooms WHERE slug = $1 AND kind <> 'dm' LIMIT 1",
            &[&slug],
        )
        .await?;
    row.map(ChatRoom::from)
        .ok_or_else(|| anyhow::anyhow!("room not found: #{slug}"))
}

fn ensure_mod_surface(permissions: Permissions) -> Result<()> {
    ensure_decision(
        permissions,
        Action::OpenControlCenter,
        TargetTier::NotApplicable,
    )
}

fn ensure_decision(permissions: Permissions, action: Action, target: TargetTier) -> Result<()> {
    if permissions.decide(action, target).is_allowed() {
        Ok(())
    } else {
        anyhow::bail!("Moderator or admin only")
    }
}

fn ensure_not_self(actor_user_id: Uuid, target_user_id: Uuid) -> Result<()> {
    if actor_user_id == target_user_id {
        anyhow::bail!("cannot target yourself");
    }
    Ok(())
}

async fn record_mod_audit(
    client: &impl GenericClient,
    actor_user_id: Uuid,
    record: ModAuditRecord,
) -> Result<()> {
    if record
        .permissions
        .should_audit(record.matrix_action, record.target_tier)
    {
        client
            .execute(
                "INSERT INTO moderation_audit_log
                 (actor_user_id, action, target_kind, target_id, metadata)
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    &actor_user_id,
                    &record.audit_action,
                    &record.target_kind,
                    &record.target_id,
                    &record.metadata,
                ],
            )
            .await?;
    }
    Ok(())
}

fn tier_label(permissions: Permissions) -> &'static str {
    if permissions.is_admin() {
        "admin"
    } else if permissions.is_moderator() {
        "moderator"
    } else {
        "regular"
    }
}

fn mod_help_lines(topic: Option<&str>) -> Vec<String> {
    let Some(topic) = topic
        .map(normalize_help_topic)
        .filter(|topic| !topic.is_empty())
    else {
        return help_lines(&[
            "help [command]",
            "status",
            "whoami",
            "users [filter]",
            "user @name",
            "sessions [@name]",
            "audit [filter]",
            "rooms [filter]",
            "room #slug",
            "room bans #slug",
            "room kick #slug @name [reason...]",
            "room ban #slug @name [duration] [reason...]",
            "room unban #slug @name",
            "room rename #old #new",
            "room public #slug",
            "room private #slug",
            "room delete #slug",
            "server bans",
            "server kick @name [reason...]",
            "server ban @name [duration] [reason...]",
            "server unban @name",
            "server ban-ip <ip> [duration] [reason...]",
            "server unban-ip <ip>",
            "artboard bans",
            "artboard ban @name [duration] [reason...]",
            "artboard unban @name",
            "grant mod @name",
            "revoke mod @name",
            "grant admin @name",
            "",
            "Use help <command> for details, e.g. help audit or help server ban-ip.",
        ]);
    };

    let lines = match topic.as_str() {
        "help" => &[
            "help [command]",
            "Shows the command list or focused help for one command.",
            "command: optional command/subcommand, e.g. audit, room ban, server ban-ip.",
        ][..],
        "status" => &[
            "status",
            "Shows your effective moderation tier and whether the mod surface is enabled.",
        ],
        "whoami" => &[
            "whoami",
            "Shows your username and effective admin/moderator flags.",
        ],
        "users" => &[
            "users [filter]",
            "Lists users.",
            "filter: optional case-insensitive username substring.",
        ],
        "user" => &[
            "user @name",
            "Shows one user's id, roles, timestamps, and active server/artboard ban flags.",
            "@name: username, with or without @.",
        ],
        "sessions" => &[
            "sessions [@name]",
            "Lists active SSH sessions.",
            "@name: optional username filter, with or without @.",
        ],
        "audit" => &[
            "audit [filter]",
            "Shows the latest 50 moderation audit entries.",
            "filter: optional case-insensitive substring matched against rendered lines.",
            "Examples: audit server_ban, audit actor=@alice, audit target=user:@bob.",
        ],
        "rooms" => &[
            "rooms [filter]",
            "Lists rooms with visibility, permanence, member count, and active ban count.",
            "filter: optional case-insensitive room label substring.",
        ],
        "room" => &[
            "room #slug",
            "Shows one room's id, kind, visibility, permanence, auto-join flag, and member count.",
            "#slug: room slug, with or without #.",
            "Subcommands: room bans, room kick, room ban, room unban, room rename, room public, room private, room delete.",
        ],
        "room bans" => &[
            "room bans #slug",
            "Lists active bans in a room.",
            "#slug: room slug, with or without #.",
        ],
        "room kick" => &[
            "room kick #slug @name [reason...]",
            "Removes a user from a room without creating a ban.",
            "#slug: room slug. @name: username. reason: optional audit text.",
        ],
        "room ban" => &[
            "room ban #slug @name [duration] [reason...]",
            "Bans a user from a room and removes their membership.",
            "#slug: room slug. @name: username.",
            "duration: optional positive number plus s/m/h/d, e.g. 30m or 7d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "room unban" => &[
            "room unban #slug @name",
            "Removes an active room ban for a user.",
            "#slug: room slug. @name: username.",
        ],
        "room rename" => &[
            "room rename #old #new",
            "Renames a room slug.",
            "#old: current slug. #new: new slug.",
        ],
        "room public" => &[
            "room public #slug",
            "Makes a room public.",
            "#slug: room slug.",
        ],
        "room private" => &[
            "room private #slug",
            "Makes a room private.",
            "#slug: room slug.",
        ],
        "room delete" => &["room delete #slug", "Deletes a room.", "#slug: room slug."],
        "server" => &[
            "server <kick|ban|unban> @name",
            "server <ban-ip|unban-ip> <ip>",
            "Applies server-wide session removal or bans.",
            "Subcommands: server bans, server kick, server ban, server unban, server ban-ip, server unban-ip.",
        ],
        "server bans" => &[
            "server bans",
            "Lists active server user, fingerprint, and IP bans.",
        ],
        "server kick" => &[
            "server kick @name [reason...]",
            "Terminates a user's active sessions without creating a ban.",
            "@name: username. reason: optional audit text.",
        ],
        "server ban" => &[
            "server ban @name [duration] [reason...]",
            "Creates a server user ban and terminates active sessions.",
            "@name: username.",
            "duration: optional positive number plus s/m/h/d, e.g. 2h or 7d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "server unban" => &[
            "server unban @name",
            "Removes active server user/fingerprint bans for that user.",
            "@name: username.",
        ],
        "server ban-ip" => &[
            "server ban-ip <ip> [duration] [reason...]",
            "Creates a server IP ban and terminates active sessions from that IP.",
            "ip: IPv4 or IPv6 address.",
            "duration: optional positive number plus s/m/h/d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "server unban-ip" => &[
            "server unban-ip <ip>",
            "Removes an active server IP ban.",
            "ip: IPv4 or IPv6 address.",
        ],
        "artboard" => &[
            "artboard <ban|unban> @name",
            "Controls whether a user may use the artboard.",
            "Subcommands: artboard bans, artboard ban, artboard unban.",
        ],
        "artboard bans" => &["artboard bans", "Lists active artboard bans."],
        "artboard ban" => &[
            "artboard ban @name [duration] [reason...]",
            "Bans a user from the artboard.",
            "@name: username.",
            "duration: optional positive number plus s/m/h/d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "artboard unban" => &[
            "artboard unban @name",
            "Removes an artboard ban for a user.",
            "@name: username.",
        ],
        "grant" => &[
            "grant <mod|admin> @name",
            "Grants a role to a user.",
            "mod: moderator role. admin: admin role. @name: username.",
        ],
        "grant mod" => &[
            "grant mod @name",
            "Grants moderator role to a user.",
            "@name: username.",
        ],
        "grant admin" => &[
            "grant admin @name",
            "Grants admin role to a user.",
            "@name: username.",
        ],
        "revoke" | "revoke mod" => &[
            "revoke mod @name",
            "Revokes moderator role from a user.",
            "@name: username.",
        ],
        _ => {
            return vec![
                format!("unknown help topic: {topic}"),
                "try: help".to_string(),
            ];
        }
    };
    help_lines(lines)
}

fn normalize_help_topic(topic: &str) -> String {
    let topic = topic
        .trim()
        .strip_prefix("/mod ")
        .map(str::trim)
        .unwrap_or_else(|| topic.trim());
    topic
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn help_lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_string()).collect()
}

#[cfg(test)]
mod mod_command_tests {
    use super::*;

    #[test]
    fn parses_optional_mod_prefix() {
        assert_eq!(
            parse_mod_command("/mod help").unwrap(),
            ModCommand::Help { topic: None }
        );
        assert_eq!(
            parse_mod_command("help").unwrap(),
            ModCommand::Help { topic: None }
        );
        assert_eq!(
            parse_mod_command("help audit").unwrap(),
            ModCommand::Help {
                topic: Some("audit".to_string())
            }
        );
        assert_eq!(
            parse_mod_command("help server ban-ip").unwrap(),
            ModCommand::Help {
                topic: Some("server ban-ip".to_string())
            }
        );
        assert!(parse_mod_command("/moderator help").is_err());
    }

    #[test]
    fn command_help_explains_audit_filter() {
        let lines = mod_help_lines(Some("audit"));

        assert!(lines.iter().any(|line| line == "audit [filter]"));
        assert!(
            lines.iter().any(|line| line.contains("substring")),
            "audit help should explain the filter behavior: {lines:?}"
        );
    }

    #[test]
    fn command_help_explains_server_ban_arguments() {
        let lines = mod_help_lines(Some("server ban"));

        assert!(
            lines
                .iter()
                .any(|line| line == "server ban @name [duration] [reason...]")
        );
        assert!(
            lines.iter().any(|line| line.contains("s/m/h/d")),
            "server ban help should explain duration syntax: {lines:?}"
        );
    }

    #[test]
    fn parses_ban_list_commands() {
        assert_eq!(
            parse_mod_command("server bans").unwrap(),
            ModCommand::ServerBans
        );
        assert_eq!(
            parse_mod_command("artboard bans").unwrap(),
            ModCommand::ArtboardBans
        );
        assert_eq!(
            parse_mod_command("room bans #lobby").unwrap(),
            ModCommand::RoomBans {
                slug: "lobby".to_string()
            }
        );
    }

    #[test]
    fn normalizes_room_slugs_like_chat_rooms() {
        assert_eq!(normalize_mod_slug("#Rust_Nerds").unwrap(), "rust-nerds");
        assert_eq!(normalize_mod_slug("vps/d9d0").unwrap(), "vps-d9d0");
        assert!(normalize_mod_slug("!!!").is_err());
    }

    #[test]
    fn parses_room_ban_with_duration_and_reason() {
        assert_eq!(
            parse_mod_command("room ban #lobby @alice 7d cleanup").unwrap(),
            ModCommand::RoomAction {
                action: RoomModAction::Ban,
                slug: "lobby".to_string(),
                username: "alice".to_string(),
                duration: Some(chrono::Duration::days(7)),
                reason: "cleanup".to_string(),
            }
        );
    }

    #[test]
    fn parses_server_permanent_ban_without_duration() {
        assert_eq!(
            parse_mod_command("server ban @alice policy").unwrap(),
            ModCommand::ServerUser {
                action: ServerUserAction::Ban,
                username: "alice".to_string(),
                duration: None,
                reason: "policy".to_string(),
            }
        );
    }

    #[test]
    fn parses_server_kick() {
        assert_eq!(
            parse_mod_command("server kick @alice go outside").unwrap(),
            ModCommand::ServerUser {
                action: ServerUserAction::Kick,
                username: "alice".to_string(),
                duration: None,
                reason: "go outside".to_string(),
            }
        );
        assert!(parse_mod_command("server disconnect @alice").is_err());
    }

    #[test]
    fn parses_server_ip_ban_with_duration_and_reason() {
        assert_eq!(
            parse_mod_command("server ban-ip 203.0.113.10 2h subnet abuse").unwrap(),
            ModCommand::ServerIp {
                action: ServerIpAction::Ban,
                ip_address: "203.0.113.10".to_string(),
                duration: Some(chrono::Duration::hours(2)),
                reason: "subnet abuse".to_string(),
            }
        );
        assert_eq!(
            parse_mod_command("server unban-ip 2001:db8::1").unwrap(),
            ModCommand::ServerIp {
                action: ServerIpAction::Unban,
                ip_address: "2001:db8::1".to_string(),
                duration: None,
                reason: String::new(),
            }
        );
        assert!(parse_mod_command("server ban-ip nope").is_err());
    }
}
