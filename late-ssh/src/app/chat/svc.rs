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
        character_sheet::{CharacterSheet, CharacterSheetParams},
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_reaction::{
            ChatMessageReaction, ChatMessageReactionOwners, ChatMessageReactionSummary,
        },
        chat_poll::{self, ActiveChatPoll, CreateChatPoll},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        chat_slow_mode::ChatSlowMode,
        moderation_audit_log::ModerationAuditLog,
        room_ban::RoomBan,
        user::User,
        voice_channel::{TARGET_CHAT_ROOM, VoiceChannel},
    },
};
use serde_json::json;
use tokio::sync::{Semaphore, broadcast, mpsc, watch};
use tracing::{Instrument, info_span};

use crate::app::bonsai::state::stage_for;
use crate::app::games::chips::svc::ChipService;
use crate::authz::{Caps, Permissions, Tier};
use crate::ircd::registry::IrcRegistry;
use crate::metrics;
use crate::moderation::event::ModerationEvent;
use crate::moderation::service::{
    ModerationInfra, ModerationService, ensure_message_permission, target_tier_for_user_id,
};
use crate::moderation::session_effects::ModerationSessionEffects;
use crate::session::SessionRegistry;
use crate::state::ActiveUsers;
use crate::usernames::UsernameDirectory;

use super::commands::RoomScopedCommand;

/// Messages before and after a search hit, plus a username lookup for their
/// authors, as loaded by `load_message_context_task`.
type MessageContext = (Vec<ChatMessage>, Vec<ChatMessage>, HashMap<Uuid, String>);

const HISTORY_LIMIT: i64 = 500;
const DELTA_LIMIT: i64 = 256;
const PINNED_MESSAGES_LIMIT: i64 = 100;
const CHAT_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const USERNAME_DIRECTORY_TTL: Duration = Duration::from_secs(30);
const POLL_FINALIZER_RECOVERY_INTERVAL: Duration = Duration::from_secs(10 * 60);
const POLL_FINALIZER_BATCH_LIMIT: i64 = 25;
pub(crate) const GIFT_MAX_AMOUNT: i64 = 1_000_000;
const GIFT_COOLDOWN: Duration = Duration::from_secs(30);
const SEARCH_RESULTS_LIMIT: i64 = 50;
/// Minimum query length before a message search fires; also the trigram
/// index floor, so shorter queries would seq-scan anyway.
pub(crate) const SEARCH_MIN_CHARS: usize = 3;
/// Messages fetched on each side of a selected search hit for the modal's
/// context window.
const SEARCH_CONTEXT_EACH_SIDE: i64 = 4;

/// The two report-only rooms. `#bugs` and `#suggestions` accept only
/// `/bug` / `/suggest` report cards from regular users; free-text posting is
/// reserved for staff so reports don't drown in conversation. A report is a
/// normal chat message whose body starts with the kind's marker (same trick
/// as `---NEWS---` cards), so reactions, replies, pins, and deletes all work
/// on it unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReportKind {
    Bug,
    Suggestion,
}

impl ReportKind {
    pub(crate) const fn slug(self) -> &'static str {
        match self {
            Self::Bug => "bugs",
            Self::Suggestion => "suggestions",
        }
    }

    pub(crate) const fn marker(self) -> &'static str {
        match self {
            Self::Bug => "---BUG---",
            Self::Suggestion => "---SUGGESTION---",
        }
    }

    pub(crate) const fn command(self) -> &'static str {
        match self {
            Self::Bug => "/bug",
            Self::Suggestion => "/suggest",
        }
    }

    pub(crate) const fn icon(self) -> &'static str {
        match self {
            Self::Bug => "🐛",
            Self::Suggestion => "💡",
        }
    }

    /// Verb phrase for the card header: `<author> filed a bug <stamp>`.
    pub(crate) const fn verb(self) -> &'static str {
        match self {
            Self::Bug => "filed a bug",
            Self::Suggestion => "made a suggestion",
        }
    }

    /// The report kind a room slug enforces, if any.
    pub(crate) fn for_room_slug(slug: &str) -> Option<Self> {
        match slug {
            "bugs" => Some(Self::Bug),
            "suggestions" => Some(Self::Suggestion),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct ChatService {
    db: Db,
    username_tx: watch::Sender<Arc<Vec<String>>>,
    username_rx: watch::Receiver<Arc<Vec<String>>>,
    evt_tx: broadcast::Sender<ChatEvent>,
    moderation_event_tx: broadcast::Sender<ModerationEvent>,
    notification_svc: super::notifications::svc::NotificationService,
    active_users: Option<ActiveUsers>,
    username_directory: Option<UsernameDirectory>,
    session_registry: Option<SessionRegistry>,
    irc_registry: Option<IrcRegistry>,
    moderation_infra: ModerationInfra,
    chip_service: Option<ChipService>,
    gift_cooldowns: Arc<Mutex<HashMap<Uuid, std::time::Instant>>>,
    /// Last time each user posted a message containing a link. Drives the
    /// account-age link cooldown (blunts fresh-account spam-and-leave). Keyed by
    /// user, so it survives reconnects; holds at most one entry per link-poster.
    link_last_sent: Arc<Mutex<HashMap<Uuid, std::time::Instant>>>,
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
    /// A snapshot of the room's most recent messages, oldest-first, captured at
    /// list-load time so the discover preview pane can render instantly while
    /// scrolling. Empty when the room has no messages yet.
    pub recent: Vec<PreviewMessage>,
}

/// One line of a room's recent activity, shown in the discover preview pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewMessage {
    pub author: String,
    pub body: String,
    pub created: DateTime<Utc>,
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

pub struct SendLoungeMessageTask {
    pub user_id: Uuid,
    pub body: String,
    pub request_id: Option<Uuid>,
    pub join_if_needed: bool,
    pub failure_log: &'static str,
}

/// Fully-resolved inputs for persisting a single chat message.
struct SendMessageParams {
    user_id: Uuid,
    room_id: Uuid,
    room_slug: Option<String>,
    body: String,
    reply_to_message_id: Option<Uuid>,
    reply_to_user_id: Option<Uuid>,
    is_admin: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomMemberListItem {
    pub user_id: Uuid,
    pub username: Option<String>,
}

fn send_error_message(error: &anyhow::Error) -> String {
    let error = error.to_string();
    if error.contains("not a member") {
        "You are not a member of this room.".to_string()
    } else if error.contains("banned from this room") {
        "You are banned from this room.".to_string()
    } else if error.contains("admin-only") {
        "Only admins can post in #announcements.".to_string()
    } else if let Some(slug) = error.strip_prefix("report-only:") {
        let command = if slug == "suggestions" {
            "/suggest"
        } else {
            "/bug"
        };
        format!(
            "#{slug} takes reports only: post with {command} <text>, or react to an existing one."
        )
    } else if let Some(rest) = error.strip_prefix("slow-mode:") {
        let mut parts = rest.splitn(2, ':');
        let secs = parts
            .next()
            .and_then(|secs| secs.parse::<u64>().ok())
            .unwrap_or(1);
        let room = parts
            .next()
            .filter(|room| !room.is_empty())
            .map(|room| {
                if room == "server" {
                    "server".to_string()
                } else {
                    format!("#{room}")
                }
            })
            .unwrap_or_else(|| "this room".to_string());
        format!(
            "Slow mode in {room}: wait {} before sending again.",
            format_cooldown(secs)
        )
    } else if let Some(secs) = error.strip_prefix("link-cooldown:") {
        let secs = secs.parse::<u64>().unwrap_or(0);
        format!(
            "🔗 New accounts can only post a link occasionally — next link in {}.",
            format_cooldown(secs)
        )
    } else {
        "Could not send message. Please try again.".to_string()
    }
}

/// Render a remaining cooldown as a compact human string, e.g. `29m 30s`, `45s`.
fn format_cooldown(secs: u64) -> String {
    let secs = secs.max(1);
    let minutes = secs / 60;
    let seconds = secs % 60;
    if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

async fn slow_mode_remaining_for_mode(
    client: &tokio_postgres::Client,
    room_id: Uuid,
    user_id: Uuid,
    slow_mode: ChatSlowMode,
) -> Result<Option<Duration>> {
    let Some(last_sent) = ChatMessage::last_sent_at_in_room(client, room_id, user_id).await? else {
        return Ok(None);
    };

    let elapsed = Utc::now()
        .signed_duration_since(last_sent)
        .num_seconds()
        .max(0);
    let remaining = i64::from(slow_mode.interval_secs) - elapsed;
    if remaining > 0 {
        Ok(Some(Duration::from_secs(remaining as u64)))
    } else {
        Ok(None)
    }
}

async fn server_slow_mode_remaining_for_mode(
    client: &tokio_postgres::Client,
    user_id: Uuid,
    slow_mode: ChatSlowMode,
) -> Result<Option<Duration>> {
    let Some(last_sent) = ChatMessage::last_sent_at_in_public_rooms(client, user_id).await? else {
        return Ok(None);
    };

    let elapsed = Utc::now()
        .signed_duration_since(last_sent)
        .num_seconds()
        .max(0);
    let remaining = i64::from(slow_mode.interval_secs) - elapsed;
    if remaining > 0 {
        Ok(Some(Duration::from_secs(remaining as u64)))
    } else {
        Ok(None)
    }
}

async fn slow_mode_remaining(
    client: &tokio_postgres::Client,
    room: &ChatRoom,
    user_id: Uuid,
) -> Result<Option<(Duration, &'static str)>> {
    if let Some(slow_mode) =
        ChatSlowMode::find_active_for_room_and_user(client, room.id, user_id).await?
        && let Some(remaining) =
            slow_mode_remaining_for_mode(client, room.id, user_id, slow_mode).await?
    {
        return Ok(Some((remaining, "room")));
    }

    if room.kind != "dm"
        && let Some(slow_mode) = ChatSlowMode::find_active_server_for_user(client, user_id).await?
        && let Some(remaining) =
            server_slow_mode_remaining_for_mode(client, user_id, slow_mode).await?
    {
        return Ok(Some((remaining, "server")));
    }

    Ok(None)
}

/// Account-age tiers for the chat link rate limit. An account under a day old may
/// only post a link every 30 minutes; under a week, every 5 minutes; older
/// accounts are unlimited.
const LINK_TIER_YOUNG_SECS: i64 = 24 * 60 * 60; // 1 day
const LINK_TIER_ESTABLISHED_SECS: i64 = 7 * 24 * 60 * 60; // 7 days
const LINK_COOLDOWN_FRESH: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const LINK_COOLDOWN_YOUNG: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// The link cooldown for an account of the given age, or `None` if the account is
/// established enough (7d+) to post links freely.
fn link_cooldown_for_age(age_secs: i64) -> Option<std::time::Duration> {
    if age_secs >= LINK_TIER_ESTABLISHED_SECS {
        None
    } else if age_secs >= LINK_TIER_YOUNG_SECS {
        Some(LINK_COOLDOWN_YOUNG)
    } else {
        Some(LINK_COOLDOWN_FRESH)
    }
}

/// Common TLDs used to spot a bare-domain link (one with no http/www scheme).
const LINK_TLDS: &[&str] = &[
    ".com", ".net", ".org", ".io", ".gg", ".xyz", ".co", ".me", ".tv", ".link", ".app", ".dev",
    ".info", ".biz", ".online", ".site", ".shop", ".ru", ".cn", ".to", ".ly", ".ai",
];

/// Whether a message body contains anything that looks like a clickable link.
/// Catches `http(s)://`, `www.`, and bare `domain.tld` forms so a link can't slip
/// through without a scheme.
fn contains_link(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        return true;
    }
    LINK_TLDS.iter().any(|tld| {
        lower.match_indices(tld).any(|(i, _)| {
            // Require an alphanumeric host char before the dot and a boundary
            // after the TLD, so "x.com" and "buy.io/now" match but "etc." does not.
            let before = lower[..i].chars().last();
            let after = lower[i + tld.len()..].chars().next();
            before.is_some_and(|c| c.is_alphanumeric())
                && after.is_none_or(|c| !c.is_alphanumeric())
        })
    })
}

fn poll_error_message(error: &anyhow::Error) -> String {
    let text = error.to_string();
    if text.contains("already has an active poll")
        || text.contains("at least two options")
        || text.contains("at most three options")
        || text.contains("too long")
        || text.contains("duration must")
        || text.contains("question is required")
        || text.contains("join the room")
        || text.contains("no longer available")
        || text.contains("invalid poll option")
    {
        service_sentence_case(&text)
    } else {
        "Could not update poll".to_string()
    }
}

fn poll_vote_key(option_position: i32) -> String {
    match option_position {
        1 => "va".to_string(),
        2 => "vb".to_string(),
        3 => "vc".to_string(),
        _ => format!("v{option_position}"),
    }
}

fn format_poll_results_message(poll: &ActiveChatPoll) -> String {
    let total_votes = poll
        .options
        .iter()
        .map(|option| option.vote_count.max(0))
        .sum::<i64>();
    let mut lines = vec![
        "---POLL RESULTS---".to_string(),
        poll.poll.question.trim().to_string(),
    ];

    for option in &poll.options {
        let count = option.vote_count.max(0);
        let percent = if total_votes > 0 {
            ((count * 100 + total_votes / 2) / total_votes).clamp(0, 100)
        } else {
            0
        };
        lines.push(format!(
            "{}. {} - {} vote{} ({}%)",
            option.position,
            option.label.trim(),
            count,
            if count == 1 { "" } else { "s" },
            percent
        ));
    }

    match winning_poll_labels(poll, total_votes) {
        PollWinner::None => lines.push("Winner: no votes cast".to_string()),
        PollWinner::One(label) => lines.push(format!("Winner: {label}")),
        PollWinner::Tie(labels) => lines.push(format!("Tie: {}", labels.join(", "))),
    }

    lines.join("\n")
}

enum PollWinner {
    None,
    One(String),
    Tie(Vec<String>),
}

fn winning_poll_labels(poll: &ActiveChatPoll, total_votes: i64) -> PollWinner {
    if total_votes <= 0 {
        return PollWinner::None;
    }
    let winning_count = poll
        .options
        .iter()
        .map(|option| option.vote_count.max(0))
        .max()
        .unwrap_or(0);
    let labels = poll
        .options
        .iter()
        .filter(|option| option.vote_count.max(0) == winning_count)
        .map(|option| option.label.trim().to_string())
        .collect::<Vec<_>>();

    if labels.len() == 1 {
        PollWinner::One(labels.into_iter().next().unwrap_or_default())
    } else {
        PollWinner::Tie(labels)
    }
}

fn service_sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

#[derive(Clone)]
struct ChatRefreshSession {
    user_id: Uuid,
    snapshot_tx: watch::Sender<ChatSnapshot>,
    /// Point-to-point delivery for single-recipient events (tail loads,
    /// search results, discover lists). These carry full message payloads;
    /// sending them over the global broadcast would clone them into every
    /// connected session only to be filtered out by all but one.
    event_tx: mpsc::UnboundedSender<ChatEvent>,
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
    pub voice_channels_by_room_id: HashMap<Uuid, VoiceChannel>,
    pub message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub lounge_room_id: Option<Uuid>,
    pub usernames: HashMap<Uuid, String>,
    pub countries: HashMap<Uuid, String>,
    pub unread_counts: HashMap<Uuid, i64>,
    pub room_last_message_at: HashMap<Uuid, Option<DateTime<Utc>>>,
    pub active_polls: HashMap<Uuid, ActiveChatPoll>,
    pub bonsai_glyphs: HashMap<Uuid, String>,
    pub chat_badges: HashMap<Uuid, String>,
    pub profile_award_badges: HashMap<Uuid, String>,
    pub ignored_user_ids: Vec<Uuid>,
    pub friend_user_ids: Vec<Uuid>,
}

#[derive(Clone, Debug)]
pub enum ChatEvent {
    MessageCreated {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
        author_username: Option<String>,
        author_bonsai_glyph: Option<String>,
        author_chat_badge: Option<String>,
        author_profile_award_badges: Option<String>,
    },
    MessageEdited {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
        author_username: Option<String>,
        author_bonsai_glyph: Option<String>,
        author_chat_badge: Option<String>,
        author_profile_award_badges: Option<String>,
    },
    RoomTailLoaded {
        user_id: Uuid,
        room_id: Uuid,
        last_read_at: Option<DateTime<Utc>>,
        messages: Vec<ChatMessage>,
        message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
        usernames: HashMap<Uuid, String>,
        bonsai_glyphs: HashMap<Uuid, String>,
        chat_badges: HashMap<Uuid, String>,
        profile_award_badges: HashMap<Uuid, String>,
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
    MessageSearchLoaded {
        user_id: Uuid,
        request_id: Uuid,
        messages: Vec<ChatMessage>,
        usernames: HashMap<Uuid, String>,
    },
    MessageSearchFailed {
        user_id: Uuid,
        request_id: Uuid,
        message: String,
    },
    MessageContextLoaded {
        user_id: Uuid,
        request_id: Uuid,
        message_id: Uuid,
        before: Vec<ChatMessage>,
        after: Vec<ChatMessage>,
        usernames: HashMap<Uuid, String>,
    },
    MessageContextFailed {
        user_id: Uuid,
        request_id: Uuid,
        message_id: Uuid,
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
    OpenProfileResolved {
        user_id: Uuid,
        target_user_id: Uuid,
        target_username: String,
    },
    OpenProfileFailed {
        user_id: Uuid,
        message: String,
    },
    OpenSheetResolved {
        user_id: Uuid,
        room_id: Uuid,
        target_user_id: Uuid,
        target_username: String,
        name: String,
        body: String,
    },
    SheetError {
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
    MessageRemoved {
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
    FriendListUpdated {
        user_id: Uuid,
        friend_user_ids: Vec<Uuid>,
        target_user_id: Uuid,
        target_username: String,
        message: String,
    },
    RoomMembersListed {
        user_id: Uuid,
        title: String,
        members: Vec<RoomMemberListItem>,
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
    FriendFailed {
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
    PollUpdated {
        actor_user_id: Uuid,
        room_id: Uuid,
        poll: ActiveChatPoll,
        message: String,
    },
    PollStartAllowed {
        user_id: Uuid,
        room_id: Uuid,
    },
    PollFailed {
        user_id: Uuid,
        message: String,
    },
    GiftSucceeded {
        /// The sender's id.
        user_id: Uuid,
        sender_username: String,
        recipient_id: Uuid,
        recipient_username: String,
        amount: i64,
        sender_balance: i64,
        recipient_balance: i64,
        /// Optional note the sender attached to the gift.
        message: Option<String>,
    },
    GiftFailed {
        user_id: Uuid,
        message: String,
    },
}

/// Result of a successful chip gift, returned by `gift_chips`.
struct GiftOutcome {
    sender_username: String,
    recipient_id: Uuid,
    recipient_username: String,
    sender_balance: i64,
    recipient_balance: i64,
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
            username_directory: None,
            session_registry: None,
            irc_registry: None,
            moderation_infra: ModerationInfra::default(),
            chip_service: None,
            gift_cooldowns: Arc::new(Mutex::new(HashMap::new())),
            link_last_sent: Arc::new(Mutex::new(HashMap::new())),
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

    pub fn with_irc_registry(mut self, irc_registry: IrcRegistry) -> Self {
        self.irc_registry = Some(irc_registry);
        self
    }

    pub fn with_username_directory(mut self, username_directory: UsernameDirectory) -> Self {
        self.username_directory = Some(username_directory);
        self
    }

    pub fn with_force_admin(mut self, force_admin: bool) -> Self {
        self.moderation_infra = self.moderation_infra.with_force_admin(force_admin);
        self
    }

    pub fn with_moderation_infra(mut self, moderation_infra: ModerationInfra) -> Self {
        self.moderation_infra = moderation_infra;
        self
    }

    pub fn with_chip_service(mut self, chip_service: ChipService) -> Self {
        self.chip_service = Some(chip_service);
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

    pub fn start_poll_finalizer_recovery_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.finalize_expired_poll_batch().await {
                    late_core::error_span!(
                        "chat_poll_finalizer_recovery_failed",
                        error = ?e,
                        "failed to recover expired chat polls"
                    );
                }

                let mut interval = tokio::time::interval(POLL_FINALIZER_RECOVERY_INTERVAL);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;

                loop {
                    interval.tick().await;
                    if let Err(e) = service.finalize_expired_poll_batch().await {
                        late_core::error_span!(
                            "chat_poll_finalizer_recovery_failed",
                            error = ?e,
                            "failed to recover expired chat polls"
                        );
                    }
                }
            }
            .instrument(info_span!("chat.poll_finalizer_recovery")),
        )
    }

    fn moderation_session_effects(&self) -> ModerationSessionEffects {
        ModerationSessionEffects::new(
            self.active_users.clone(),
            self.username_directory.clone(),
            self.session_registry.clone(),
            self.irc_registry.clone(),
        )
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

    pub(crate) async fn run_mod_command(
        &self,
        user_id: Uuid,
        permissions: Permissions,
        command: &str,
    ) -> Result<Vec<String>> {
        self.moderation_service()
            .run_command(user_id, permissions, command)
            .await
    }

    fn moderation_service(&self) -> ModerationService {
        ModerationService::new(
            self.db.clone(),
            self.moderation_session_effects(),
            self.moderation_event_tx.clone(),
            self.moderation_infra.clone(),
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
        let room_ids: Vec<Uuid> = rooms.iter().map(|room| room.id).collect();
        let voice_channels_by_room_id =
            VoiceChannel::enabled_for_chat_rooms(&client, &room_ids).await?;
        let room_last_message_at =
            ChatMessage::last_message_at_for_rooms(&client, &room_ids).await?;
        let active_polls = chat_poll::list_active_polls_for_rooms(&client, user_id, &room_ids)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(error = ?error, user_id = %user_id, "failed to load active chat polls");
                HashMap::new()
            });
        let unread_counts = ChatRoomMember::unread_counts_for_user(&client, user_id).await?;
        let friend_user_ids = User::friend_user_ids(&client, user_id).await?;
        let lounge_room_id = rooms
            .iter()
            .find(|room| room.kind == "lounge" && room.slug.as_deref() == Some("lounge"))
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
        visible_user_ids.extend(friend_user_ids.iter().copied());
        visible_user_ids.sort();
        visible_user_ids.dedup();
        let author_metadata = Self::load_chat_author_metadata(&client, &visible_user_ids).await?;
        let ignored_user_ids = User::ignored_user_ids(&client, user_id).await?;

        let rooms = rooms.into_iter().map(|chat| (chat, Vec::new())).collect();

        Ok(ChatSnapshot {
            user_id: Some(user_id),
            chat_rooms: rooms,
            voice_channels_by_room_id,
            message_reactions: HashMap::new(),
            lounge_room_id,
            usernames: author_metadata.usernames,
            countries: HashMap::new(),
            unread_counts,
            room_last_message_at,
            active_polls,
            bonsai_glyphs: author_metadata.bonsai_glyphs,
            chat_badges: author_metadata.chat_badges,
            profile_award_badges: author_metadata.profile_award_badges,
            ignored_user_ids,
            friend_user_ids,
        })
    }

    async fn load_chat_author_metadata(
        client: &tokio_postgres::Client,
        user_ids: &[Uuid],
    ) -> Result<ChatAuthorMaps> {
        if user_ids.is_empty() {
            return Ok(ChatAuthorMaps::default());
        }

        let metadata = User::list_chat_author_metadata(client, user_ids).await?;

        let mut maps = ChatAuthorMaps {
            usernames: HashMap::with_capacity(metadata.len()),
            bonsai_glyphs: HashMap::new(),
            chat_badges: HashMap::new(),
            profile_award_badges: HashMap::new(),
        };
        for item in metadata {
            if !item.username.trim().is_empty() {
                maps.usernames.insert(item.user_id, item.username);
            }

            if item.dynamic_bonsai_selected {
                if let Some(glyph) = item
                    .bonsai_v2_badge_glyph
                    .as_deref()
                    .filter(|glyph| !glyph.is_empty())
                {
                    maps.bonsai_glyphs.insert(item.user_id, glyph.to_string());
                }
            } else if let (Some(is_alive), Some(growth_points)) =
                (item.bonsai_is_alive, item.bonsai_growth_points)
            {
                let glyph = stage_for(is_alive, growth_points).glyph();
                if !glyph.is_empty() {
                    maps.bonsai_glyphs.insert(item.user_id, glyph.to_string());
                }
            }

            if let Some(badge) = chat_author_badge(item.chat_flag, item.chat_badge) {
                maps.chat_badges.insert(item.user_id, badge);
            }
            if let Some(badge) = item
                .profile_award_badges
                .filter(|badge| !badge.trim().is_empty())
            {
                maps.profile_award_badges.insert(item.user_id, badge);
            }
        }

        Ok(maps)
    }
}

#[derive(Default)]
struct ChatAuthorMaps {
    usernames: HashMap<Uuid, String>,
    bonsai_glyphs: HashMap<Uuid, String>,
    chat_badges: HashMap<Uuid, String>,
    profile_award_badges: HashMap<Uuid, String>,
}

fn chat_author_badge(flag: Option<String>, badge: Option<String>) -> Option<String> {
    let joined = [flag, badge]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

impl ChatService {
    async fn list_all_discover_rooms(
        client: &tokio_postgres::Client,
    ) -> Result<Vec<DiscoverRoomItem>> {
        let rows = ChatRoom::list_discover_public_topic_rooms(client).await?;

        Ok(rows
            .into_iter()
            .map(|row| DiscoverRoomItem {
                room_id: row.room_id,
                slug: row.slug,
                member_count: row.member_count,
                message_count: row.message_count,
                last_message_at: row.last_message_at,
                recent: Vec::new(),
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
        mpsc::UnboundedReceiver<ChatEvent>,
        mpsc::UnboundedSender<()>,
        tokio::task::AbortHandle,
    ) {
        self.ensure_refresh_scheduler();

        let session_id = Uuid::now_v7();
        let (snapshot_tx, snapshot_rx) = watch::channel(ChatSnapshot::default());
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (force_refresh_tx, mut force_refresh_rx) = mpsc::unbounded_channel();
        let initial_room_id = *room_rx.borrow();
        self.refresh_sessions.lock_recover().insert(
            session_id,
            ChatRefreshSession {
                user_id,
                snapshot_tx,
                event_tx,
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
        (
            snapshot_rx,
            event_rx,
            force_refresh_tx,
            handle.abort_handle(),
        )
    }

    /// Deliver a single-recipient event to every registered session of
    /// `user_id` (a user can hold several SSH sessions). Events for users
    /// with no live session are dropped, same as an unobserved broadcast.
    fn send_user_event(&self, user_id: Uuid, event: ChatEvent) {
        let sessions: Vec<mpsc::UnboundedSender<ChatEvent>> = self
            .refresh_sessions
            .lock_recover()
            .values()
            .filter(|session| session.user_id == user_id)
            .map(|session| session.event_tx.clone())
            .collect();
        // Common case is one session; move the payload instead of cloning it.
        if sessions.len() == 1 {
            let _ = sessions[0].send(event);
            return;
        }
        for session in &sessions {
            let _ = session.send(event.clone());
        }
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

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id, read_at = %read_at))]
    async fn mark_room_read_at(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        read_at: DateTime<Utc>,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let count = ChatRoomMember::mark_read_at(&client, room_id, user_id, read_at).await?;
        if count == 0 {
            anyhow::bail!("user is not a member of room");
        }
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

    pub fn mark_room_read_at_task(&self, user_id: Uuid, room_id: Uuid, read_at: DateTime<Utc>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.mark_room_read_at(user_id, room_id, read_at).await {
                    late_core::error_span!(
                        "chat_mark_read_failed",
                        error = ?e,
                        "failed to mark room read"
                    );
                }
            }
            .instrument(info_span!(
                "chat.mark_room_read_at_task",
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
            self.send_user_event(
                user_id,
                ChatEvent::DeltaSynced {
                    user_id,
                    room_id,
                    messages,
                },
            );
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
        let last_read_at = ChatRoomMember::last_read_at(&client, room_id, user_id).await?;

        let messages = ChatMessage::list_recent(&client, room_id, HISTORY_LIMIT).await?;
        let message_ids: Vec<Uuid> = messages.iter().map(|message| message.id).collect();
        let author_ids: Vec<Uuid> = messages.iter().map(|message| message.user_id).collect();
        let message_reactions =
            ChatMessageReaction::list_summaries_for_messages(&client, &message_ids).await?;
        let author_metadata = Self::load_chat_author_metadata(&client, &author_ids).await?;

        self.send_user_event(
            user_id,
            ChatEvent::RoomTailLoaded {
                user_id,
                room_id,
                last_read_at,
                messages,
                message_reactions,
                usernames: author_metadata.usernames,
                bonsai_glyphs: author_metadata.bonsai_glyphs,
                chat_badges: author_metadata.chat_badges,
                profile_award_badges: author_metadata.profile_award_badges,
            },
        );
        Ok(())
    }

    pub fn load_room_tail_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.load_room_tail(user_id, room_id).await {
                    service.send_user_event(
                        user_id,
                        ChatEvent::RoomTailLoadFailed { user_id, room_id },
                    );
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

    #[tracing::instrument(skip(self, query, exclude_user_ids), fields(user_id = %user_id))]
    async fn search_messages(
        &self,
        user_id: Uuid,
        room_id: Option<Uuid>,
        query: String,
        exclude_user_ids: Vec<Uuid>,
    ) -> Result<(Vec<ChatMessage>, HashMap<Uuid, String>)> {
        if query.chars().count() < SEARCH_MIN_CHARS {
            anyhow::bail!("search query too short");
        }
        let _permit = self.read_permits.acquire().await?;
        let client = self.db.get().await?;
        let messages = ChatMessage::search_for_user(
            &client,
            user_id,
            &query,
            room_id,
            &exclude_user_ids,
            SEARCH_RESULTS_LIMIT,
        )
        .await?;
        let author_ids: Vec<Uuid> = messages.iter().map(|message| message.user_id).collect();
        let usernames = User::list_usernames_by_ids(&client, &author_ids).await?;
        Ok((messages, usernames))
    }

    /// Load up to `SEARCH_CONTEXT_EACH_SIDE` messages either side of a search
    /// hit (`MessageContextLoaded`, user-targeted, keyed by request id) for
    /// the modal's detail-pane context window. Membership-gated; system-feed
    /// lines and the caller's ignored users are excluded.
    #[allow(clippy::too_many_arguments)]
    pub fn load_message_context_task(
        &self,
        user_id: Uuid,
        request_id: Uuid,
        room_id: Uuid,
        message_id: Uuid,
        created: DateTime<Utc>,
        exclude_user_ids: Vec<Uuid>,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result: Result<MessageContext> = async {
                    let _permit = service.read_permits.acquire().await?;
                    let client = service.db.get().await?;
                    // Same read boundary as `get_for_viewer`: members
                    // always, public non-game rooms for everyone (mention
                    // previews can reference rooms the user never joined).
                    if !ChatRoomMember::is_member(&client, room_id, user_id).await? {
                        let public_readable = ChatRoom::get(&client, room_id)
                            .await?
                            .is_some_and(|room| room.visibility == "public" && room.kind != "game");
                        if !public_readable {
                            anyhow::bail!("user cannot read this room");
                        }
                    }
                    let (before, after) = ChatMessage::list_around(
                        &client,
                        room_id,
                        created,
                        message_id,
                        &exclude_user_ids,
                        SEARCH_CONTEXT_EACH_SIDE,
                    )
                    .await?;
                    let author_ids: Vec<Uuid> = before
                        .iter()
                        .chain(after.iter())
                        .map(|message| message.user_id)
                        .collect();
                    let usernames = User::list_usernames_by_ids(&client, &author_ids).await?;
                    Ok((before, after, usernames))
                }
                .await;
                match result {
                    Ok((before, after, usernames)) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageContextLoaded {
                                user_id,
                                request_id,
                                message_id,
                                before,
                                after,
                                usernames,
                            },
                        );
                    }
                    Err(e) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageContextFailed {
                                user_id,
                                request_id,
                                message_id,
                            },
                        );
                        late_core::error_span!(
                            "chat_message_context_failed",
                            error = ?e,
                            "failed to load chat message context"
                        );
                    }
                }
            }
            .instrument(info_span!(
                "chat.load_message_context_task",
                user_id = %user_id,
                message_id = %message_id
            )),
        );
    }

    /// Fetch one membership-gated message as a single-hit search result
    /// (`MessageSearchLoaded`). Backs the Mentions fallback: previewing a
    /// mention whose message is older than the loaded room history.
    pub fn load_message_preview_task(&self, user_id: Uuid, request_id: Uuid, message_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result: Result<Option<(ChatMessage, HashMap<Uuid, String>)>> = async {
                    let _permit = service.read_permits.acquire().await?;
                    let client = service.db.get().await?;
                    let Some(message) =
                        ChatMessage::get_for_viewer(&client, message_id, user_id).await?
                    else {
                        return Ok(None);
                    };
                    let usernames =
                        User::list_usernames_by_ids(&client, &[message.user_id]).await?;
                    Ok(Some((message, usernames)))
                }
                .await;
                match result {
                    Ok(Some((message, usernames))) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageSearchLoaded {
                                user_id,
                                request_id,
                                messages: vec![message],
                                usernames,
                            },
                        );
                    }
                    Ok(None) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageSearchFailed {
                                user_id,
                                request_id,
                                message: "message is no longer available".to_string(),
                            },
                        );
                    }
                    Err(e) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageSearchFailed {
                                user_id,
                                request_id,
                                message: "preview failed, try again".to_string(),
                            },
                        );
                        late_core::error_span!(
                            "chat_message_preview_failed",
                            error = ?e,
                            "failed to load chat message preview"
                        );
                    }
                }
            }
            .instrument(info_span!(
                "chat.load_message_preview_task",
                user_id = %user_id,
                message_id = %message_id
            )),
        );
    }

    /// Fire-and-forget message search. Results come back as a user-targeted
    /// `MessageSearchLoaded` keyed by `request_id`; consumers drop results
    /// whose request id is no longer current (latest wins).
    pub fn search_messages_task(
        &self,
        user_id: Uuid,
        request_id: Uuid,
        room_id: Option<Uuid>,
        query: String,
        exclude_user_ids: Vec<Uuid>,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .search_messages(user_id, room_id, query, exclude_user_ids)
                    .await
                {
                    Ok((messages, usernames)) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageSearchLoaded {
                                user_id,
                                request_id,
                                messages,
                                usernames,
                            },
                        );
                    }
                    Err(e) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::MessageSearchFailed {
                                user_id,
                                request_id,
                                message: "search failed, try again".to_string(),
                            },
                        );
                        late_core::error_span!(
                            "chat_search_messages_failed",
                            error = ?e,
                            "failed to search chat messages"
                        );
                    }
                }
            }
            .instrument(info_span!(
                "chat.search_messages_task",
                user_id = %user_id,
                request_id = %request_id
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
        let mut rooms: Vec<DiscoverRoomItem> = Self::list_all_discover_rooms(&client)
            .await?
            .into_iter()
            .filter(|room| !joined_ids.contains(&room.room_id))
            .collect();

        Self::attach_recent_previews(&client, &mut rooms).await?;
        Ok(rooms)
    }

    /// Fetch a snapshot of each room's most recent messages and attach them as
    /// the `recent` preview, so the discover UI can render a preview pane
    /// instantly while the user scrolls. Best-effort: a preview-fetch failure is
    /// logged but leaves the rooms usable with empty previews.
    async fn attach_recent_previews(
        client: &tokio_postgres::Client,
        rooms: &mut [DiscoverRoomItem],
    ) -> Result<()> {
        const PREVIEW_MESSAGES_PER_ROOM: i64 = 5;

        let room_ids: Vec<Uuid> = rooms.iter().map(|room| room.room_id).collect();
        if room_ids.is_empty() {
            return Ok(());
        }

        let mut messages_by_room =
            ChatMessage::list_recent_for_rooms(client, &room_ids, PREVIEW_MESSAGES_PER_ROOM)
                .await?;

        let author_ids: Vec<Uuid> = messages_by_room
            .values()
            .flatten()
            .map(|msg| msg.user_id)
            .collect();
        let usernames = User::list_usernames_by_ids(client, &author_ids).await?;

        for room in rooms.iter_mut() {
            let Some(mut messages) = messages_by_room.remove(&room.room_id) else {
                continue;
            };
            // `list_recent_for_rooms` returns newest-first; flip to chronological
            // so the preview reads top-to-bottom like a normal chat transcript.
            messages.reverse();
            room.recent = messages
                .into_iter()
                .map(|msg| PreviewMessage {
                    author: usernames
                        .get(&msg.user_id)
                        .cloned()
                        .unwrap_or_else(|| "someone".to_string()),
                    body: msg.body,
                    created: msg.created,
                })
                .collect();
        }

        Ok(())
    }

    pub fn list_discover_rooms_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service.list_discover_rooms(user_id).await {
                    Ok(rooms) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::DiscoverRoomsLoaded { user_id, rooms },
                        );
                    }
                    Err(e) => {
                        service.send_user_event(
                            user_id,
                            ChatEvent::DiscoverRoomsFailed {
                                user_id,
                                message: "Could not load public rooms.".to_string(),
                            },
                        );
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

    pub fn check_poll_start_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    chat_poll::ensure_can_start_poll(&client, user_id, room_id).await
                }
                .await;
                match result {
                    Ok(()) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::PollStartAllowed { user_id, room_id });
                    }
                    Err(error) => {
                        let _ = service.evt_tx.send(ChatEvent::PollFailed {
                            user_id,
                            message: poll_error_message(&error),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.check_poll_start_task",
                user_id = %user_id,
                room_id = %room_id
            )),
        );
    }

    pub fn create_poll_task(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        question: String,
        options: Vec<String>,
        duration_secs: i64,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let mut client = service.db.get().await?;
                    chat_poll::create_poll(
                        &mut client,
                        CreateChatPoll {
                            user_id,
                            room_id,
                            question,
                            options,
                            duration_secs,
                        },
                    )
                    .await
                }
                .await;
                match result {
                    Ok(poll) => {
                        service.schedule_poll_finalizer(poll.poll.id, poll.poll.ends_at);
                        let _ = service.evt_tx.send(ChatEvent::PollUpdated {
                            actor_user_id: user_id,
                            room_id,
                            poll,
                            message: "Poll started".to_string(),
                        });
                        service.refresh_registered_sessions().await;
                    }
                    Err(error) => {
                        let _ = service.evt_tx.send(ChatEvent::PollFailed {
                            user_id,
                            message: poll_error_message(&error),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.create_poll_task",
                user_id = %user_id,
                room_id = %room_id
            )),
        );
    }

    fn schedule_poll_finalizer(&self, poll_id: Uuid, ends_at: DateTime<Utc>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let wait = (ends_at - Utc::now())
                    .to_std()
                    .unwrap_or(Duration::ZERO)
                    .saturating_add(Duration::from_millis(250));
                tokio::time::sleep(wait).await;
                if let Err(e) = service.finalize_expired_poll(poll_id).await {
                    late_core::error_span!(
                        "chat_poll_finalize_failed",
                        poll_id = %poll_id,
                        error = ?e,
                        "failed to finalize chat poll"
                    );
                }
            }
            .instrument(info_span!("chat.poll_finalizer", poll_id = %poll_id)),
        );
    }

    async fn finalize_expired_poll_batch(&self) -> Result<usize> {
        let client = self.db.get().await?;
        let poll_ids =
            chat_poll::list_expired_active_poll_ids(&client, POLL_FINALIZER_BATCH_LIMIT).await?;
        drop(client);

        let mut finalized = 0;
        for poll_id in poll_ids {
            if self.finalize_expired_poll(poll_id).await? {
                finalized += 1;
            }
        }

        Ok(finalized)
    }

    async fn finalize_expired_poll(&self, poll_id: Uuid) -> Result<bool> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(poll) = chat_poll::claim_expired_poll(&tx, poll_id).await? else {
            return Ok(false);
        };
        let body = format_poll_results_message(&poll);
        let message = ChatMessageParams {
            room_id: poll.poll.room_id,
            user_id: poll.poll.user_id,
            body,
        };
        let chat = ChatMessage::create_with_reply_to(&tx, message, None).await?;
        ChatRoom::touch_updated(&tx, poll.poll.room_id).await?;
        tx.commit().await?;

        let target_user_ids = ChatRoom::get_target_user_ids(&client, poll.poll.room_id).await?;
        let mut author_metadata =
            Self::load_chat_author_metadata(&client, &[poll.poll.user_id]).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageCreated {
            message: chat,
            target_user_ids,
            author_username: author_metadata.usernames.remove(&poll.poll.user_id),
            author_bonsai_glyph: author_metadata.bonsai_glyphs.remove(&poll.poll.user_id),
            author_chat_badge: author_metadata.chat_badges.remove(&poll.poll.user_id),
            author_profile_award_badges: author_metadata
                .profile_award_badges
                .remove(&poll.poll.user_id),
        });
        self.refresh_registered_sessions().await;
        Ok(true)
    }

    pub fn cast_poll_vote_task(&self, user_id: Uuid, poll_id: Uuid, option_position: i32) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let mut client = service.db.get().await?;
                    chat_poll::cast_vote(&mut client, user_id, poll_id, option_position).await
                }
                .await;
                match result {
                    Ok(poll) => {
                        let room_id = poll.poll.room_id;
                        let _ = service.evt_tx.send(ChatEvent::PollUpdated {
                            actor_user_id: user_id,
                            room_id,
                            poll,
                            message: format!("Poll vote {}", poll_vote_key(option_position)),
                        });
                        service.refresh_registered_sessions().await;
                    }
                    Err(error) => {
                        let _ = service.evt_tx.send(ChatEvent::PollFailed {
                            user_id,
                            message: poll_error_message(&error),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.cast_poll_vote_task",
                user_id = %user_id,
                poll_id = %poll_id,
                option_position = option_position
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

    /// Post a `/bug` or `/suggest` report card into its report-only room,
    /// regardless of which room the composer was focused on. Joins the room
    /// first so a report never bounces on membership. Success/failure surfaces
    /// through the usual `SendSucceeded`/`SendFailed` banners.
    pub(crate) fn send_report_task(
        &self,
        user_id: Uuid,
        kind: ReportKind,
        text: String,
        request_id: Uuid,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service.send_report(user_id, kind, text).await {
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::SendSucceeded {
                            user_id,
                            request_id,
                        });
                    }
                    Err(e) => {
                        let message = send_error_message(&e);
                        let _ = service.evt_tx.send(ChatEvent::SendFailed {
                            user_id,
                            request_id,
                            message,
                        });
                        late_core::error_span!(
                            "chat_report_send_failed",
                            error = ?e,
                            "failed to send report"
                        );
                    }
                }
            }
            .instrument(info_span!(
                "chat.send_report_task",
                user_id = %user_id,
                slug = kind.slug(),
                request_id = %request_id
            )),
        );
    }

    async fn send_report(&self, user_id: Uuid, kind: ReportKind, text: String) -> Result<()> {
        let client = self.db.get().await?;
        // Resolve only the public report room. Slugs are not globally unique
        // (a private topic room or a game room may share one), so a loose
        // slug lookup could leak the report into, and join the reporter to,
        // whichever same-slug row happened to come back first.
        let room = ChatRoom::find_topic_room(&client, "public", kind.slug())
            .await?
            .ok_or_else(|| anyhow::anyhow!("room #{} not found", kind.slug()))?;
        ChatRoomMember::join(&client, room.id, user_id).await?;
        drop(client);

        self.send_message(SendMessageParams {
            user_id,
            room_id: room.id,
            room_slug: Some(kind.slug().to_string()),
            body: format!("{} {}", kind.marker(), text),
            reply_to_message_id: None,
            reply_to_user_id: None,
            is_admin: false,
        })
        .await
    }

    /// Send a bot/automated reply that is a response to `reply_to_user_id`.
    /// Recording the triggering user lets each viewer hide the reply when they
    /// ignore that user, so ignored users cannot use a bot to be heard.
    pub fn send_bot_reply_task(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        body: String,
        reply_to_user_id: Option<Uuid>,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .send_message(SendMessageParams {
                        user_id,
                        room_id,
                        room_slug: None,
                        body,
                        reply_to_message_id: None,
                        reply_to_user_id,
                        is_admin: false,
                    })
                    .await
                {
                    late_core::error_span!(
                        "chat_bot_send_failed",
                        error = ?e,
                        "failed to send bot reply"
                    );
                }
            }
            .instrument(info_span!(
                "chat.send_bot_reply_task",
                user_id = %user_id,
                room_id = %room_id,
            )),
        );
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
                    .send_message(SendMessageParams {
                        user_id,
                        room_id,
                        room_slug,
                        body,
                        reply_to_message_id,
                        reply_to_user_id: None,
                        is_admin,
                    })
                    .await
                {
                    Err(e) => {
                        let message = send_error_message(&e);
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

    pub fn send_lounge_message_task(&self, task: SendLoungeMessageTask) {
        let SendLoungeMessageTask {
            user_id,
            body,
            request_id,
            join_if_needed,
            failure_log,
        } = task;
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .send_lounge_message(user_id, body, join_if_needed)
                    .await
                {
                    Ok(()) => {
                        if let Some(request_id) = request_id {
                            let _ = service.evt_tx.send(ChatEvent::SendSucceeded {
                                user_id,
                                request_id,
                            });
                        }
                    }
                    Err(e) => {
                        if let Some(request_id) = request_id {
                            let message = send_error_message(&e);
                            let _ = service.evt_tx.send(ChatEvent::SendFailed {
                                user_id,
                                request_id,
                                message: message.to_string(),
                            });
                        }
                        tracing::warn!(error = ?e, %user_id, failure_log);
                    }
                }
            }
            .instrument(info_span!("chat.send_lounge_message_task", user_id = %user_id)),
        );
    }

    async fn send_lounge_message(
        &self,
        user_id: Uuid,
        body: String,
        join_if_needed: bool,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let room = ChatRoom::find_lounge(&client)
            .await?
            .ok_or_else(|| anyhow::anyhow!("lounge room not found"))?;
        if join_if_needed {
            ChatRoomMember::join(&client, room.id, user_id).await?;
        }
        drop(client);

        self.send_message(SendMessageParams {
            user_id,
            room_id: room.id,
            room_slug: Some("lounge".to_string()),
            body,
            reply_to_message_id: None,
            reply_to_user_id: None,
            is_admin: false,
        })
        .await
    }

    /// How much longer `user_id` must wait before posting another link, or `None`
    /// if they may post one now. Records the send time when it returns `None`.
    /// Established accounts (7d+) always return `None`.
    async fn link_cooldown_remaining(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
    ) -> Result<Option<std::time::Duration>> {
        let age = User::account_age_seconds(client, user_id)
            .await?
            .unwrap_or(i64::MAX);
        let Some(cooldown) = link_cooldown_for_age(age) else {
            return Ok(None);
        };
        let now = std::time::Instant::now();
        let mut last_sent = self.link_last_sent.lock_recover();
        if let Some(prev) = last_sent.get(&user_id) {
            let elapsed = now.duration_since(*prev);
            if elapsed < cooldown {
                return Ok(Some(cooldown - elapsed));
            }
        }
        last_sent.insert(user_id, now);
        Ok(None)
    }

    #[tracing::instrument(skip(self, params), fields(user_id = %params.user_id, room_id = %params.room_id, body_len = params.body.len()))]
    async fn send_message(&self, params: SendMessageParams) -> Result<()> {
        let SendMessageParams {
            user_id,
            room_id,
            room_slug,
            body,
            reply_to_message_id,
            reply_to_user_id,
            is_admin,
        } = params;
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
        let room = ChatRoom::get(&client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("room not found"))?;
        // Report-only rooms: regular users may only post report cards (bodies
        // built by /bug and /suggest); staff keep free text so they can reply
        // under a report. Checked against the DB slug so the IRC path is
        // covered too. The staff lookup only runs on this rare gated path.
        if !is_admin
            && let Some(kind) = room.slug.as_deref().and_then(ReportKind::for_room_slug)
            && !body.trim_start().starts_with(kind.marker())
        {
            let is_moderator = User::staff_flags_by_ids(&client, &[user_id])
                .await?
                .get(&user_id)
                .is_some_and(|(_, is_moderator)| *is_moderator);
            if !is_moderator {
                anyhow::bail!("report-only:{}", kind.slug());
            }
        }
        if !is_admin
            && let Some((remaining, scope)) = slow_mode_remaining(&client, &room, user_id).await?
        {
            let label = if scope == "server" {
                "server".to_string()
            } else {
                room_slug.clone().unwrap_or_default()
            };
            anyhow::bail!("slow-mode:{}:{}", remaining.as_secs(), label);
        }

        // Account-age link rate limit: younger accounts can only post a link
        // every so often, to blunt spam-and-leave without silencing them. Old
        // (7d+) accounts and admins are unlimited. The age lookup only runs when
        // a non-admin message actually contains a link, which is rare.
        if !is_admin
            && contains_link(body)
            && let Some(remaining) = self.link_cooldown_remaining(&client, user_id).await?
        {
            anyhow::bail!("link-cooldown:{}", remaining.as_secs());
        }

        if let Some(reply_to_message_id) = reply_to_message_id {
            let reply_target = ChatMessage::get(&client, reply_to_message_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("reply target not found"))?;
            if reply_target.room_id != room_id {
                anyhow::bail!("reply target is not in this room");
            }
        }
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
        let chat = ChatMessage::create_with_reply_targets(
            &client,
            message,
            reply_to_message_id,
            reply_to_user_id,
        )
        .await?;
        ChatRoom::touch_updated(&client, room_id).await?;
        ChatRoomMember::mark_read_now(&client, room_id, user_id).await?;
        let target_user_ids = ChatRoom::get_target_user_ids(&client, room_id).await?;
        let mut author_metadata = Self::load_chat_author_metadata(&client, &[user_id]).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageCreated {
            message: chat.clone(),
            target_user_ids,
            author_username: author_metadata.usernames.remove(&user_id),
            author_bonsai_glyph: author_metadata.bonsai_glyphs.remove(&user_id),
            author_chat_badge: author_metadata.chat_badges.remove(&user_id),
            author_profile_award_badges: author_metadata.profile_award_badges.remove(&user_id),
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
                        } else if e.to_string().starts_with("report-only:") {
                            "Reports here must keep their marker; edit the text after it."
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

        // Report-only rooms: a regular user's only messages there are their own
        // report cards, and an edit must not strip the marker — otherwise
        // editing would bypass the free-text send gate.
        if !permissions.is_admin() && !permissions.is_moderator() {
            let room = ChatRoom::get(&client, existing.room_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("room not found"))?;
            if let Some(kind) = room.slug.as_deref().and_then(ReportKind::for_room_slug)
                && !new_body.trim_start().starts_with(kind.marker())
            {
                anyhow::bail!("report-only:{}", kind.slug());
            }
        }

        let tx = client.transaction().await?;
        let updated = ChatMessage::edit_after_authorization(&tx, message_id, new_body).await?;
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(is_owner),
            user_id,
            "message_edit",
            "message",
            Some(message_id),
            json!({ "room_id": existing.room_id }),
        )
        .await?;
        tx.commit().await?;
        let target_user_ids = ChatRoom::get_target_user_ids(&client, existing.room_id).await?;
        let mut author_metadata =
            Self::load_chat_author_metadata(&client, &[existing.user_id]).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageEdited {
            message: updated,
            target_user_ids,
            author_username: author_metadata.usernames.remove(&existing.user_id),
            author_bonsai_glyph: author_metadata.bonsai_glyphs.remove(&existing.user_id),
            author_chat_badge: author_metadata.chat_badges.remove(&existing.user_id),
            author_profile_award_badges: author_metadata
                .profile_award_badges
                .remove(&existing.user_id),
        });
        metrics::record_chat_message_edited();
        Ok(())
    }

    pub fn toggle_message_reaction_task(&self, user_id: Uuid, message_id: Uuid, icon: String) {
        let service = self.clone();
        let span_icon = icon.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .toggle_message_reaction(user_id, message_id, &icon)
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
                icon = %span_icon
            )),
        );
    }

    pub fn toggle_message_pin_task(
        &self,
        message_id: Uuid,
        is_admin: bool,
        pinned_tx: watch::Sender<Vec<ChatMessage>>,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result: Result<Vec<ChatMessage>> = async {
                    if !is_admin {
                        anyhow::bail!("admin-only");
                    }
                    let client = service.db.get().await?;
                    let message = ChatMessage::get(&client, message_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("message not found"))?;
                    ChatMessage::set_pinned(&client, message_id, !message.pinned).await?;
                    let pinned = ChatMessage::list_pinned(&client, PINNED_MESSAGES_LIMIT).await?;
                    Ok(pinned)
                }
                .await;
                match result {
                    Ok(pinned) => {
                        let _ = pinned_tx.send(pinned);
                    }
                    Err(e) => late_core::error_span!(
                        "chat_pin_failed",
                        error = ?e,
                        "failed to toggle message pin"
                    ),
                }
            }
            .instrument(info_span!(
                "chat.toggle_message_pin_task",
                message_id = %message_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, message_id = %message_id, icon = %icon))]
    async fn toggle_message_reaction(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        icon: &str,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let message = ChatMessage::get(&client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("message not found"))?;
        let is_member = ChatRoomMember::is_member(&client, message.room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        ChatMessageReaction::toggle(&client, message_id, user_id, icon).await?;
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
        VoiceChannel::upsert_for_target(&client, TARGET_CHAT_ROOM, room.id, "dm", true).await?;
        Ok(room.id)
    }

    pub fn open_profile_by_username_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span = info_span!(
            "chat.open_profile_by_username_task",
            user_id = %user_id,
            target = %target_username
        );
        tokio::spawn(
            async move {
                match service.resolve_profile_target(&target_username).await {
                    Ok((target_user_id, name)) => {
                        let _ = service.evt_tx.send(ChatEvent::OpenProfileResolved {
                            user_id,
                            target_user_id,
                            target_username: name,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::OpenProfileFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn resolve_profile_target(&self, target_username: &str) -> Result<(Uuid, String)> {
        let client = self.db.get().await?;
        let target = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user '{}' not found", target_username))?;
        Ok((target.id, target.username))
    }

    /// Resolve `/sheet [username]`: fetch the target's sheet for `room_id` and
    /// emit `OpenSheetResolved`, or `SheetError` when the target is unknown or
    /// (for other users only) has no sheet yet. `None` targets the caller; a
    /// missing own sheet resolves to an empty draft so the modal opens
    /// editable.
    pub fn open_sheet_task(&self, user_id: Uuid, room_id: Uuid, target_username: Option<String>) {
        let service = self.clone();
        let span = info_span!(
            "chat.open_sheet_task",
            user_id = %user_id,
            room_id = %room_id,
        );
        tokio::spawn(
            async move {
                match service
                    .resolve_sheet(user_id, room_id, target_username)
                    .await
                {
                    Ok(event) => {
                        let _ = service.evt_tx.send(event);
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::SheetError {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn resolve_sheet(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        target_username: Option<String>,
    ) -> Result<ChatEvent> {
        let client = self.db.get().await?;
        let room = self
            .ensure_room_scoped_command_access(&client, user_id, room_id, RoomScopedCommand::Sheet)
            .await?;
        let (target_user_id, target_username) = match target_username {
            Some(name) => {
                let target = User::find_by_username(&client, &name)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("user '{}' not found", name))?;
                (target.id, target.username)
            }
            None => {
                let user = User::get(&client, user_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("user not found"))?;
                (user.id, user.username)
            }
        };
        if target_user_id != user_id
            && !ChatRoomMember::is_member(&client, room_id, target_user_id).await?
        {
            anyhow::bail!(
                "@{} is not a member of #{}",
                target_username,
                room.slug.as_deref().unwrap_or("room")
            );
        }
        let sheet = CharacterSheet::find_by_user_room(&client, target_user_id, room_id).await?;
        if sheet.is_none() && target_user_id != user_id {
            anyhow::bail!("@{} has no character sheet here yet", target_username);
        }
        let (name, body) = sheet.map(|s| (s.name, s.body)).unwrap_or_default();
        Ok(ChatEvent::OpenSheetResolved {
            user_id,
            room_id,
            target_user_id,
            target_username,
            name,
            body,
        })
    }

    /// Persist a sheet edit. Success is silent (the modal already shows the
    /// committed state); failure surfaces as a chat banner via `SheetError`.
    pub fn save_sheet_task(&self, user_id: Uuid, room_id: Uuid, name: String, body: String) {
        let service = self.clone();
        let span = info_span!(
            "chat.save_sheet_task",
            user_id = %user_id,
            room_id = %room_id,
        );
        tokio::spawn(
            async move {
                if let Err(e) = service.save_sheet(user_id, room_id, name, body).await {
                    let _ = service.evt_tx.send(ChatEvent::SheetError {
                        user_id,
                        message: format!("failed to save sheet: {e}"),
                    });
                }
            }
            .instrument(span),
        );
    }

    async fn save_sheet(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        name: String,
        body: String,
    ) -> Result<()> {
        let client = self.db.get().await?;
        self.ensure_room_scoped_command_access(&client, user_id, room_id, RoomScopedCommand::Sheet)
            .await?;
        CharacterSheet::upsert(
            &client,
            CharacterSheetParams {
                user_id,
                room_id,
                name,
                body,
            },
        )
        .await?;
        Ok(())
    }

    async fn ensure_room_scoped_command_access(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
        room_id: Uuid,
        command: RoomScopedCommand,
    ) -> Result<ChatRoom> {
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if !command.available_in(&room) {
            anyhow::bail!(
                "/{} is only available in #{}",
                command.name(),
                command.room_slug()
            );
        }
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }
        Ok(room)
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
    ) -> Result<(String, Vec<RoomMemberListItem>)> {
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
                let username = usernames.get(&id).cloned();
                RoomMemberListItem {
                    user_id: id,
                    username,
                }
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

    pub fn gift_chips_task(
        &self,
        user_id: Uuid,
        target_username: String,
        amount: i64,
        message: Option<String>,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.gift_chips_task",
            user_id = %user_id,
            target_username = %target_username,
            amount
        );
        tokio::spawn(
            async move {
                let event = match service.gift_chips(user_id, &target_username, amount).await {
                    Ok(gift) => ChatEvent::GiftSucceeded {
                        user_id,
                        sender_username: gift.sender_username,
                        recipient_id: gift.recipient_id,
                        recipient_username: gift.recipient_username,
                        amount,
                        sender_balance: gift.sender_balance,
                        recipient_balance: gift.recipient_balance,
                        message,
                    },
                    Err(error) => ChatEvent::GiftFailed {
                        user_id,
                        message: service_sentence_case(&error.to_string()),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn gift_chips(
        &self,
        user_id: Uuid,
        target_username: &str,
        amount: i64,
    ) -> Result<GiftOutcome> {
        if amount <= 0 {
            anyhow::bail!("gift amount must be positive");
        }
        if amount > GIFT_MAX_AMOUNT {
            anyhow::bail!("gift amount is too large");
        }
        let Some(chip_service) = &self.chip_service else {
            anyhow::bail!("chip gifts are unavailable");
        };

        let client = self.db.get().await?;
        let sender = User::get(&client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("sender not found"))?;
        let sender_username = sender.username.clone();
        let recipient = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("recipient not found"))?;
        if recipient.id == user_id {
            anyhow::bail!("cannot gift yourself");
        }
        let recipient_id = recipient.id;
        let recipient_username = recipient.username.clone();
        drop(client);

        let now = std::time::Instant::now();
        {
            let mut cooldowns = self.gift_cooldowns.lock_recover();
            if let Some(last) = cooldowns.get(&user_id)
                && now.duration_since(*last) < GIFT_COOLDOWN
            {
                anyhow::bail!("gift is on cooldown");
            }
            cooldowns.insert(user_id, now);
        }

        match chip_service
            .transfer_chips(user_id, recipient.id, amount)
            .await
        {
            Ok((sender_balance, recipient_balance)) => Ok(GiftOutcome {
                sender_username,
                recipient_id,
                recipient_username,
                sender_balance,
                recipient_balance,
            }),
            Err(error) => {
                self.gift_cooldowns.lock_recover().remove(&user_id);
                Err(error)
            }
        }
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
        let rows = ChatRoom::list_public_topic_room_summaries(&client).await?;

        let rooms: Vec<String> = rows
            .into_iter()
            .map(|row| {
                let label = row
                    .slug
                    .map(|slug| format!("#{slug}"))
                    .or_else(|| row.language_code.map(|code| format!("language:{code}")))
                    .unwrap_or(row.kind);
                let noun = if row.member_count == 1 {
                    "member"
                } else {
                    "members"
                };
                format!("{label} ({} {noun})", row.member_count)
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

    pub fn friend_user_task(&self, user_id: Uuid, target_username: String) {
        self.friend_mark_task(user_id, target_username, true);
    }

    pub fn unfriend_user_task(&self, user_id: Uuid, target_username: String) {
        self.friend_mark_task(user_id, target_username, false);
    }

    fn friend_mark_task(&self, user_id: Uuid, target_username: String, add: bool) {
        let service = self.clone();
        let span =
            info_span!("chat.friend_mark_task", user_id = %user_id, target = %target_username, add);
        tokio::spawn(
            async move {
                let event = match service
                    .update_friend_mark(user_id, &target_username, add)
                    .await
                {
                    Ok((friend_user_ids, target_user_id, target_username, message)) => {
                        ChatEvent::FriendListUpdated {
                            user_id,
                            friend_user_ids,
                            target_user_id,
                            target_username,
                            message,
                        }
                    }
                    Err(e) => ChatEvent::FriendFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn update_friend_mark(
        &self,
        user_id: Uuid,
        target_username: &str,
        add: bool,
    ) -> Result<(Vec<Uuid>, Uuid, String, String)> {
        let client = self.db.get().await?;
        let target = User::find_by_username(&client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!(
                "Cannot {} yourself",
                if add { "friend" } else { "unfriend" }
            );
        }
        let (changed, ids) = if add {
            User::add_friend_user_id(&client, user_id, target.id).await?
        } else {
            User::remove_friend_user_id(&client, user_id, target.id).await?
        };
        if !changed && add {
            anyhow::bail!("@{} is already a friend", target.username);
        } else if !changed {
            anyhow::bail!("@{} is not a friend", target.username);
        }
        let message = if add {
            format!("Added @{} to friends", target.username)
        } else {
            format!("Removed @{} from friends", target.username)
        };
        Ok((ids, target.id, target.username, message))
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
        // Private game rooms are daily match chats: membership is fixed at
        // claim time to the two players, so joining is only the idempotent
        // re-join that kicks off the tail/list refresh chain. Nobody else
        // may enter.
        if room.visibility != "public"
            && !ChatRoomMember::is_member(&client, room.id, user_id).await?
        {
            anyhow::bail!("this match chat is players only");
        }
        ChatRoomMember::join(&client, room.id, user_id).await?;
        Ok(room.id)
    }

    async fn open_public_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = self.db.get().await?;
        let room = ChatRoom::get_or_create_public_room(&client, slug).await?;
        ChatRoom::set_auto_join(&client, room.id, false).await?;
        tracing::info!(
            slug = %slug,
            room_id = %room.id,
            "public room opened"
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
        let display_name = room.slug.as_deref().unwrap_or("private");
        VoiceChannel::upsert_for_target(&client, TARGET_CHAT_ROOM, room.id, display_name, true)
            .await?;
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
            ChatMessage::delete_by_author(&tx, message_id, user_id).await?
        } else {
            ChatMessage::delete_by_admin(&tx, message_id).await?
        };
        if count == 0 {
            anyhow::bail!("Cannot delete this message");
        }
        ModerationAuditLog::record_if(
            &tx,
            permissions.should_audit(is_owner),
            user_id,
            "message_delete",
            "message",
            Some(message_id),
            json!({ "room_id": msg.room_id }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(message_id = %message_id, "message deleted");
        Ok(msg.room_id)
    }

    pub async fn delete_news_announcements_by_user_and_url(
        &self,
        article_user_id: Uuid,
        news_marker: &str,
        url: &str,
    ) -> Result<usize> {
        let client = self.db.get().await?;
        let deleted =
            ChatMessage::delete_news_by_user_and_url(&client, article_user_id, news_marker, url)
                .await?;
        for (room_id, message_id) in &deleted {
            let _ = self.evt_tx.send(ChatEvent::MessageRemoved {
                room_id: *room_id,
                message_id: *message_id,
            });
        }
        Ok(deleted.len())
    }
}

#[cfg(test)]
#[path = "svc_internal_test.rs"]
mod svc_internal_test;
