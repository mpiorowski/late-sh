use std::{
    cell::Cell,
    cmp::Ordering,
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use late_core::models::cyberspace_account::CmailThread;
use late_core::{
    MutexRecover,
    models::{
        article::{ArticleFeedItem, NEWS_MARKER},
        chat_message::ChatMessage,
        chat_message_gild::{ChatMessageGild, ChatMessageGildSummary, GildTier},
        chat_message_reaction::{ChatMessageReactionOwners, ChatMessageReactionSummary},
        chat_poll::ActiveChatPoll,
        chat_room::ChatRoom,
        message_translation::{TRANSLATE_MAX_BODY_CHARS, TranslateLang, needs_translation},
        voice_channel::VoiceChannel,
    },
};
use rand_core::{OsRng, RngCore};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};
use ratatui_textarea::{CursorMove, Input, TextArea, WrapMode};
use tokio::sync::{broadcast::error::TryRecvError, mpsc, watch};
use uuid::Uuid;

use late_core::models::chat_message::HistoryDirection;

use crate::app::ai::ladder::MentionLadders;
use crate::app::ai::summary::{
    SummaryBasis, SummaryEvent, SummaryOutcome, SummaryService, SummaryWindow,
};
use crate::app::ai::translate::{TranslationEvent, TranslationOutcome, TranslationService};
use crate::app::common::overlay::Overlay;
use crate::app::common::theme;

use crate::app::common::{composer, mentions, primitives::Banner};
use crate::app::help_modal::data::HelpTopic;
use crate::app::notify::{Notification, Notifier};
use crate::authz::Permissions;
use crate::moderation::{
    command::{RoomModAction, ServerUserAction, parse_optional_duration},
    event::ModerationEvent,
    service::{RoomModRequest, RoomRef},
};
use crate::state::{ActiveUser, ActiveUsers};
use crate::usernames::UsernameResolver;

use super::{
    commands::{RoomScopedCommand, rank_command_matches, room_owns_command},
    cyberspace, discover, feeds,
    gild::state::GildTarget,
    history_modal, news, notifications,
    notifications::svc::NotificationService,
    showcase,
    svc::{
        ChatEvent, ChatService, ChatSnapshot, GIFT_MAX_AMOUNT, GildRefusal, ReportKind,
        RoomMemberListItem,
    },
    ui_text::{NewsPayload, parse_news_payload, parse_report_payload},
    work,
};

pub(crate) const ROOM_JUMP_KEYS: &[u8] =
    b"asdfghjklqwertyuiopzxcvbnm1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const USER_CREATED_CHANNEL_NAME_MAX_CHARS: usize = 16;
const REACTION_OWNER_DISPLAY_LIMIT: usize = 4;
const REACTION_OWNER_COLUMNS: usize = 3;
const INLINE_IMAGE_FETCHES_PER_TICK: usize = 8;
const INLINE_IMAGE_SCAN_LIMIT: usize = 100;
const INLINE_IMAGE_MAX_WIDTH: u32 = 96;
const INLINE_IMAGE_MAX_ROWS: u32 = 12;
const INLINE_IMAGE_TRACKED_LIMIT: usize = 2_000;
const INLINE_IMAGE_MAX_FAILURES: u8 = 6;
const TERMINAL_IMAGE_MAX_COLS: u32 = 200;
const TERMINAL_IMAGE_MAX_ROWS: u32 = 60;
const CLIPBOARD_IMAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const READ_CURSOR_FLUSH_DELAY: Duration = Duration::from_secs(2);

/// How long this session's keyboard has to stay quiet before the room on
/// screen gets an AFK line.
///
/// This is a threshold for *absence*, not for reading: reading produces no
/// input at all, so a lurker who sits silent this long collects a line they
/// did not need. That is the deliberate direction to be wrong in. A line
/// nobody wanted costs one glance; the failure it replaced, treating a
/// parked terminal as a reader, silently swallowed the whole day someone
/// missed. Long enough that a coffee run does not trip it, short enough
/// that a real absence is caught before the conversation moves on.
const AFK_LINE_IDLE: Duration = Duration::from_secs(10 * 60);

pub(crate) type InlineImagePreview = crate::app::files::inline_image::InlineImagePreview;
pub(crate) type InlineImageRenderSettings =
    crate::app::files::inline_image::InlineImageRenderSettings;
pub(crate) type InlineImageRenderResult = (
    Uuid,
    InlineImageRenderSettings,
    Result<InlineImagePreview, String>,
);
pub(crate) type TerminalImageRenderResult = (
    Uuid,
    Result<crate::app::files::terminal_image::TerminalImageData, String>,
);

#[derive(Clone, Copy, Debug)]
struct InlineImageFailure {
    attempts: u8,
    next_retry_at: Instant,
}

#[derive(Default)]
struct PendingReadCursorFlush {
    rooms: HashSet<Uuid>,
    flush_at: Option<Instant>,
}

impl PendingReadCursorFlush {
    fn queue(&mut self, room_id: Uuid, now: Instant) {
        self.rooms.insert(room_id);
        if self.flush_at.is_none() {
            self.flush_at = Some(now + READ_CURSOR_FLUSH_DELAY);
        }
    }

    fn take_due(&mut self, now: Instant) -> Vec<Uuid> {
        match self.flush_at {
            Some(deadline) if now >= deadline => self.take_all(),
            _ => Vec::new(),
        }
    }

    fn take_all(&mut self) -> Vec<Uuid> {
        self.flush_at = None;
        self.rooms.drain().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionMatch {
    pub name: String,
    pub online: bool,
    pub prefix: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Default)]
pub(crate) struct MentionAutocomplete {
    pub active: bool,
    pub query: String,
    pub trigger_offset: usize,
    pub matches: Vec<MentionMatch>,
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplyTarget {
    pub message_id: Uuid,
    pub author: String,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModCommandOutput {
    pub request_id: Uuid,
    pub lines: Vec<String>,
    pub success: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingUrlUpload {
    pub url: String,
    pub room_id: Option<Uuid>,
    /// The reply the composer was aiming at when the upload was asked for.
    /// Submitting `/upload` clears the composer, so the target has to travel
    /// with the request or the finished upload comes back as a plain message.
    pub reply_target: Option<ReplyTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingClipboardImageUpload {
    pub room_id: Option<Uuid>,
    /// See [`PendingUrlUpload::reply_target`]; `/paste-image` clears the
    /// composer the same way.
    pub reply_target: Option<ReplyTarget>,
    requested_at: Instant,
}

impl PendingClipboardImageUpload {
    fn new(room_id: Option<Uuid>, reply_target: Option<ReplyTarget>) -> Self {
        Self {
            room_id,
            reply_target,
            requested_at: Instant::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.requested_at.elapsed() >= CLIPBOARD_IMAGE_REQUEST_TIMEOUT
    }
}

/// A pending request to open the room-info form, carried until the app-level
/// input loop opens the modal. `Create` comes from `/private`, `Edit` from
/// `/roominfo` on a room the user may set info on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomInfoRequest {
    Create {
        slug: String,
    },
    Edit {
        room_id: Uuid,
        /// `#slug` of the room being edited, for the form's title.
        room_label: String,
        /// Who the form says holds this room: a username for a private room,
        /// "moderators" for a public one. Ownership can move on its own when a
        /// creator leaves, so the form is where it becomes visible.
        owner_label: String,
        topic: Option<String>,
        rules: Option<String>,
    },
}

/// Who may set a room's topic and rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomInfoAuthority {
    /// This user owns the room (private rooms only).
    Owner,
    /// Staff standing, which carries every room.
    Moderator,
    /// The room has info, but not for this user to set.
    None,
    /// The room has no topic or rules at all (DMs, game rooms).
    NotEditable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewsModalState {
    pub payload: NewsPayload,
    pub meta: String,
    pub article_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageModalState {
    pub message_id: Uuid,
    pub url: String,
}

/// A voice control requested from the composer (`/voice`, `/mute`)
/// in a voice-enabled room. `App` owns the paired-CLI voice plumbing, so the
/// composer just records the intent and `App` carries it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceCommand {
    Join,
    Mute,
}

/// A stream control requested from the composer. `App` owns the stream
/// service, so the composer just records the intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoLiveCommand {
    /// `/golive [title]`: register (or re-surface) this user's stream and
    /// show the publisher URL modal.
    Start { title: Option<String> },
    /// `/golive obs [title]`: register the stream with OBS as the
    /// publisher and show the WHIP connection details modal.
    StartObs { title: Option<String> },
    /// `/golive stop`: tear the stream down now.
    Stop,
}

/// `/golive` with an optional title, or the `stop` / `obs [title]`
/// subcommands. `None` means the body is not a golive command at all.
fn parse_golive_command(body: &str) -> Option<GoLiveCommand> {
    let trimmed = body.trim();
    let rest = trimmed.strip_prefix("/golive")?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    // Free text that ends up in the rail, the stream header, and the
    // #lounge announcement; clamp it at the parse boundary so no downstream
    // surface needs its own cap.
    let clamp = |title: &str| Some(title.chars().take(GOLIVE_TITLE_MAX_CHARS).collect());
    Some(match rest.trim() {
        "" => GoLiveCommand::Start { title: None },
        "stop" => GoLiveCommand::Stop,
        "obs" => GoLiveCommand::StartObs { title: None },
        text => match text.strip_prefix("obs ") {
            Some(title) => GoLiveCommand::StartObs {
                title: clamp(title.trim()),
            },
            None => GoLiveCommand::Start { title: clamp(text) },
        },
    })
}

/// Longest `/golive` title kept; the rest is cut at the parse boundary.
const GOLIVE_TITLE_MAX_CHARS: usize = 80;

/// The crown, requested from the composer. `App` owns the crown service, so
/// the composer just records the intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CrownCommand {
    /// `/crown`: who wears it, for how long, and what taking it costs.
    Status,
    /// `/crown take`: buy it at whatever the ladder says right now.
    Take,
}

/// `Some(Some(command))` on `/crown` or `/crown take`, `Some(None)` on
/// anything else after `/crown` (usage banner), `None` when the line is not
/// a crown command at all.
fn parse_crown_command(body: &str) -> Option<Option<CrownCommand>> {
    let rest = body.trim().strip_prefix("/crown")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(match rest.trim() {
        "" => Some(CrownCommand::Status),
        "take" => Some(CrownCommand::Take),
        _ => None,
    })
}

/// The pot, requested from the composer. `App` owns the pot service, so the
/// composer just records the intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PotCommand {
    /// `/pot`: size, tickets, your holding, and time to the draw.
    Status,
    /// `/pot buy N`: buy N tickets at the flat ticket price.
    Buy { count: i64 },
}

/// `Some(Some(command))` on `/pot` or a well-formed `/pot buy N`,
/// `Some(None)` on anything else after `/pot` (usage banner), `None` when the
/// line is not a pot command at all.
///
/// The count is parsed here rather than in the service: a boundary that only
/// admits 1..=(the daily cap) is what lets everything downstream trust the
/// number; whether today still has room is the service's call.
fn parse_pot_command(body: &str) -> Option<Option<PotCommand>> {
    use late_core::models::pot::POT_MAX_TICKETS_PER_DAY;
    let rest = body.trim().strip_prefix("/pot")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(Some(PotCommand::Status));
    }
    let Some(count) = rest.strip_prefix("buy ") else {
        return Some(None);
    };
    Some(
        count
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|count| (1..=POT_MAX_TICKETS_PER_DAY).contains(count))
            .map(|count| PotCommand::Buy { count }),
    )
}

/// An aquarium control requested from the composer (`/aquarium`,
/// `/aquarium feed`). `App` owns the tray state and entitlements, so the
/// composer just records the intent and `App` carries it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AquariumCommand {
    Toggle,
    Feed,
}

/// A pet action requested from the composer (`/pet` toggles the strip;
/// `/pet feed` and `/pet water` are care). `App` owns the pet state and
/// entitlements, so the composer just records the intent and `App` carries
/// it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PetCommand {
    Toggle,
    Feed,
    Water,
}

/// The two cyberspace rows a room can be entered from, and the one leaving
/// it goes back to.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CyberspaceRow {
    #[default]
    Feeds,
    Notifications,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CyberspaceCommand {
    Open,
    Post,
    /// Their chat roster, where rooms get pinned into the rail.
    Chat,
    /// Their C-Mail conversations, pinned into the rail the same way.
    Mail,
    /// `/cs mail @user`: start (or find) a conversation and pin it. The
    /// username is theirs, not a late.sh one, so it is taken as written.
    MailTo(String),
    Link,
    Unlink,
    Invalid,
}

/// `/cs` and `/cyberspace` with an optional subcommand. `None` means the
/// body is not a cyberspace command at all (falls through to other handlers).
/// Two composers parse against this: the main chat one, and the composer
/// inside one of their rooms, where a command of ours must never be sent to
/// their API as a message.
pub(crate) fn parse_cyberspace_command(body: &str) -> Option<CyberspaceCommand> {
    let trimmed = body.trim();
    let rest = trimmed
        .strip_prefix("/cyberspace")
        .or_else(|| trimmed.strip_prefix("/cs"))?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let rest = rest.trim();
    if let Some(target) = rest
        .strip_prefix("mail ")
        .or_else(|| rest.strip_prefix("dm "))
    {
        let username = target.trim().trim_start_matches('@');
        return Some(match username.is_empty() {
            true => CyberspaceCommand::Invalid,
            false => CyberspaceCommand::MailTo(username.to_string()),
        });
    }
    Some(match rest {
        "" => CyberspaceCommand::Open,
        "post" => CyberspaceCommand::Post,
        "chat" | "rooms" => CyberspaceCommand::Chat,
        "mail" | "dms" => CyberspaceCommand::Mail,
        "link" => CyberspaceCommand::Link,
        "unlink" => CyberspaceCommand::Unlink,
        _ => CyberspaceCommand::Invalid,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RoomSlot {
    Room(Uuid),
    Feeds,
    News,
    Cyberspace,
    /// Their notification list, its own row beside `feeds`. It carries its own
    /// badge (their counter endpoint) rather than being folded into the feed's
    /// count: the two are read in different places and open from different
    /// rows, so one number could only say "somewhere in cyberspace".
    CyberspaceNotifications,
    /// A pinned cyberspace C-Mail conversation, by its position in the pinned
    /// list. Indexed for the same reason `CyberspaceRoom` is: the selection
    /// state is `Copy` and their conversation id is not.
    CyberspaceMail(usize),
    /// A pinned cyberspace chat room, by its position in the pinned list.
    /// Every synthetic entry before this one was a singleton picked out by a
    /// bool; these are user-added and ordered, and the index is what fits in
    /// the `Copy` selection state a slug could not. The pinned list changes
    /// only where rooms are added or removed, which is the one place that has
    /// to re-point a selection.
    CyberspaceRoom(usize),
    Notifications,
    Discover,
    Showcase,
    Work,
}

/// Collapsible groupings of the room-list rail. Each maps to one section
/// header drawn by `build_cozy_room_rail_rows`. A section in
/// `ChatState::collapsed_sections` renders header-only and its rooms drop out
/// of `visual_order` (so navigation skips them too).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoomSection {
    Favorites,
    Core,
    /// Registered "watch me" streams: one row per stream while any exists.
    /// The whole section disappears when nobody is streaming.
    Stream,
    /// Only rendered for a linked account: the cyberspace pane plus the chat
    /// rooms this user pinned.
    Cyberspace,
    Channels,
    Dms,
}

impl RoomSection {
    /// Every section. Key maps and tests iterate this rather than repeating a
    /// hand-written list: a copy of the roster somewhere else silently misses
    /// a new section, which is how `z`-folding lost the cyberspace header.
    pub(crate) const ALL: [RoomSection; 6] = [
        RoomSection::Favorites,
        RoomSection::Core,
        RoomSection::Stream,
        RoomSection::Cyberspace,
        RoomSection::Channels,
        RoomSection::Dms,
    ];

    /// The header label as rendered in the rail. Used to map a clicked header
    /// row back to its section.
    pub(crate) fn label(self) -> &'static str {
        match self {
            RoomSection::Favorites => "favorites",
            RoomSection::Core => "core",
            RoomSection::Stream => "stream",
            RoomSection::Cyberspace => "cyberspace",
            RoomSection::Channels => "channels",
            RoomSection::Dms => "dms",
        }
    }

    pub(crate) fn shortcut(self) -> u8 {
        match self {
            RoomSection::Favorites => b'f',
            RoomSection::Core => b'o',
            RoomSection::Stream => b's',
            RoomSection::Cyberspace => b'y',
            RoomSection::Channels => b'c',
            RoomSection::Dms => b'd',
        }
    }

    /// Resolve a header label back to its section (inverse of `label`).
    pub(crate) fn from_label(label: &str) -> Option<RoomSection> {
        match label {
            "favorites" => Some(RoomSection::Favorites),
            "core" => Some(RoomSection::Core),
            "stream" => Some(RoomSection::Stream),
            "cyberspace" => Some(RoomSection::Cyberspace),
            "channels" => Some(RoomSection::Channels),
            "dms" => Some(RoomSection::Dms),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SelectedRoomSlotState {
    pub selected_room_id: Option<Uuid>,
    pub feeds_selected: bool,
    pub news_selected: bool,
    pub cyberspace_selected: bool,
    pub cyberspace_notifications_selected: bool,
    pub cyberspace_room_selected: Option<usize>,
    pub cyberspace_mail_selected: Option<usize>,
    pub notifications_selected: bool,
    pub discover_selected: bool,
    pub showcase_selected: bool,
    pub work_selected: bool,
}

pub(crate) fn is_selected_slot(slot: RoomSlot, selected: SelectedRoomSlotState) -> bool {
    match slot {
        RoomSlot::Room(room_id) => {
            !selected.feeds_selected
                && !selected.news_selected
                && !selected.cyberspace_selected
                && !selected.cyberspace_notifications_selected
                && selected.cyberspace_room_selected.is_none()
                && selected.cyberspace_mail_selected.is_none()
                && !selected.notifications_selected
                && !selected.discover_selected
                && !selected.showcase_selected
                && !selected.work_selected
                && selected.selected_room_id == Some(room_id)
        }
        RoomSlot::Feeds => selected.feeds_selected,
        RoomSlot::News => selected.news_selected,
        RoomSlot::Cyberspace => selected.cyberspace_selected,
        RoomSlot::CyberspaceNotifications => selected.cyberspace_notifications_selected,
        RoomSlot::CyberspaceRoom(index) => selected.cyberspace_room_selected == Some(index),
        RoomSlot::CyberspaceMail(index) => selected.cyberspace_mail_selected == Some(index),
        RoomSlot::Notifications => selected.notifications_selected,
        RoomSlot::Discover => selected.discover_selected,
        RoomSlot::Showcase => selected.showcase_selected,
        RoomSlot::Work => selected.work_selected,
    }
}

fn synthetic_entry_selected(selected: SelectedRoomSlotState) -> bool {
    selected.feeds_selected
        || selected.news_selected
        || selected.cyberspace_selected
        || selected.cyberspace_notifications_selected
        || selected.cyberspace_room_selected.is_some()
        || selected.cyberspace_mail_selected.is_some()
        || selected.notifications_selected
        || selected.discover_selected
        || selected.showcase_selected
        || selected.work_selected
}

fn current_slot_from_state(state: SelectedRoomSlotState) -> Option<RoomSlot> {
    if state.feeds_selected {
        return Some(RoomSlot::Feeds);
    }
    if state.news_selected {
        return Some(RoomSlot::News);
    }
    if state.cyberspace_selected {
        return Some(RoomSlot::Cyberspace);
    }
    if state.cyberspace_notifications_selected {
        return Some(RoomSlot::CyberspaceNotifications);
    }
    if let Some(index) = state.cyberspace_room_selected {
        return Some(RoomSlot::CyberspaceRoom(index));
    }
    if let Some(index) = state.cyberspace_mail_selected {
        return Some(RoomSlot::CyberspaceMail(index));
    }
    if state.notifications_selected {
        return Some(RoomSlot::Notifications);
    }
    if state.discover_selected {
        return Some(RoomSlot::Discover);
    }
    if state.showcase_selected {
        return Some(RoomSlot::Showcase);
    }
    if state.work_selected {
        return Some(RoomSlot::Work);
    }
    state.selected_room_id.map(RoomSlot::Room)
}

fn room_membership_command_target(
    composer_room_id: Option<Uuid>,
    selected: SelectedRoomSlotState,
) -> Option<Uuid> {
    composer_room_id.or_else(|| {
        if synthetic_entry_selected(selected) {
            None
        } else {
            selected.selected_room_id
        }
    })
}

pub(crate) fn is_chat_list_room(room: &ChatRoom) -> bool {
    if room.kind == "game" {
        return false;
    }

    room.kind == "dm" || room.permanent || matches!(room.visibility.as_str(), "public" | "private")
}

/// Payload handed from chat to the app layer (via `take_requested_open_sheet`)
/// to open the character sheet modal. `editable` is true when the sheet
/// belongs to the viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetOpenRequest {
    pub room_id: Uuid,
    pub target_username: String,
    pub name: String,
    pub body: String,
    pub editable: bool,
}

/// Outcome of one chat tick: the banner to surface plus whether the tick may
/// have changed render-visible chat state. `changed` over-reports by design
/// (prove-clean, not prove-dirty): every queued event or snapshot counts,
/// even ones that end up filtered out during the drain.
pub struct ChatTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

pub struct ChatState {
    pub(crate) service: ChatService,
    user_id: Uuid,
    permissions: Permissions,
    is_admin: bool,
    is_moderator: bool,
    active_users: Option<ActiveUsers>,
    /// S3/R2 upload storage; `None` means every upload feature is disabled.
    files: Option<crate::config::FilesConfig>,
    /// Process-global ghost-bot cooldown ladders, peeked at submit time for
    /// the "bot is cooling down" banner. The ghost loops own stepping it.
    mention_ladders: MentionLadders,
    snapshot_rx: watch::Receiver<ChatSnapshot>,
    /// Single-recipient events (tail loads, search results, discover lists)
    /// delivered point-to-point by the service instead of over the global
    /// broadcast; drained ahead of `event_rx` in `drain_events`.
    targeted_event_rx: mpsc::UnboundedReceiver<ChatEvent>,
    event_rx: tokio::sync::broadcast::Receiver<ChatEvent>,
    moderation_event_rx: tokio::sync::broadcast::Receiver<ModerationEvent>,
    pub(crate) rooms: Vec<(ChatRoom, Vec<ChatMessage>)>,
    /// Per-room message-store version, bumped on any change to that room's
    /// stored messages or their reactions. Rendered row caches compare this
    /// instead of hashing message bodies (`ChatRowsVersions`).
    room_versions: HashMap<Uuid, u64>,
    /// Author-context epoch, bumped when any map feeding chat row rendering
    /// (usernames, countries, friends, glyphs, badges, inline images)
    /// actually changes.
    context_epoch: u64,
    pub(crate) active_polls: HashMap<Uuid, ActiveChatPoll>,
    lounge_room_id: Option<Uuid>,
    /// Recent #lounge system-feed lines (see `activity/lounge.rs`), newest
    /// first, capped at `ACTIVITY_TICKER_CAP`. System messages are diverted
    /// here at every ingestion point instead of being stored as chat rows;
    /// the UI packs these left-to-right into the one-row activity ticker
    /// above the composer. IRC and persistence are unaffected.
    activity_ticker: Vec<ActivityTickerEntry>,
    pub(crate) usernames: HashMap<Uuid, String>,
    pub(crate) countries: HashMap<Uuid, String>,
    ignored_user_ids: HashSet<Uuid>,
    friend_user_ids: HashSet<Uuid>,
    /// When the last `IgnoreListUpdated`/`FriendListUpdated` was applied. Both
    /// lists also arrive on every chat snapshot, and a snapshot read that began
    /// before this write would roll the lists back to what the database held
    /// before it committed.
    friend_ignore_applied_at: Option<Instant>,
    /// Derived owner per private room, from the chat snapshot. Ownership is not
    /// stored on the room (a creator who leaves hands it on), so this is the
    /// session's view of who currently holds each private room.
    room_owner_ids: HashMap<Uuid, Uuid>,
    username_rx: watch::Receiver<Arc<Vec<String>>>,
    overlay: Option<Overlay>,
    /// A ready `/summary` that arrived while another overlay was up. It
    /// takes the surface in `drain_summary_events` as soon as it frees,
    /// instead of clobbering what the user is reading.
    pending_summary_overlay: Option<Overlay>,
    news_modal: Option<NewsModalState>,
    image_modal: Option<ImageModalState>,
    /// Cells the open image modal can devote to an image, reported back from
    /// the previous frame's draw. Sixel fetches encode to fit this.
    image_modal_capacity: Option<(u16, u16)>,
    pending_reaction_owners_message_id: Option<Uuid>,
    pub(crate) unread_counts: HashMap<Uuid, i64>,
    /// The AFK line per room: the instant this session's human went quiet
    /// while that room was on screen. The `new messages` divider draws
    /// against it and `/history` opens there. It is one of two marks, and
    /// the other one, [`ChatState::device_left_at`], is what `/summary`
    /// reads; they never feed each other.
    ///
    /// Deliberately session-local and never written to the DB. "Did the
    /// person at this terminal step away" is a fact about this terminal;
    /// your phone being idle says nothing about your desktop.
    ///
    /// Placed by [`ChatState::sync_afk_line`], removed only when a message
    /// you wrote lands in the room. Not by coming back and touching a key
    /// (the line exists to show you where to start reading, and clearing it
    /// on the keystroke that returns you destroys it exactly when it is
    /// wanted), and not by `/summary` or `/history`, which do not read it
    /// as a catch-up cursor at all in the first case and only look in the
    /// second.
    pub(crate) afk_lines: HashMap<Uuid, DateTime<Utc>>,
    /// The other mark: when you last left the app on this device, the
    /// moment the keyboard went quiet before the previous session on this
    /// SSH key ended (`user_ssh_keys.left_at`, taken once at boot). A bare
    /// `/summary` always reads from here, whatever the AFK line says: the
    /// question it answers is "what happened since I was last here", and
    /// "here" is this device, not this room. Fixed for the session, so
    /// asking twice reads the same stretch twice; `None` for a keyless
    /// session or a device with no mark yet.
    device_left_at: Option<DateTime<Utc>>,
    pending_read_rooms: HashSet<Uuid>,
    pending_read_flush: PendingReadCursorFlush,
    /// Message ids whose mentions of this user were already reported as
    /// rendered, so a room that stays visible does not resend the same stamp
    /// on every cursor flush. Session-local; the DB update is idempotent.
    mention_reads_sent: HashSet<Uuid>,
    /// The DM currently held in the promoted unread-DMs group even though its
    /// unread count is already zero. See `note_sticky_unread_dm`.
    pub(crate) sticky_unread_dm: Option<Uuid>,
    visible_room_id: Option<Uuid>,
    room_tx: watch::Sender<Option<Uuid>>,
    refresh_tx: mpsc::UnboundedSender<()>,
    refresh_room_id: Option<Uuid>,
    loading_tail_rooms: HashSet<Uuid>,
    pub(crate) selected_room_id: Option<Uuid>,
    pub(crate) room_jump_active: bool,
    composer: TextArea<'static>,
    pub(crate) composing: bool,
    composer_room_id: Option<Uuid>,
    /// Index into the cup-art variant list, advanced each time the user
    /// runs `/coffee` or `/tea` so back-to-back rituals rotate through
    /// different ASCII cups within a session. Session-local; never
    /// persisted.
    next_cup_variant: u8,
    /// Last-rendered chat composer area, set by `chat::ui` during draw and
    /// consumed by mouse hit-testing in `app::input`. `Cell` keeps the
    /// interior mutable through the immutable view references used in
    /// rendering. Reset to `None` at the start of every frame.
    pub(crate) last_composer_rect: Cell<Option<Rect>>,
    /// Top visible wrapped composer row, updated on every render that draws
    /// the composer. Mouse clicks use this to map visible rows back to the
    /// underlying multiline composer when `ratatui_textarea` has scrolled.
    /// Unlike `last_composer_rect` this persists across frames: it mirrors
    /// the widget's own persistent `Viewport` (which the crate keeps
    /// `pub(crate)`), and the minimal-scroll replay in
    /// `next_composer_viewport_top` needs the previous top as input.
    pub(crate) last_composer_viewport_top: Cell<Option<usize>>,
    /// Most recent left-button click coordinates + timestamp inside the
    /// composer rect, used to detect a double-click that enters compose mode.
    pub(crate) last_composer_click: Option<(u16, u16, Instant)>,
    /// Last-rendered chat-scroll hit layout (content rect + per-row hit
    /// info), set by `chat::ui` during draw and consumed by mouse
    /// hit-testing in `app::input`. Reset to `None` at the top of every
    /// frame alongside `last_composer_rect`. Only one chat surface paints
    /// per frame, so this single cell covers Home #lounge, Home chat
    /// center, and embedded Rooms chat.
    pub(crate) last_chat_hit_layout: Cell<Option<super::ui::ChatHitLayout>>,
    pending_send_notices: VecDeque<Uuid>,
    pub(crate) pending_chat_screen_switch: bool,
    pub(crate) mention_ac: MentionAutocomplete,
    pub(crate) all_usernames: Arc<Vec<String>>,
    pub(crate) bonsai_glyphs: HashMap<Uuid, String>,
    pub(crate) chat_badges: HashMap<Uuid, String>,
    pub(crate) profile_award_badges: HashMap<Uuid, String>,
    pub(crate) message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    /// Gild markers by message. Only gilded messages have an entry, which is
    /// almost none of them, so this stays tiny even in a busy room.
    pub(crate) message_gilds: HashMap<Uuid, ChatMessageGildSummary>,
    pub(crate) voice_channels_by_room_id: HashMap<Uuid, VoiceChannel>,
    /// Translation handle + result feed (`app/ai/translate.rs`). Requests are
    /// fire-and-forget; results land on `translation_rx` and drain in tick.
    translation_service: TranslationService,
    translation_rx: tokio::sync::broadcast::Receiver<TranslationEvent>,
    /// `/summary` handle + result feed (`app/ai/summary.rs`), same
    /// fire-and-forget contract as translation.
    summary_service: SummaryService,
    summary_rx: tokio::sync::broadcast::Receiver<SummaryEvent>,
    /// This session's translations for the current target language, keyed by
    /// message id. Cleared when the target language changes.
    pub(crate) translations: HashMap<Uuid, TranslationDisplay>,
    /// Messages whose shown translation was collapsed with `t`. Session-local
    /// override; wins over auto mode and the cache at render time.
    pub(crate) translation_hidden: HashSet<Uuid>,
    /// Message ids this session requested via `t`, so a failure banners only
    /// for the requester, never for auto-mode bystanders.
    translation_manual: HashSet<Uuid>,
    /// Message ids already sent through the bulk cache lookup, so re-entering
    /// a room does not requery its history. Cleared when the target changes.
    translation_cache_checked: HashSet<Uuid>,
    /// Mirrors of the profile's translation settings, synced by `App::tick`.
    translate_to: TranslateLang,
    auto_translate: bool,
    /// The viewer's account timezone, synced by `App::tick`. `None` (unset or
    /// unparseable) means every absolute time this pane writes stays UTC.
    viewer_tz: Option<chrono_tz::Tz>,
    pub(crate) selected_message_id: Option<Uuid>,
    /// Row-level viewport offset inside a selected message that wraps taller
    /// than the chat pane; see [`SelectionScroll`]. Reset whenever the
    /// selection lands on a different message.
    pub(crate) selection_scroll: SelectionScroll,
    /// Armed by a first `d` press on a message; a second `d` on the same
    /// still-selected message confirms the delete. Any selection change or
    /// clear disarms it so a stale confirm can't reap the wrong message.
    pub(crate) pending_delete_message_id: Option<Uuid>,
    pub(crate) reaction_leader_active: bool,
    pub(crate) highlighted_message_id: Option<Uuid>,
    pub(crate) edited_message_id: Option<Uuid>,
    pub(crate) reply_target: Option<ReplyTarget>,
    pub(crate) room_last_message_at: HashMap<Uuid, Option<DateTime<Utc>>>,
    bg_task: tokio::task::AbortHandle,

    /// News (shown as a virtual room in the room list)
    pub(crate) news_selected: bool,
    pub(crate) feeds_selected: bool,
    pub feeds: feeds::state::State,
    pub(crate) news: news::state::State,
    pub(crate) cyberspace_selected: bool,
    /// Their notification list, the row beside `feeds`.
    pub(crate) cyberspace_notifications_selected: bool,
    /// Which cyberspace row a room or conversation was entered from, so
    /// leaving one goes back where the user came in rather than always
    /// landing on `feeds`. A mention jumped to from the notifications row is
    /// what made the difference visible.
    pub(crate) cyberspace_return_row: CyberspaceRow,
    /// Which pinned cyberspace chat room is selected, by position in the
    /// pinned list. `None` whenever any other rail entry is.
    pub(crate) cyberspace_room_selected: Option<usize>,
    /// Which pinned C-Mail conversation is selected, same contract.
    pub(crate) cyberspace_mail_selected: Option<usize>,
    pub cyberspace: cyberspace::state::State,

    /// Notifications / mentions (shown as a virtual room in the room list)
    pub(crate) notifications_selected: bool,
    pub(crate) notifications: notifications::state::State,
    pub(crate) discover_selected: bool,
    pub(crate) discover: discover::state::State,
    /// Message-search results for the Ctrl+/ modal's `?` mode. Owned here
    /// (not on the modal) because ChatState owns the chat event receiver.
    pub(crate) message_search: MessageSearch,
    /// A search hit the user asked to jump to before its room tail was
    /// loaded: `(room_id, message_id)`. Resolved when that room's tail lands,
    /// or handed to the history modal if the message turns out to sit further
    /// back than the tail reaches.
    pending_search_jump: Option<(Uuid, Uuid)>,
    /// Paginated room history. Owned here, like `message_search`, because
    /// ChatState owns the chat event receiver its pages arrive on.
    pub(crate) history_modal: history_modal::state::ChatHistoryModalState,
    pub(crate) showcase_selected: bool,
    pub(crate) showcase: showcase::state::State,
    pub(crate) work_selected: bool,
    pub(crate) work: work::state::State,
    favorite_room_ids: Vec<Uuid>,

    /// Producer handle for desktop notifications; drained by render through
    /// `App::notify_outbox`.
    notifier: Notifier,
    requested_help_topic: Option<HelpTopic>,
    requested_room_info_modal: Option<RoomInfoRequest>,
    requested_settings_modal: bool,
    requested_shop_modal: bool,
    requested_mod_modal: bool,
    requested_ultimate_modal: bool,
    requested_pair: Option<PairRequest>,
    requested_pomodoro: Option<PomodoroRequest>,
    requested_icon_picker: bool,
    /// Set by /search [query]; consumed by `App`, which opens the Ctrl+/
    /// modal pre-filled with `?query`.
    requested_message_search: Option<String>,
    requested_petname: Option<PetnameRequest>,
    requested_open_profile: Option<(Uuid, String)>,
    requested_open_sheet: Option<SheetOpenRequest>,
    requested_quit: bool,
    requested_audio_url: Option<String>,
    requested_audio_fallback_url: Option<String>,
    requested_audio_skip: bool,
    /// Set by /voice or /mute in a voice-enabled room; consumed by `App`
    /// (which owns the paired-CLI voice controls).
    requested_voice_command: Option<VoiceCommand>,
    /// Set by /golive; consumed by `App` (which owns the stream service).
    requested_golive: Option<GoLiveCommand>,
    requested_crown: Option<CrownCommand>,
    requested_pot: Option<PotCommand>,
    /// Set by /watch @user; consumed by `App`.
    requested_watch: Option<String>,
    /// A stream room this session just opened; consumed by `App`, which
    /// tells the stream service a named viewer showed up. Recorded here
    /// rather than acted on inline because `App` owns the stream service.
    opened_stream_room: Option<Uuid>,
    /// Set by /aquarium [feed]; consumed by `App` (which owns the tray).
    requested_aquarium_command: Option<AquariumCommand>,
    /// Set by /pet, /pet feed, /pet water; consumed by `App` (which owns the pet).
    requested_pet_command: Option<PetCommand>,
    requested_poll_room: Option<Uuid>,
    /// Set by /brb command; contains the custom message (empty = no message).
    requested_brb: Option<String>,
    /// Set when a real (non-command) chat message is sent; used to clear AFK.
    sent_regular_message: bool,
    pending_mod_outputs: VecDeque<ModCommandOutput>,

    /// Room-list sections the user has collapsed. Empty = all expanded
    /// (the default). Session-only — resets on reconnect.
    pub(crate) collapsed_sections: HashSet<RoomSection>,

    /// Registered "watch me" streams, copied from the stream registry watch
    /// in `App::tick` (~1/s) so render paths read local memory only. Drives
    /// the rail's `stream` section, the LIVE author tag (live entries only),
    /// and the stream header block above a live room's chat.
    pub(crate) live_streams: Vec<crate::app::stream::registry::LiveStreamView>,
    /// Users whose stream is actually on air right now (`live` only, never
    /// pending): the LIVE author tag reads this. Derived in
    /// `set_live_streams`.
    pub(crate) live_user_ids: HashSet<Uuid>,

    // image upload
    pub(crate) image_upload_rx: Option<tokio::sync::oneshot::Receiver<Result<String, String>>>,
    pub(crate) image_upload_pending: bool,
    pub(crate) image_upload_target_room_id: Option<Uuid>,
    /// The reply the in-flight upload was composed against, handed back to the
    /// composer when the URL lands.
    image_upload_reply_target: Option<ReplyTarget>,
    pub(crate) requested_url_upload: Option<PendingUrlUpload>,
    requested_clipboard_image_upload: Option<PendingClipboardImageUpload>,
    pending_clipboard_image_upload: Option<PendingClipboardImageUpload>,

    // inline image rendering
    pub(crate) inline_image_rx:
        Option<tokio::sync::mpsc::UnboundedReceiver<InlineImageRenderResult>>,
    pub(crate) inline_image_tx: Option<tokio::sync::mpsc::UnboundedSender<InlineImageRenderResult>>,
    pub(crate) inline_image_cache: HashMap<uuid::Uuid, InlineImagePreview>,
    pub(crate) inline_image_requested: HashSet<uuid::Uuid>,
    inline_image_failures: HashMap<uuid::Uuid, InlineImageFailure>,
    inline_image_render_settings: InlineImageRenderSettings,
    inline_image_tracked_order: VecDeque<uuid::Uuid>,
    terminal_image_rx: Option<tokio::sync::mpsc::UnboundedReceiver<TerminalImageRenderResult>>,
    terminal_image_tx: Option<tokio::sync::mpsc::UnboundedSender<TerminalImageRenderResult>>,
    pub(crate) terminal_image_cache:
        HashMap<uuid::Uuid, crate::app::files::terminal_image::TerminalImageData>,
    terminal_image_requested: HashSet<uuid::Uuid>,
    terminal_image_failed: HashSet<uuid::Uuid>,
    pub(crate) last_image_upload_at: Option<std::time::Instant>,
}

/// Row-level scroll inside a selected message that wraps taller than the
/// chat pane. Chat scroll is otherwise derived purely from message
/// selection, which pins a too-tall message's top edge and leaves its
/// bottom unreachable; this is the escape hatch `j`/`k` fall back to.
///
/// `rows` is how many rows past the message's pinned top the viewport
/// starts; `overflow` is the renderer's measurement of how far `rows` can
/// go (0 for a selection that fits the pane). Both are `Cell`s because the
/// renderer writes them through the shared view's `&self`: it publishes
/// `overflow` each frame and clamps `rows` back when a resize shrinks the
/// range. Input reads the previous frame's measurement, the same one-frame
/// contract as the history modal's `visible_rows`.
#[derive(Default)]
pub struct SelectionScroll {
    pub(crate) rows: Cell<usize>,
    pub(crate) overflow: Cell<usize>,
}

impl SelectionScroll {
    fn reset(&self) {
        self.rows.set(0);
        self.overflow.set(0);
    }

    /// Apply a row delta against the last measured overflow. Returns whether
    /// the viewport moved; `false` means that direction has nothing left to
    /// reveal (or the selection fits the pane) and the caller should move
    /// the selection instead.
    fn step(&self, delta: isize) -> bool {
        let overflow = self.overflow.get();
        if overflow == 0 {
            return false;
        }
        let current = self.rows.get();
        let next = (current as isize + delta).clamp(0, overflow as isize) as usize;
        if next == current {
            return false;
        }
        self.rows.set(next);
        true
    }
}

/// What the UI knows about one message's translation into the session's
/// target language. `Failed` renders nothing but lets `t` retry.
/// `SameLanguage` renders nothing and sticks: the model already judged the
/// message readable, so `t` answers with a banner instead of a new call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranslationDisplay {
    Pending,
    Ready(String),
    SameLanguage,
    Failed,
}

/// The session identity a `ChatState` is built for: who is connected, under
/// what name, with what rights.
pub(crate) struct ChatSession {
    pub user_id: Uuid,
    pub username: String,
    pub permissions: Permissions,
    /// When you last left the app on this device (`user_ssh_keys.left_at`,
    /// taken once at boot); `None` for a keyless session or a device with
    /// no mark yet. See [`ChatState::device_left_at`].
    pub device_left_at: Option<DateTime<Utc>>,
}

pub(crate) struct ChatServices {
    pub chat: ChatService,
    pub translation: crate::app::ai::translate::TranslationService,
    pub summary: crate::app::ai::summary::SummaryService,
    pub notifications: NotificationService,
    pub articles: news::svc::ArticleService,
    pub feeds: feeds::svc::FeedService,
    pub showcases: showcase::svc::ShowcaseService,
    pub work: work::svc::WorkService,
    pub cyberspace: cyberspace::svc::CyberspaceService,
}

impl Drop for ChatState {
    fn drop(&mut self) {
        self.bg_task.abort();
    }
}

impl ChatState {
    pub(crate) fn new(
        services: ChatServices,
        session: ChatSession,
        active_users: Option<ActiveUsers>,
        notifier: Notifier,
        mention_ladders: MentionLadders,
        files: Option<crate::config::FilesConfig>,
    ) -> Self {
        let ChatSession {
            user_id,
            username,
            permissions,
            device_left_at,
        } = session;
        let ChatServices {
            chat: service,
            translation: translation_service,
            summary: summary_service,
            notifications: notification_service,
            articles: article_service,
            feeds: feed_service,
            showcases: showcase_service,
            work: work_service,
            cyberspace: cyberspace_service,
        } = services;
        let event_rx = service.subscribe_events();
        let moderation_event_rx = service.subscribe_moderation_events();
        let username_rx = service.subscribe_usernames();
        let (room_tx, room_rx) = watch::channel(None);
        let (snapshot_rx, targeted_event_rx, refresh_tx, bg_task) =
            service.start_user_refresh_task(user_id, room_rx);

        let (inline_image_tx, inline_image_rx) = tokio::sync::mpsc::unbounded_channel();
        let (terminal_image_tx, terminal_image_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            service,
            user_id,
            permissions,
            is_admin: permissions.is_admin(),
            is_moderator: permissions.is_moderator(),
            active_users,
            files,
            mention_ladders,
            snapshot_rx,
            targeted_event_rx,
            event_rx,
            moderation_event_rx,
            rooms: Vec::new(),
            room_versions: HashMap::new(),
            context_epoch: 0,
            active_polls: HashMap::new(),
            lounge_room_id: None,
            activity_ticker: Vec::new(),
            // Seeded with the session's own name so self-referential features
            // (the mention highlight, rendered-mention read stamps) work
            // before the user has authored anything a payload would carry.
            usernames: HashMap::from([(user_id, username)]),
            countries: HashMap::new(),
            ignored_user_ids: HashSet::new(),
            friend_user_ids: HashSet::new(),
            friend_ignore_applied_at: None,
            room_owner_ids: HashMap::new(),
            username_rx,
            overlay: None,
            pending_summary_overlay: None,
            news_modal: None,
            image_modal: None,
            image_modal_capacity: None,
            pending_reaction_owners_message_id: None,
            unread_counts: HashMap::new(),
            afk_lines: HashMap::new(),
            device_left_at,
            pending_read_rooms: HashSet::new(),
            pending_read_flush: PendingReadCursorFlush::default(),
            mention_reads_sent: HashSet::new(),
            sticky_unread_dm: None,
            visible_room_id: None,
            room_tx,
            refresh_tx,
            refresh_room_id: None,
            loading_tail_rooms: HashSet::new(),
            selected_room_id: None,
            room_jump_active: false,
            composer: new_chat_textarea(),
            composing: false,
            composer_room_id: None,
            next_cup_variant: 0,
            last_composer_rect: Cell::new(None),
            last_composer_viewport_top: Cell::new(None),
            last_composer_click: None,
            last_chat_hit_layout: Cell::new(None),
            pending_send_notices: VecDeque::new(),
            pending_chat_screen_switch: false,
            mention_ac: MentionAutocomplete::default(),
            all_usernames: Arc::new(Vec::new()),
            bonsai_glyphs: HashMap::new(),
            chat_badges: HashMap::new(),
            profile_award_badges: HashMap::new(),
            message_reactions: HashMap::new(),
            message_gilds: HashMap::new(),
            voice_channels_by_room_id: HashMap::new(),
            translation_rx: translation_service.subscribe(),
            translation_service,
            summary_rx: summary_service.subscribe(),
            summary_service,
            translations: HashMap::new(),
            translation_hidden: HashSet::new(),
            translation_manual: HashSet::new(),
            translation_cache_checked: HashSet::new(),
            translate_to: TranslateLang::En,
            auto_translate: false,
            viewer_tz: None,
            selected_message_id: None,
            selection_scroll: SelectionScroll::default(),
            pending_delete_message_id: None,
            reaction_leader_active: false,
            highlighted_message_id: None,
            edited_message_id: None,
            reply_target: None,
            room_last_message_at: HashMap::new(),
            bg_task,
            news_selected: false,
            feeds_selected: false,
            feeds: feeds::state::State::new(feed_service, article_service.clone(), user_id),
            news: news::state::State::new(article_service, user_id, permissions.is_admin()),
            cyberspace_selected: false,
            cyberspace_notifications_selected: false,
            cyberspace_return_row: CyberspaceRow::Feeds,
            cyberspace_room_selected: None,
            cyberspace_mail_selected: None,
            cyberspace: cyberspace::state::State::new(cyberspace_service, user_id),
            notifications_selected: false,
            notifications: notifications::state::State::new(notification_service, user_id),
            discover_selected: false,
            discover: discover::state::State::new(),
            message_search: MessageSearch::default(),
            pending_search_jump: None,
            history_modal: history_modal::state::ChatHistoryModalState::default(),
            showcase_selected: false,
            showcase: showcase::state::State::new(
                showcase_service,
                user_id,
                permissions.is_admin(),
            ),
            work_selected: false,
            work: work::state::State::new(work_service, user_id, permissions.is_admin()),
            favorite_room_ids: Vec::new(),
            notifier,
            requested_help_topic: None,
            requested_room_info_modal: None,
            requested_settings_modal: false,
            requested_shop_modal: false,
            requested_mod_modal: false,
            requested_ultimate_modal: false,
            requested_pair: None,
            requested_pomodoro: None,
            requested_icon_picker: false,
            requested_message_search: None,
            requested_petname: None,
            requested_open_profile: None,
            requested_open_sheet: None,
            requested_quit: false,
            requested_voice_command: None,
            requested_golive: None,
            requested_crown: None,
            requested_pot: None,
            requested_watch: None,
            opened_stream_room: None,
            requested_aquarium_command: None,
            requested_pet_command: None,
            requested_audio_url: None,
            requested_audio_fallback_url: None,
            requested_audio_skip: false,
            requested_poll_room: None,
            requested_brb: None,
            sent_regular_message: false,
            pending_mod_outputs: VecDeque::new(),
            collapsed_sections: HashSet::new(),
            live_streams: Vec::new(),
            live_user_ids: HashSet::new(),
            image_upload_rx: None,
            image_upload_pending: false,
            image_upload_target_room_id: None,
            image_upload_reply_target: None,
            requested_url_upload: None,
            requested_clipboard_image_upload: None,
            pending_clipboard_image_upload: None,
            inline_image_rx: Some(inline_image_rx),
            inline_image_tx: Some(inline_image_tx),
            inline_image_cache: HashMap::new(),
            inline_image_requested: HashSet::new(),
            inline_image_failures: HashMap::new(),
            inline_image_render_settings: InlineImageRenderSettings::default(),
            inline_image_tracked_order: VecDeque::new(),
            terminal_image_rx: Some(terminal_image_rx),
            terminal_image_tx: Some(terminal_image_tx),
            terminal_image_cache: HashMap::new(),
            terminal_image_requested: HashSet::new(),
            terminal_image_failed: HashSet::new(),
            last_image_upload_at: None,
        }
    }

    pub(crate) fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }

    pub(crate) fn refresh_composer_theme(&mut self) {
        composer::apply_themed_textarea_style(&mut self.composer, self.composing);
        self.news.refresh_composer_theme();
        self.showcase.refresh_composer_theme();
        self.work.refresh_composer_theme();
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn start_composing(&mut self) {
        if let Some(room_id) = self.selected_room_id {
            self.start_composing_in_room(room_id);
        }
    }

    pub fn start_composing_in_room(&mut self, room_id: Uuid) {
        self.room_jump_active = false;
        self.composing = true;
        self.composer_room_id = Some(room_id);
        self.selected_message_id = None;
        self.selection_scroll.reset();
        self.reply_target = None;
        self.edited_message_id = None;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
    }

    pub fn start_command_composer_in_room(&mut self, room_id: Uuid) {
        self.start_composing_in_room(room_id);
        self.composer = new_chat_textarea();
        self.composer.insert_char('/');
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
        self.update_autocomplete();
    }

    pub fn request_list(&mut self) {
        self.flush_pending_read_cursors();
        self.sync_refresh_room_id();
        let _ = self.refresh_tx.send(());
        if let Some(room_id) = self.selected_room_id {
            self.request_room_tail(room_id);
        }
    }

    pub fn request_room_tail(&mut self, room_id: Uuid) {
        if self.loading_tail_rooms.insert(room_id) {
            self.service.load_room_tail_task(self.user_id, room_id);
        }
    }

    pub fn join_game_room_chat(&self, room_id: Uuid) {
        self.service.join_game_room_task(self.user_id, room_id);
    }

    fn sync_refresh_room_id(&mut self) {
        if self.refresh_room_id != self.selected_room_id {
            self.refresh_room_id = self.selected_room_id;
            let _ = self.room_tx.send(self.selected_room_id);
        }
    }

    pub fn sync_selection(&mut self) {
        if self.rooms.is_empty() {
            self.selected_room_id = None;
            self.room_jump_active = false;
            return;
        }

        if let Some(selected_id) = self.selected_room_id {
            // A selected stream room survives snapshot refreshes even
            // though it is `kind='game'` (not a chat-list room), and even
            // before its lazy membership lands in `rooms`.
            if self.stream_for_room(selected_id).is_some() {
                return;
            }
            if self
                .rooms
                .iter()
                .any(|(room, _)| room.id == selected_id && is_chat_list_room(room))
            {
                return;
            }
        }

        self.selected_room_id = self
            .rooms
            .iter()
            .find(|(room, _)| is_chat_list_room(room))
            .map(|(room, _)| room.id);
    }

    pub fn mark_room_read(&mut self, room_id: Uuid) {
        self.note_sticky_unread_dm(room_id);
        self.pending_read_rooms.insert(room_id);
        self.unread_counts.insert(room_id, 0);
        self.pending_read_flush.queue(room_id, Instant::now());
    }

    /// Place the AFK line for the room on screen once this session's
    /// keyboard has been quiet for [`AFK_LINE_IDLE`]. `idle` is how long ago
    /// the last input landed, mirrored from `App` the same way `viewer_tz`
    /// is. Reports whether the line moved, since that is render-visible.
    ///
    /// Only the visible room can collect a line: a room you are not looking
    /// at was never being attended, and its rail badge already says what is
    /// waiting. A room that already holds a line keeps the one it has, so
    /// the line marks when you went quiet and does not slide forward while
    /// you stay away.
    pub(crate) fn sync_afk_line(&mut self, idle: Duration) -> bool {
        if idle < AFK_LINE_IDLE {
            return false;
        }
        let Some(room_id) = self.visible_room_id else {
            return false;
        };
        if self.afk_lines.contains_key(&room_id) {
            return false;
        }
        let Ok(idle) = chrono::Duration::from_std(idle) else {
            return false;
        };
        self.afk_lines.insert(room_id, Utc::now() - idle);
        self.bump_room_version(room_id);
        true
    }

    /// Drop the room's AFK line: this reader is in the conversation again.
    ///
    /// One caller, and it is an act the reader performed rather than a state
    /// we inferred: a message of theirs landing in the room.
    fn clear_afk_line(&mut self, room_id: Uuid) {
        if self.afk_lines.remove(&room_id).is_some() {
            self.bump_room_version(room_id);
        }
    }

    /// Remember the DM being read so the promoted unread-DMs group can hold it
    /// in place. Reading is what zeroes the unread count, so this runs before
    /// the count is cleared.
    fn note_sticky_unread_dm(&mut self, room_id: Uuid) {
        let unread = self.unread_counts.get(&room_id).copied().unwrap_or(0) > 0;
        let is_dm = self
            .rooms
            .iter()
            .any(|(room, _)| room.id == room_id && room.kind == "dm");
        self.sticky_unread_dm = next_sticky_unread_dm(NextStickyUnreadDm {
            current: self.sticky_unread_dm,
            room_id,
            is_dm,
            unread,
        });
    }

    pub fn mark_room_read_at(&self, room_id: Uuid, read_at: DateTime<Utc>) {
        self.service
            .mark_room_read_at_task(self.user_id, room_id, read_at);
    }

    pub fn mark_selected_room_read(&mut self) {
        let Some(room_id) = self.selected_room_id else {
            return;
        };

        self.mark_room_read(room_id);
    }

    pub fn visible_room_id(&self) -> Option<Uuid> {
        self.visible_room_id
    }

    pub fn set_visible_room_id(&mut self, room_id: Option<Uuid>) {
        let changed = self.visible_room_id != room_id;
        if changed {
            self.flush_pending_read_cursors();
        }
        self.visible_room_id = room_id;
        if changed {
            self.request_cached_translations_for_visible_room();
        }
    }

    /// Sync the profile's translation settings into this session. Called from
    /// `App::tick`; a target change drops every per-message translation state
    /// (it all describes the old language) and invalidates row caches.
    /// Mirror the account timezone the summary overlay dates its window in.
    pub(crate) fn set_viewer_tz(&mut self, timezone: Option<chrono_tz::Tz>) {
        self.viewer_tz = timezone;
    }

    pub fn set_translate_settings(&mut self, target: TranslateLang, auto: bool) -> bool {
        let mut changed = false;
        if self.translate_to != target {
            self.translate_to = target;
            self.translations.clear();
            self.translation_hidden.clear();
            self.translation_manual.clear();
            self.translation_cache_checked.clear();
            self.context_epoch += 1;
            changed = true;
        }
        if self.auto_translate != auto {
            self.auto_translate = auto;
            changed = true;
        }
        if changed {
            self.request_cached_translations_for_visible_room();
        }
        changed
    }

    /// `t` on the selected message: show a translation (cache or API), or
    /// collapse/re-open one already shown. Failed entries retry.
    pub fn toggle_translation_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        let (message_id, message_room_id, body) = {
            let message = self.selected_message_in_room(room_id)?;
            (message.id, message.room_id, message.body.clone())
        };
        match self.translations.get(&message_id) {
            Some(TranslationDisplay::Pending) => None,
            Some(TranslationDisplay::SameLanguage) => Some(Banner::info(&format!(
                "Already written in {}",
                self.translate_to.prompt_name()
            ))),
            Some(TranslationDisplay::Ready(_)) => {
                if !self.translation_hidden.remove(&message_id) {
                    self.translation_hidden.insert(message_id);
                }
                self.bump_room_version(message_room_id);
                None
            }
            Some(TranslationDisplay::Failed) | None => {
                // Length check first: `needs_translation` also returns false
                // for over-cap bodies, and "nothing to translate" would be a
                // lie for a genuinely foreign wall of text. The cap judges
                // the reply-quote-free text, same as the check itself.
                if late_core::models::message_translation::translation_source_text(&body)
                    .chars()
                    .count()
                    > TRANSLATE_MAX_BODY_CHARS
                {
                    return Some(Banner::info("Message too long to translate"));
                }
                if !needs_translation(&body, self.translate_to) {
                    return Some(Banner::info("Nothing to translate here"));
                }
                self.translations
                    .insert(message_id, TranslationDisplay::Pending);
                self.translation_manual.insert(message_id);
                self.translation_cache_checked.insert(message_id);
                self.translation_service.request(
                    message_id,
                    message_room_id,
                    body,
                    self.translate_to,
                );
                self.bump_room_version(message_room_id);
                None
            }
        }
    }

    /// Entering a room (or settings change, or the tail loading): one bulk
    /// cache-only lookup over the translatable history already loaded. Every
    /// session sweeps, auto mode or not; the drain decides what to display
    /// (auto mode pre-expands all hits, everyone else gets author-shared
    /// rows only). Misses stay collapsed until `t`, which is what keeps the
    /// sweep free of API calls.
    fn request_cached_translations_for_visible_room(&mut self) {
        let Some(room_id) = self.visible_room_id else {
            return;
        };
        let Some((_, messages)) = self.rooms.iter().find(|(room, _)| room.id == room_id) else {
            return;
        };
        let ids: Vec<Uuid> = messages
            .iter()
            .filter(|message| {
                message.user_id != self.user_id
                    && !self.translations.contains_key(&message.id)
                    && !self.translation_cache_checked.contains(&message.id)
                    && needs_translation(&message.body, self.translate_to)
            })
            .map(|message| message.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        self.translation_cache_checked.extend(ids.iter().copied());
        self.translation_service
            .load_cached(room_id, ids, self.translate_to);
    }

    fn drain_translation_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            let event = match self.translation_rx.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Lagged(_)) => {
                    // The dropped events may have carried results for entries
                    // this session marked Pending, and nothing else ever
                    // clears Pending (`t` on a Pending message is a no-op).
                    // Reset them so the marker disappears and `t` can
                    // re-request instead of reading "translating…" forever.
                    self.reset_pending_translations();
                    continue;
                }
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            };
            if event.target != self.translate_to {
                continue;
            }
            let author = self.rooms.iter().find_map(|(room, messages)| {
                if room.id != event.room_id {
                    return None;
                }
                messages
                    .iter()
                    .find(|m| m.id == event.message_id)
                    .map(|m| m.user_id)
            });
            let Some(author) = author else {
                self.translation_manual.remove(&event.message_id);
                continue;
            };
            // Author-shared results display to everyone reading the target
            // language, except the author's own session: they wrote the
            // original and don't need it echoed back translated.
            let shared_for_me = event.author_shared && author != self.user_id;
            match event.outcome {
                TranslationOutcome::Translated(text) => {
                    // Requested here (pending), free coverage from another
                    // session's call while this one runs auto mode, or the
                    // author chose to share it with this target language.
                    let show = self.auto_translate
                        || self.translations.contains_key(&event.message_id)
                        || shared_for_me;
                    if show {
                        self.translations
                            .insert(event.message_id, TranslationDisplay::Ready(text));
                        self.translation_manual.remove(&event.message_id);
                        self.bump_room_version(event.room_id);
                    }
                }
                TranslationOutcome::SameLanguage => {
                    // Remembered so the message is never re-requested; renders
                    // as nothing. A manual `t` gets told instead of left
                    // staring at a spinner that produced no line.
                    let show = self.auto_translate
                        || self.translations.contains_key(&event.message_id)
                        || shared_for_me;
                    if show {
                        self.translations
                            .insert(event.message_id, TranslationDisplay::SameLanguage);
                        self.bump_room_version(event.room_id);
                    }
                    if self.translation_manual.remove(&event.message_id) {
                        banner = Some(Banner::info(&format!(
                            "Already written in {}",
                            self.translate_to.prompt_name()
                        )));
                    }
                }
                TranslationOutcome::Failed => {
                    if self.translations.get(&event.message_id)
                        == Some(&TranslationDisplay::Pending)
                    {
                        self.translations
                            .insert(event.message_id, TranslationDisplay::Failed);
                        self.bump_room_version(event.room_id);
                    }
                    if self.translation_manual.remove(&event.message_id) {
                        banner = Some(Banner::error("Translation unavailable right now"));
                    }
                }
            }
        }
        banner
    }

    /// Drain `/summary` results. A ready summary opens the shared overlay
    /// (the `/rules` surface); when another overlay is already up it waits
    /// in `pending_summary_overlay` instead of clobbering it, and takes the
    /// surface here as soon as it frees. Every other outcome banners. The
    /// broadcast carries all users' results, so foreign events are dropped.
    /// Returns the banner plus whether a waiting summary was promoted (a
    /// render-visible change `tick` cannot see from the queues).
    fn drain_summary_events(&mut self) -> (Option<Banner>, bool) {
        use tokio::sync::broadcast::error::TryRecvError;
        let mut promoted = false;
        if self.overlay.is_none()
            && let Some(overlay) = self.pending_summary_overlay.take()
        {
            self.overlay = Some(overlay);
            promoted = true;
        }
        let mut banner = None;
        loop {
            let event = match self.summary_rx.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            };
            if event.user_id != self.user_id {
                continue;
            }
            match event.outcome {
                SummaryOutcome::Ready {
                    text,
                    message_count,
                    since,
                    basis,
                    capped,
                    truncated,
                } => {
                    let plural = if message_count == 1 { "" } else { "s" };
                    let stamp = crate::app::common::time::instant_for_viewer(since, self.viewer_tz);
                    // Every arm names where the window came from, because
                    // "since you left" and "the last 24h" are different
                    // claims and the reader has to be able to tell which
                    // one they got. A capped window says so rather than
                    // presenting the cap as the moment they left.
                    let mut head = match basis {
                        SummaryBasis::LeftApp => {
                            format!("{message_count} message{plural} since you left · {stamp}")
                        }
                        SummaryBasis::Default => format!(
                            "{message_count} message{plural} since {stamp} · the last {}h",
                            crate::app::ai::summary::SUMMARY_DEFAULT_WINDOW_HOURS
                        ),
                        SummaryBasis::Explicit => {
                            format!("{message_count} message{plural} since {stamp}")
                        }
                    };
                    if capped {
                        head.push_str(&format!(
                            " · capped at {}h",
                            crate::app::ai::summary::SUMMARY_MAX_WINDOW_HOURS
                        ));
                    }
                    if truncated {
                        head.push_str(" · older messages past the cap left out");
                    }
                    let mut lines = vec![head, String::new()];
                    lines.extend(text.lines().map(str::to_string));
                    let overlay = Overlay::new(format!("{} catch-up", event.room_label), lines);
                    match self.overlay {
                        // An overlay the user is reading is not clobbered
                        // by an async result; the summary waits its turn.
                        Some(_) => {
                            self.pending_summary_overlay = Some(overlay);
                            banner =
                                Some(Banner::info("Summary ready, close the open panel to view"));
                        }
                        None => self.overlay = Some(overlay),
                    }
                }
                SummaryOutcome::Empty { basis } => {
                    banner = Some(Banner::info(match basis {
                        SummaryBasis::LeftApp => "Nothing new since you left",
                        SummaryBasis::Default => "Nothing said in the last 24h",
                        SummaryBasis::Explicit => "Nothing said in that window",
                    }));
                }
                SummaryOutcome::InFlight => {
                    banner = Some(Banner::info("Summary already running"));
                }
                SummaryOutcome::Cooldown { remaining } => {
                    let minutes = remaining.as_secs().div_ceil(60).max(1);
                    banner = Some(Banner::info(&format!(
                        "Summary cooling down, try again in {minutes}m"
                    )));
                }
                SummaryOutcome::CapExhausted => {
                    banner = Some(Banner::error("Summaries hit today's cap, back tomorrow"));
                }
                SummaryOutcome::Unavailable => {
                    banner = Some(Banner::error("Summaries are not available here"));
                }
                SummaryOutcome::Failed => {
                    banner = Some(Banner::error("Summary failed, try again"));
                }
            }
        }
        (banner, promoted)
    }

    fn flush_pending_read_cursors(&mut self) {
        let room_ids = self.pending_read_flush.take_all();
        self.flush_read_cursors(room_ids);
    }

    fn flush_pending_read_cursors_if_due(&mut self) {
        let room_ids = self.pending_read_flush.take_due(Instant::now());
        self.flush_read_cursors(room_ids);
    }

    fn flush_read_cursors(&mut self, room_ids: Vec<Uuid>) {
        for room_id in &room_ids {
            self.service.mark_room_read_task(self.user_id, *room_id);
        }
        self.flush_rendered_mention_reads(&room_ids);
    }

    /// Report mentions of this user whose messages are actually loaded in the
    /// rooms being marked read, so their rail-badge entries clear. This is
    /// deliberately narrower than the room's own read cursor: a mention
    /// sitting above the loaded tail was never on screen and stays unread.
    /// Runs on the same debounced flush as the cursors; every event that can
    /// surface a mention (tail landing, a live message, the room becoming
    /// visible) re-queues the room, so nothing is missed by collecting here.
    fn flush_rendered_mention_reads(&mut self, room_ids: &[Uuid]) {
        let username_lower = self
            .usernames
            .get(&self.user_id)
            .map(|username| username.to_ascii_lowercase());
        let Some(username_lower) = username_lower else {
            return;
        };
        let mut rendered: Vec<Uuid> = Vec::new();
        for room_id in room_ids {
            let Some((_, messages)) = self.rooms.iter().find(|(room, _)| room.id == *room_id)
            else {
                continue;
            };
            rendered.extend(
                messages
                    .iter()
                    .filter(|message| {
                        message.user_id != self.user_id
                            && !self.mention_reads_sent.contains(&message.id)
                            && mentions::mentions_user(&message.body, Some(&username_lower))
                    })
                    .map(|message| message.id),
            );
        }
        if rendered.is_empty() {
            return;
        }
        self.mention_reads_sent.extend(rendered.iter().copied());
        self.notifications.mark_read_for_messages(rendered);
    }

    /// Returns visible messages for the given room.
    fn visible_messages_for_room(&self, room_id: Uuid) -> Vec<&ChatMessage> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(_, msgs)| msgs.iter().collect())
            .unwrap_or_default()
    }

    pub(crate) fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    pub(crate) fn news_modal(&self) -> Option<&NewsModalState> {
        self.news_modal.as_ref()
    }

    pub(crate) fn has_news_modal(&self) -> bool {
        self.news_modal.is_some()
    }

    pub(crate) fn close_news_modal(&mut self) {
        self.news_modal = None;
    }

    pub(crate) fn image_modal(&self) -> Option<&ImageModalState> {
        self.image_modal.as_ref()
    }

    pub(crate) fn has_image_modal(&self) -> bool {
        self.image_modal.is_some()
    }

    pub(crate) fn close_image_modal(&mut self) {
        if let Some(modal) = self.image_modal.as_ref() {
            self.terminal_image_failed.remove(&modal.message_id);
        }
        self.image_modal = None;
        self.image_modal_capacity = None;
    }

    pub(crate) fn set_image_modal_capacity(&mut self, capacity: Option<(u16, u16)>) {
        if let Some(capacity) = capacity {
            self.image_modal_capacity = Some(capacity);
        }
    }

    pub(crate) fn news_modal_url(&self) -> Option<&str> {
        self.news_modal
            .as_ref()
            .map(|modal| modal.payload.url.as_str())
    }

    pub(crate) fn jump_to_news_modal_article(&mut self) -> bool {
        let Some(modal) = self.news_modal.take() else {
            return false;
        };
        self.select_news();
        if let Some(article_id) = modal.article_id {
            self.news.select_article_by_id(article_id);
            return true;
        }
        if let Some(article_id) = self.news.article_id_by_url(&modal.payload.url) {
            self.news.select_article_by_id(article_id);
        }
        true
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
        self.pending_reaction_owners_message_id = None;
    }

    pub fn scroll_overlay(&mut self, delta: i16) {
        if let Some(overlay) = &mut self.overlay {
            overlay.scroll(delta);
        }
    }

    pub fn take_requested_help_topic(&mut self) -> Option<HelpTopic> {
        self.requested_help_topic.take()
    }

    pub fn take_requested_settings_modal(&mut self) -> bool {
        std::mem::take(&mut self.requested_settings_modal)
    }

    pub fn take_requested_shop_modal(&mut self) -> bool {
        std::mem::take(&mut self.requested_shop_modal)
    }

    pub fn take_requested_room_info_modal(&mut self) -> Option<RoomInfoRequest> {
        self.requested_room_info_modal.take()
    }

    pub fn take_requested_mod_modal(&mut self) -> bool {
        std::mem::take(&mut self.requested_mod_modal)
    }

    pub fn take_requested_ultimate_modal(&mut self) -> bool {
        std::mem::take(&mut self.requested_ultimate_modal)
    }

    pub(crate) fn take_requested_pair(&mut self) -> Option<PairRequest> {
        self.requested_pair.take()
    }

    pub(crate) fn take_requested_pomodoro(&mut self) -> Option<PomodoroRequest> {
        self.requested_pomodoro.take()
    }

    pub(crate) fn take_requested_petname(&mut self) -> Option<PetnameRequest> {
        self.requested_petname.take()
    }

    pub fn take_requested_icon_picker(&mut self) -> bool {
        std::mem::take(&mut self.requested_icon_picker)
    }

    pub(crate) fn take_requested_message_search(&mut self) -> Option<String> {
        self.requested_message_search.take()
    }

    pub fn take_requested_open_profile(&mut self) -> Option<(Uuid, String)> {
        self.requested_open_profile.take()
    }

    pub fn take_requested_open_sheet(&mut self) -> Option<SheetOpenRequest> {
        self.requested_open_sheet.take()
    }

    pub fn take_requested_quit(&mut self) -> bool {
        std::mem::take(&mut self.requested_quit)
    }

    pub fn take_requested_audio_url(&mut self) -> Option<String> {
        self.requested_audio_url.take()
    }

    pub fn take_requested_audio_fallback_url(&mut self) -> Option<String> {
        self.requested_audio_fallback_url.take()
    }

    pub fn take_requested_brb(&mut self) -> Option<String> {
        self.requested_brb.take()
    }

    pub fn take_sent_regular_message(&mut self) -> bool {
        std::mem::replace(&mut self.sent_regular_message, false)
    }

    pub fn take_requested_audio_skip(&mut self) -> bool {
        std::mem::take(&mut self.requested_audio_skip)
    }

    pub(crate) fn take_requested_voice_command(&mut self) -> Option<VoiceCommand> {
        self.requested_voice_command.take()
    }

    pub(crate) fn take_requested_golive(&mut self) -> Option<GoLiveCommand> {
        self.requested_golive.take()
    }

    pub(crate) fn take_requested_crown(&mut self) -> Option<CrownCommand> {
        self.requested_crown.take()
    }

    pub(crate) fn take_requested_pot(&mut self) -> Option<PotCommand> {
        self.requested_pot.take()
    }

    pub(crate) fn take_requested_watch(&mut self) -> Option<String> {
        self.requested_watch.take()
    }

    pub(crate) fn take_opened_stream_room(&mut self) -> Option<Uuid> {
        self.opened_stream_room.take()
    }

    /// Replace the live-stream copy. Returns true when it changed (the
    /// caller bumps the row-cache epoch so LIVE tags and rail rows repaint).
    pub(crate) fn set_live_streams(
        &mut self,
        streams: Vec<crate::app::stream::registry::LiveStreamView>,
    ) -> bool {
        if self.live_streams == streams {
            return false;
        }
        self.live_streams = streams;
        self.live_user_ids = self
            .live_streams
            .iter()
            .filter(|stream| stream.live)
            .map(|stream| stream.user_id)
            .collect();
        true
    }

    /// The registered stream owning `room_id`, if any.
    pub(crate) fn stream_for_room(
        &self,
        room_id: Uuid,
    ) -> Option<&crate::app::stream::registry::LiveStreamView> {
        self.live_streams
            .iter()
            .find(|stream| stream.room_id == room_id)
    }

    pub(crate) fn take_requested_aquarium_command(&mut self) -> Option<AquariumCommand> {
        self.requested_aquarium_command.take()
    }

    pub(crate) fn take_requested_pet_command(&mut self) -> Option<PetCommand> {
        self.requested_pet_command.take()
    }

    pub fn take_requested_poll_room(&mut self) -> Option<Uuid> {
        self.requested_poll_room.take()
    }

    pub fn create_poll(
        &self,
        room_id: Uuid,
        question: String,
        options: Vec<String>,
        duration_secs: i64,
    ) {
        self.service
            .create_poll_task(self.user_id, room_id, question, options, duration_secs);
    }

    pub fn cast_poll_vote_for_selected_room(&self, option_position: i32) -> bool {
        let Some(room_id) = self.visible_real_room_id_for_poll() else {
            return false;
        };
        let Some(poll) = self.active_polls.get(&room_id) else {
            return false;
        };
        if !poll
            .options
            .iter()
            .any(|option| option.position == option_position)
        {
            return false;
        }
        self.service
            .cast_poll_vote_task(self.user_id, poll.poll.id, option_position);
        true
    }

    fn visible_real_room_id_for_poll(&self) -> Option<Uuid> {
        if self.synthetic_entry_selected() {
            return None;
        }
        self.selected_room_id
    }

    pub fn active_poll_for_room(&self, room_id: Uuid) -> Option<&ActiveChatPoll> {
        self.active_polls.get(&room_id)
    }

    pub(crate) fn set_permissions(&mut self, permissions: Permissions) {
        self.permissions = permissions;
        self.is_admin = permissions.is_admin();
        self.is_moderator = permissions.is_moderator();
        self.news.set_is_admin(self.is_admin);
        self.showcase.set_is_admin(self.is_admin);
        self.work.set_is_admin(self.is_admin);
    }

    pub(crate) fn submit_mod_command(&mut self, command: String) -> Uuid {
        let request_id = Uuid::now_v7();
        self.service
            .run_mod_command_task(self.user_id, self.permissions, request_id, command);
        request_id
    }

    pub(crate) fn take_mod_outputs(&mut self) -> Vec<ModCommandOutput> {
        self.pending_mod_outputs.drain(..).collect()
    }

    fn select_from_ids(&mut self, ids: &[Uuid], delta: isize) {
        self.reaction_leader_active = false;
        self.pending_delete_message_id = None;
        if ids.is_empty() {
            self.selected_message_id = None;
            self.selection_scroll.reset();
            return;
        }

        let current_idx = self
            .selected_message_id
            .and_then(|id| ids.iter().position(|mid| *mid == id));

        let new_idx = match current_idx {
            Some(idx) => (idx as isize)
                .saturating_add(delta)
                .clamp(0, ids.len() as isize - 1) as usize,
            None => 0,
        };

        // A clamped move that stays on the same message keeps its row
        // offset; landing anywhere else starts reading from the top again.
        if self.selected_message_id != Some(ids[new_idx]) {
            self.selection_scroll.reset();
        }
        self.selected_message_id = Some(ids[new_idx]);
    }

    /// Scroll by rows inside the selected message when it wraps taller than
    /// the chat pane. Positive walks toward the message's end, negative back
    /// toward its top. Returns whether the viewport moved; `false` means
    /// there is nothing left to reveal in that direction and the caller
    /// should move the selection instead. The overflow measurement is the
    /// previous frame's, so a fresh selection reads 0 until it has rendered
    /// once; a held-down key therefore skims across messages instead of
    /// getting stuck inside every long one.
    pub(crate) fn scroll_selected_message_rows(&mut self, delta: isize) -> bool {
        self.selected_message_id.is_some() && self.selection_scroll.step(delta)
    }

    /// Move message cursor by delta. Positive = toward older, negative = toward newer.
    /// First press activates cursor on the newest message.
    pub fn select_message_in_room(&mut self, room_id: Uuid, delta: isize) {
        self.highlighted_message_id = None;
        let ids: Vec<Uuid> = self
            .visible_messages_for_room(room_id)
            .iter()
            .map(|m| m.id)
            .collect();
        self.select_from_ids(&ids, delta);
    }

    pub fn clear_message_selection(&mut self) {
        self.reaction_leader_active = false;
        self.pending_delete_message_id = None;
        self.selected_message_id = None;
        self.selection_scroll.reset();
    }

    pub fn focus_message_in_room(&mut self, room_id: Uuid, message_id: Uuid) {
        self.reaction_leader_active = false;
        self.pending_delete_message_id = None;
        self.selection_scroll.reset();
        // Every synthetic entry drops, the open cyberspace room included: a
        // jump lands on a real room, and a room nobody is looking at must not
        // keep its stream and heartbeat running.
        self.clear_synthetic_selection();
        self.selected_room_id = Some(room_id);
        self.selected_message_id = Some(message_id);
        self.highlighted_message_id = Some(message_id);
    }

    pub fn begin_reaction_leader(&mut self) -> bool {
        if self.selected_message_id.is_none() {
            return false;
        }
        self.reaction_leader_active = true;
        true
    }

    pub fn cancel_reaction_leader(&mut self) {
        self.reaction_leader_active = false;
    }

    pub fn is_reaction_leader_active(&self) -> bool {
        self.reaction_leader_active
    }

    pub fn open_selected_message_reactions_in_room(&mut self, room_id: Uuid) -> bool {
        self.reaction_leader_active = false;
        let Some(message_id) = self.selected_message_in_room(room_id).map(|m| m.id) else {
            return false;
        };

        self.overlay = Some(Overlay::dismissible(
            "Reactions",
            vec!["Loading reactions…".to_string()],
        ));
        self.pending_reaction_owners_message_id = Some(message_id);
        self.service
            .list_reaction_owners_task(self.user_id, message_id);
        true
    }

    pub fn begin_reply_to_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        self.reaction_leader_active = false;
        let message = self.selected_message_in_room(room_id)?;
        let message_user_id = message.user_id;
        let message_body = message.body.clone();
        let author = self
            .usernames
            .get(&message_user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(message_user_id));
        self.reply_target = Some(ReplyTarget {
            message_id: message.id,
            author,
            preview: reply_preview_text(&message_body),
        });
        self.composing = true;
        self.composer_room_id = Some(room_id);
        self.edited_message_id = None;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
        None
    }

    /// Try to jump from a selected reply message to the original message in
    /// the currently-loaded room tail. Returns true when the selected message
    /// carries a reply target, even if the target is not loaded locally.
    pub fn try_jump_to_selected_reply_target_in_room(&mut self, room_id: Uuid) -> bool {
        self.reaction_leader_active = false;
        let Some(selected_id) = self.selected_message_id else {
            return false;
        };

        let Some(reply_to_message_id) = self
            .rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, messages)| loaded_reply_target_id(messages, selected_id))
        else {
            return false;
        };

        if let Some(reply_to_message_id) = reply_to_message_id {
            self.focus_message_in_room(room_id, reply_to_message_id);
        }
        true
    }

    pub fn begin_edit_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        self.reaction_leader_active = false;
        let selected_id = self.selected_message_id?;
        let Some(message) = self.find_message_in_room(room_id, selected_id) else {
            return Some(Banner::error("Selected message not found"));
        };
        let message_user_id = message.user_id;
        let room_id = message.room_id;
        let body = message.body.clone();
        self.begin_edit_message(selected_id, message_user_id, room_id, &body)
    }

    fn begin_edit_message(
        &mut self,
        selected_id: Uuid,
        message_user_id: Uuid,
        room_id: Uuid,
        body: &str,
    ) -> Option<Banner> {
        let is_own = message_user_id == self.user_id;
        if !is_own && !self.permissions.can_moderate() {
            return Some(Banner::error("Can only edit your own messages"));
        }
        self.edited_message_id = Some(selected_id);
        self.composer = new_chat_textarea();
        self.composer.insert_str(body);
        self.composing = true;
        self.composer_room_id = Some(room_id);
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
        None
    }

    pub(crate) fn reply_target(&self) -> Option<&ReplyTarget> {
        self.reply_target.as_ref()
    }

    /// Delete the selected message if owned by user (or if admin).
    ///
    /// Requires a confirming double-press: the first `d` arms the delete and
    /// asks for a second press; a second `d` on the same still-selected
    /// message goes through. Any selection change disarms it (see
    /// `select_from_ids` / `clear_message_selection`), so after a delete the
    /// cursor lands on the adjacent message disarmed and the next `d` re-arms
    /// rather than reaping it — you still walk a run of own messages, just
    /// `dd` per message instead of a single `d`.
    ///
    /// Selection moves to the adjacent message (prefer the next/older one,
    /// fall back to the previous/newer one) so the cursor doesn't jump back to
    /// the newest every time.
    pub fn delete_selected_message_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        let selected_id = self.selected_message_id?;
        let msg_user_id = self
            .find_message_in_room(room_id, selected_id)
            .map(|m| m.user_id)?;
        let is_own = msg_user_id == self.user_id;
        if !is_own && !self.permissions.can_moderate() {
            return Some(Banner::error("Can only delete your own messages"));
        }
        if self.pending_delete_message_id != Some(selected_id) {
            self.pending_delete_message_id = Some(selected_id);
            return Some(Banner::info("Press d again to delete"));
        }
        self.pending_delete_message_id = None;
        self.service
            .delete_message_task(self.user_id, selected_id, self.permissions);
        self.selected_message_id = self
            .rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, msgs)| adjacent_message_id(msgs, selected_id));
        self.selection_scroll.reset();
        Some(Banner::success("Deleting message..."))
    }

    fn selected_message_in_room(&self, room_id: Uuid) -> Option<&ChatMessage> {
        let selected_id = self.selected_message_id?;
        self.find_message_in_room(room_id, selected_id)
    }

    pub fn selected_message_body_in_room(&self, room_id: Uuid) -> Option<String> {
        self.selected_message_in_room(room_id)
            .map(|m| m.body.clone())
    }

    pub fn selected_message_id_in_room(&self, room_id: Uuid) -> Option<Uuid> {
        self.selected_message_in_room(room_id).map(|m| m.id)
    }

    pub fn selected_message_is_news_in_room(&self, room_id: Uuid) -> bool {
        self.selected_message_in_room(room_id)
            .and_then(|m| parse_news_payload(&m.body))
            .is_some()
    }

    pub fn selected_message_has_inline_image_in_room(&self, room_id: Uuid) -> bool {
        self.selected_message_in_room(room_id)
            .and_then(|m| inline_image_url_in_body(&m.body))
            .is_some()
    }

    /// Display name for a user id with the trim + non-empty +
    /// `short_user_id` fallback. Single source of truth for chat-author
    /// labeling — `selected_message_author_in_room`,
    /// `message_author_in_room`, and the chat-scroll click dispatcher
    /// all route through this helper.
    pub fn username_for(&self, user_id: Uuid) -> String {
        self.usernames
            .get(&user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(user_id))
    }

    pub fn selected_message_author_in_room(&self, room_id: Uuid) -> Option<(Uuid, String)> {
        let user_id = self.selected_message_in_room(room_id)?.user_id;
        Some((user_id, self.username_for(user_id)))
    }

    /// Same shape as `selected_message_author_in_room` but for an arbitrary
    /// message id — used by mouse hit-testing in the chat scroll.
    pub fn message_author_in_room(
        &self,
        room_id: Uuid,
        message_id: Uuid,
    ) -> Option<(Uuid, String)> {
        let user_id = self.find_message_in_room(room_id, message_id)?.user_id;
        Some((user_id, self.username_for(user_id)))
    }

    /// Move the message cursor onto a specific message id in `room_id`. Used
    /// by mouse hit-testing; no-op if the message is not in the visible tail.
    /// Mirrors the field writes in `select_message_in_room` (clears the reply
    /// highlight + reaction-leader transient state, leaves the room selection
    /// alone). Returns `true` if the selection actually moved.
    pub fn select_message_by_id_in_room(&mut self, room_id: Uuid, message_id: Uuid) -> bool {
        if self.find_message_in_room(room_id, message_id).is_none() {
            return false;
        }
        self.reaction_leader_active = false;
        self.highlighted_message_id = None;
        self.pending_delete_message_id = None;
        let changed = self.selected_message_id != Some(message_id);
        if changed {
            self.selection_scroll.reset();
        }
        self.selected_message_id = Some(message_id);
        changed
    }

    /// Whether a message is present in the locally loaded tail for a room.
    pub(crate) fn message_is_loaded_in_room(&self, room_id: Uuid, message_id: Uuid) -> bool {
        self.find_message_in_room(room_id, message_id).is_some()
    }

    /// Fire a message search through the service, superseding any in-flight
    /// request (latest wins: stale results are dropped by request id).
    pub(crate) fn start_message_search(&mut self, room_id: Option<Uuid>, query: String) {
        let request_id = Uuid::now_v7();
        self.message_search.begin(request_id, query.clone());
        let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
        self.service.search_messages_task(
            self.user_id,
            request_id,
            room_id,
            query,
            exclude_user_ids,
        );
    }

    /// Remember a search hit to select once its room tail loads. Used when
    /// jumping from the Ctrl+/ message search to a room whose history is not
    /// loaded yet.
    pub(crate) fn set_pending_search_jump(&mut self, room_id: Uuid, message_id: Uuid) {
        self.pending_search_jump = Some((room_id, message_id));
    }

    /// Open the history modal at a room's newest messages (the `/history`
    /// path for a caught-up room).
    pub(crate) fn open_history_at_tail(&mut self, room_id: Uuid) {
        let request_id = Uuid::now_v7();
        self.history_modal.open_at_tail(
            room_id,
            self.history_room_label(room_id),
            request_id,
            self.user_id,
            self.history_unread_cutoff(room_id),
        );
        let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
        self.service.load_history_page_task(
            self.user_id,
            request_id,
            room_id,
            None,
            HistoryDirection::Older,
            exclude_user_ids,
        );
    }

    /// Open the history modal centered on one message: the destination for a
    /// search hit or mention that sits further back than the live room tail
    /// reaches.
    pub(crate) fn open_history_at_message(&mut self, room_id: Uuid, message_id: Uuid) {
        let request_id = Uuid::now_v7();
        self.history_modal.open_at_message(
            room_id,
            self.history_room_label(room_id),
            message_id,
            request_id,
            self.user_id,
            self.history_unread_cutoff(room_id),
        );
        let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
        self.service.load_history_anchor_task(
            self.user_id,
            request_id,
            message_id,
            exclude_user_ids,
        );
    }

    /// Open the history modal at the room's first unread message (the
    /// `/history` path while the unread marker is set). The service resolves
    /// the exact first unread even when it sits further back than the live
    /// tail's 500; a room with nothing unread server-side falls back to a
    /// plain tail page under the same request id.
    pub(crate) fn open_history_at_unread(&mut self, room_id: Uuid, cutoff: DateTime<Utc>) {
        let request_id = Uuid::now_v7();
        self.history_modal.open_at_unread(
            room_id,
            self.history_room_label(room_id),
            request_id,
            self.user_id,
            Some(cutoff),
        );
        let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
        self.service.load_history_unread_task(
            self.user_id,
            request_id,
            room_id,
            cutoff,
            exclude_user_ids,
        );
    }

    /// The unread cutoff the history modal's divider draws against: this
    /// session's AFK line for the room. No line (you never stepped away, or
    /// you have caught up since) yields no divider, mirroring the live tail.
    ///
    /// `/history` only reads the line, never clears it: opening scrollback
    /// is how you go and look at the backlog, not proof you got through it,
    /// and the anchor should still be there when you close the modal.
    fn history_unread_cutoff(&self, room_id: Uuid) -> Option<DateTime<Utc>> {
        self.afk_lines.get(&room_id).copied()
    }

    /// Title for the history modal. A public-room mention can point at a room
    /// the rail has never listed, so an unknown room gets a neutral label
    /// rather than blocking the open.
    fn history_room_label(&self, room_id: Uuid) -> String {
        crate::app::room_search_modal::state::hit_room_label(self, self.user_id, room_id)
    }

    /// Fetch the next page if the viewport has reached that edge. Called
    /// after every scroll; `wants_page` owns the "is there more, and is a
    /// fetch already running" decision.
    pub(crate) fn request_history_page_if_needed(&mut self, direction: HistoryDirection) {
        if !self.history_modal.wants_page(direction) {
            return;
        }
        let Some(room_id) = self.history_modal.room_id() else {
            return;
        };
        let cursor = match direction {
            HistoryDirection::Older => self.history_modal.older_cursor(),
            HistoryDirection::Newer => self.history_modal.newer_cursor(),
        };
        let request_id = Uuid::now_v7();
        self.history_modal.begin_page(direction, request_id);
        let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
        self.service.load_history_page_task(
            self.user_id,
            request_id,
            room_id,
            cursor,
            direction,
            exclude_user_ids,
        );
    }

    /// Lazily fetch the context window (3 messages either side) for a search
    /// hit if it is not cached and no other context fetch is running. Called
    /// from the modal's tick for the currently selected hit; the single
    /// in-flight slot makes fast scrolling converge instead of fanning out.
    pub(crate) fn ensure_search_hit_context(&mut self, message_id: Uuid) {
        if self.message_search.context.contains_key(&message_id)
            || self.message_search.context_in_flight.is_some()
        {
            return;
        }
        let Some(hit) = self
            .message_search
            .hits
            .iter()
            .find(|hit| hit.message.id == message_id)
        else {
            return;
        };
        let request_id = Uuid::now_v7();
        self.message_search.context_in_flight = Some((request_id, message_id));
        let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
        self.service.load_message_context_task(
            self.user_id,
            request_id,
            hit.message.room_id,
            message_id,
            hit.message.created,
            exclude_user_ids,
        );
    }

    /// Load one message as a single-hit search preview (the Mentions
    /// fallback for messages older than the loaded history). The result
    /// arrives through the same latest-wins search pipeline, so typing a
    /// real `?` query afterwards simply replaces the preview.
    pub(crate) fn start_message_preview(&mut self, message_id: Uuid) {
        let request_id = Uuid::now_v7();
        self.message_search.begin(request_id, String::new());
        self.service
            .load_message_preview_task(self.user_id, request_id, message_id);
    }

    /// Drop the user into compose mode in `room_id` (if not already) and
    /// append `@username ` at the textarea cursor. Used by the chat-scroll
    /// double-click-username gesture. Composer text already in the box is
    /// preserved.
    pub fn insert_mention_in_room(&mut self, room_id: Uuid, username: &str) {
        let trimmed = username.trim();
        if trimmed.is_empty() {
            return;
        }
        if !self.composing || self.composer_room_id != Some(room_id) {
            self.start_composing_in_room(room_id);
        }
        // Mirror `ac_confirm`'s pattern: insert a space-terminated mention at
        // the cursor so subsequent typing flows naturally.
        self.composer.insert_str(format!("@{trimmed} "));
        let composing = self.composing;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, composing);
    }

    pub fn open_selected_news_modal_in_room(&mut self, room_id: Uuid) -> bool {
        self.reaction_leader_active = false;
        let Some((chat_payload, user_id, created)) =
            self.selected_message_in_room(room_id).and_then(|m| {
                parse_news_payload(&m.body).map(|payload| (payload, m.user_id, m.created))
            })
        else {
            return false;
        };

        let (payload, author, created, article_id) = if let Some((payload, author, created, id)) =
            news_modal_source_from_articles(self.news.all_articles(), &chat_payload.url)
        {
            (payload, author, created, Some(id))
        } else {
            let author =
                modal_author_label(self.usernames.get(&user_id).map(String::as_str), user_id);
            (chat_payload, author, created, None)
        };
        let relative = crate::app::common::primitives::format_relative_time(created);
        let meta = format!(
            "{author} - {relative} - {}",
            created.format("%a %Y-%m-%d %H:%M UTC")
        );
        self.news_modal = Some(NewsModalState {
            payload,
            meta,
            article_id,
        });
        true
    }

    pub fn open_selected_image_modal_in_room(&mut self, room_id: Uuid) -> bool {
        self.reaction_leader_active = false;
        let Some((message_id, url)) = self.selected_message_in_room(room_id).and_then(|message| {
            inline_image_url_in_body(&message.body).map(|url| (message.id, url))
        }) else {
            return false;
        };
        self.terminal_image_failed.remove(&message_id);
        self.image_modal = Some(ImageModalState { message_id, url });
        true
    }

    pub fn react_to_selected_message_in_room(
        &mut self,
        room_id: Uuid,
        icon: String,
    ) -> Option<Banner> {
        self.reaction_leader_active = false;
        let message = self.selected_message_in_room(room_id)?;
        self.service
            .toggle_message_reaction_task(self.user_id, message.id, icon);
        None
    }

    /// What the gild picker needs about the selected message, or the refusal
    /// the service would answer with anyway. The picker never opens on a
    /// message that could only be refused: your own message, one in a DM or
    /// private room, or one in a game or stream chat. Only the rules a room
    /// list can answer live here; the rest (membership, bots, cooldown,
    /// balance) stay with the service.
    pub(crate) fn gild_target_in_room(&self, room_id: Uuid) -> Result<GildTarget, GildRefusal> {
        let Some(message) = self.selected_message_in_room(room_id) else {
            return Err(GildRefusal::MessageNotFound);
        };
        let Some(room) = self.room_by_id(room_id) else {
            return Err(GildRefusal::MessageNotFound);
        };
        if room.visibility != "public" {
            return Err(GildRefusal::NotPublic);
        }
        if room.kind == "game" {
            return Err(GildRefusal::GameRoom);
        }
        if message.user_id == self.user_id {
            return Err(GildRefusal::SelfGild);
        }
        Ok(GildTarget {
            message_id: message.id,
            author_username: self.username_for(message.user_id),
            // One line, always: the picker prints the preview in a single
            // row, and a multi-line body would otherwise walk over the
            // tier rows below it.
            preview: message
                .body
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        })
    }

    /// Ask the service to buy a gild. Fire and forget: the answer arrives as
    /// a banner, and the marker arrives as a repaint.
    pub fn gild_message(&self, message_id: Uuid, tier: GildTier) {
        self.service
            .gild_message_task(self.user_id, message_id, tier);
    }

    fn find_message_in_room(&self, room_id: Uuid, message_id: Uuid) -> Option<&ChatMessage> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, msgs)| msgs.iter().find(|m| m.id == message_id))
    }

    fn room_slug(&self, room_id: Uuid) -> Option<String> {
        room_slug_for(&self.rooms, room_id)
    }

    /// Who may set this room's topic and rules. Private topic rooms answer to
    /// their derived owner, every other room with info answers to the house,
    /// and DMs and game rooms have no info at all.
    pub(crate) fn room_info_authority(&self, room: &ChatRoom) -> RoomInfoAuthority {
        if room.kind == "dm" || room.kind == "game" {
            return RoomInfoAuthority::NotEditable;
        }
        if room.kind == "topic"
            && room.visibility == "private"
            && self.room_owner_ids.get(&room.id) == Some(&self.user_id)
        {
            return RoomInfoAuthority::Owner;
        }
        if self.permissions.can_moderate() {
            return RoomInfoAuthority::Moderator;
        }
        RoomInfoAuthority::None
    }

    /// The owner line the room-info form shows.
    fn room_owner_label(&self, room: &ChatRoom) -> String {
        if room.kind != "topic" || room.visibility != "private" {
            return "moderators".to_string();
        }
        match self.room_owner_ids.get(&room.id) {
            Some(owner) if *owner == self.user_id => "you".to_string(),
            Some(owner) => self
                .usernames
                .get(owner)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            None => "unowned".to_string(),
        }
    }

    pub(crate) fn room_by_id(&self, room_id: Uuid) -> Option<&ChatRoom> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(room, _)| room)
    }

    /// Enabled voice channel for a chat room, if one exists.
    pub(crate) fn room_voice_channel_id(&self, room_id: Uuid) -> Option<Uuid> {
        self.voice_channels_by_room_id
            .get(&room_id)
            .filter(|channel| channel.enabled)
            .map(|channel| channel.id)
    }

    /// Whether the room the composer is currently in owns the room-scoped
    /// command `name`. Room-scoped command branches in `submit_composer` guard
    /// on this so they only fire in their owning room (and fall through to the
    /// "unknown command" handler elsewhere).
    fn composer_room_owns_command(&self, command: RoomScopedCommand) -> bool {
        self.composer_room_id
            .and_then(|id| self.room_by_id(id))
            .is_some_and(|room| room_owns_command(room, command.name()))
    }

    fn room_membership_command_target(&self) -> Option<Uuid> {
        room_membership_command_target(self.composer_room_id, self.selected_slot_state())
    }

    fn selected_slot_state(&self) -> SelectedRoomSlotState {
        SelectedRoomSlotState {
            selected_room_id: self.selected_room_id,
            feeds_selected: self.feeds_selected,
            news_selected: self.news_selected,
            cyberspace_selected: self.cyberspace_selected,
            cyberspace_notifications_selected: self.cyberspace_notifications_selected,
            cyberspace_room_selected: self.cyberspace_room_selected,
            cyberspace_mail_selected: self.cyberspace_mail_selected,
            notifications_selected: self.notifications_selected,
            discover_selected: self.discover_selected,
            showcase_selected: self.showcase_selected,
            work_selected: self.work_selected,
        }
    }

    /// The room slot currently selected, if any.
    fn current_slot(&self) -> Option<RoomSlot> {
        current_slot_from_state(self.selected_slot_state())
    }

    /// Whether a synthetic rail entry (rss, news, cyberspace, mentions,
    /// browse rooms, showcase, work) owns the center pane instead of a real
    /// room. The shell asks this instead of re-deriving the list: a new
    /// synthetic entry that misses one of those chains renders the room
    /// behind it while the rail says otherwise.
    pub fn synthetic_entry_selected(&self) -> bool {
        synthetic_entry_selected(self.selected_slot_state())
    }

    /// Collapse/expand a room-list section. If collapsing hides the currently
    /// selected room, selection snaps to the first still-visible slot so the
    /// cursor never ends up stranded inside a hidden section.
    pub(crate) fn toggle_section(&mut self, section: RoomSection) {
        if !self.collapsed_sections.remove(&section) {
            self.collapsed_sections.insert(section);
        }
        let order = self.visual_order();
        let still_visible = match self.current_slot() {
            Some(slot) => order.contains(&slot),
            None => true,
        };
        if !still_visible && let Some(&first) = order.first() {
            self.select_room_slot(first);
        }
    }

    fn selected_synthetic_entry_label(&self) -> Option<&'static str> {
        if self.news_selected {
            Some("news")
        } else if self.feeds_selected {
            Some("rss")
        } else if self.cyberspace_selected {
            Some("cyberspace")
        } else if self.cyberspace_notifications_selected {
            Some("cyberspace notifications")
        } else if self.notifications_selected {
            Some("mentions")
        } else if self.discover_selected {
            Some("browse rooms")
        } else if self.showcase_selected {
            Some("showcase")
        } else if self.work_selected {
            Some("work")
        } else {
            None
        }
    }

    fn leave_selected_synthetic_entry(&mut self) -> Option<&'static str> {
        let label = self.selected_synthetic_entry_label()?;
        self.feeds_selected = false;
        self.news_selected = false;
        self.cyberspace_selected = false;
        self.cyberspace_notifications_selected = false;
        self.notifications_selected = false;
        self.discover_selected = false;
        self.showcase_selected = false;
        self.work_selected = false;

        if self.selected_room_id.is_none() {
            self.selected_room_id = self
                .rooms
                .iter()
                .find(|(room, _)| is_chat_list_room(room))
                .map(|(room, _)| room.id);
        }
        if let Some(room_id) = self.selected_room_id {
            self.visible_room_id = Some(room_id);
            self.mark_room_read(room_id);
            self.request_room_tail(room_id);
        }

        Some(label)
    }

    pub fn lounge_room_id(&self) -> Option<Uuid> {
        self.lounge_room_id.or_else(|| {
            self.rooms
                .iter()
                .find(|(room, _)| room.kind == "lounge" && room.slug.as_deref() == Some("lounge"))
                .map(|(room, _)| room.id)
        })
    }

    pub(crate) fn set_favorite_room_ids(&mut self, favorite_room_ids: Vec<Uuid>) {
        self.favorite_room_ids = favorite_room_ids;
    }

    pub(crate) fn favorite_room_ids(&self) -> &[Uuid] {
        &self.favorite_room_ids
    }

    pub(crate) fn selected_favorite_room_id(&self) -> Option<Uuid> {
        if self.synthetic_entry_selected() {
            return None;
        }
        let room_id = self.selected_room_id?;
        self.rooms
            .iter()
            .any(|(room, _)| room.id == room_id && is_chat_list_room(room))
            .then_some(room_id)
    }

    /// Build the flat visual navigation order.
    /// Order matches the cozy rail exactly: favorites, core/mentions/news/rss,
    /// unread DMs, channels, DMs.
    pub(crate) fn visual_order(&self) -> Vec<RoomSlot> {
        visual_order_for_rooms(RoomVisualOrderInput {
            rooms: &self.rooms,
            user_id: self.user_id,
            usernames: &self.usernames,
            unread_counts: &self.unread_counts,
            room_last_message_at: &self.room_last_message_at,
            feeds_available: self.feeds.has_feeds(),
            cyberspace_linked: self.cyberspace.is_linked(),
            cyberspace_rooms: self.cyberspace.pinned_rooms(),
            cyberspace_mail: self.cyberspace.pinned_cmail(),
            favorite_room_ids: &self.favorite_room_ids,
            collapsed_sections: &self.collapsed_sections,
            ignored_user_ids: &self.ignored_user_ids,
            sticky_unread_dm: self.sticky_unread_dm,
            live_streams: &self.live_streams,
        })
    }

    pub(crate) fn room_jump_targets(&self) -> Vec<(u8, RoomSlot)> {
        self.visual_order()
            .into_iter()
            .zip(ROOM_JUMP_KEYS.iter().copied())
            .map(|(slot, key)| (key, slot))
            .collect()
    }

    fn adjacent_composer_room(&self, delta: isize) -> Option<Uuid> {
        adjacent_composer_room(
            &self.visual_order(),
            self.composer_room_id.or(self.selected_room_id),
            delta,
        )
    }

    pub(crate) fn select_room_slot(&mut self, slot: RoomSlot) -> bool {
        self.selected_message_id = None;
        self.selection_scroll.reset();
        self.reaction_leader_active = false;
        self.highlighted_message_id = None;

        match slot {
            RoomSlot::Feeds => {
                let changed = !self.feeds_selected;
                self.select_feeds();
                changed
            }
            RoomSlot::News => {
                let changed = !self.news_selected;
                self.select_news();
                changed
            }
            RoomSlot::Cyberspace => {
                let changed = !self.cyberspace_selected;
                self.select_cyberspace();
                changed
            }
            RoomSlot::CyberspaceNotifications => {
                let changed = !self.cyberspace_notifications_selected;
                self.select_cyberspace_notifications();
                changed
            }
            RoomSlot::CyberspaceRoom(index) => {
                let changed = self.cyberspace_room_selected != Some(index);
                self.select_cyberspace_room(index);
                changed
            }
            RoomSlot::CyberspaceMail(index) => {
                let changed = self.cyberspace_mail_selected != Some(index);
                self.select_cyberspace_mail(index);
                changed
            }
            RoomSlot::Notifications => {
                let changed = !self.notifications_selected;
                self.select_notifications();
                changed
            }
            RoomSlot::Discover => {
                let changed = !self.discover_selected;
                self.select_discover();
                changed
            }
            RoomSlot::Showcase => {
                let changed = !self.showcase_selected;
                self.select_showcase();
                changed
            }
            RoomSlot::Work => {
                let changed = !self.work_selected;
                self.select_work();
                changed
            }
            RoomSlot::Room(next_id) => {
                let is_stream_room = self.stream_for_room(next_id).is_some();
                let in_list = self.rooms.iter().any(|(room, _)| {
                    room.id == next_id
                        && (is_chat_list_room(room) || (is_stream_room && room.kind == "game"))
                });
                if !in_list {
                    // A stream room the user has never joined: select it
                    // anyway and join lazily (public game-room join path);
                    // the tail request rides the `GameRoomJoined` event.
                    if is_stream_room {
                        self.join_game_room_chat(next_id);
                    } else {
                        return false;
                    }
                }
                let changed = self.feeds_selected
                    || self.news_selected
                    || self.cyberspace_selected
                    || self.cyberspace_notifications_selected
                    || self.cyberspace_room_selected.is_some()
                    || self.cyberspace_mail_selected.is_some()
                    || self.notifications_selected
                    || self.discover_selected
                    || self.showcase_selected
                    || self.work_selected
                    || self.selected_room_id != Some(next_id);
                // Clearing here also drops any open cyberspace chat room,
                // which is what stops its stream and heartbeat: a room the
                // user has navigated away from must not keep fetching.
                self.clear_synthetic_selection();
                self.selected_room_id = Some(next_id);
                if !changed {
                    self.mark_room_read(next_id);
                }
                if changed && is_stream_room {
                    // Walking into someone's stream room is one of the two
                    // identified ways into a stream (`/watch @user` is the
                    // other). `App` turns this into the "is watching" line
                    // and the streamer's notification; the once-per-viewer
                    // dedupe lives in the stream registry, so re-opening the
                    // room is free.
                    self.opened_stream_room = Some(next_id);
                }
                changed
            }
        }
    }

    /// Switch to the adjacent room while keeping an in-progress composer
    /// draft in place. Reply/edit targets are dropped (they reference a
    /// message in the prior room, and carrying them across would submit
    /// to the wrong thread) and the composer is re-anchored to the new
    /// room so `submit_composer` posts to the correct place.
    ///
    /// Returns `true` if the selection actually changed.
    pub fn switch_room_preserving_draft(&mut self, delta: isize) -> bool {
        let Some(next_room_id) = self.adjacent_composer_room(delta) else {
            return false;
        };
        if !self.select_room_slot(RoomSlot::Room(next_room_id)) {
            return false;
        }
        self.reply_target = None;
        self.edited_message_id = None;
        self.composer_room_id = Some(next_room_id);
        self.visible_room_id = Some(next_room_id);
        self.mark_room_read(next_room_id);
        self.request_list();
        true
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        let order = self.visual_order();
        if order.is_empty() {
            return false;
        }

        let current_item = if self.feeds_selected {
            RoomSlot::Feeds
        } else if self.cyberspace_selected {
            RoomSlot::Cyberspace
        } else if self.cyberspace_notifications_selected {
            RoomSlot::CyberspaceNotifications
        } else if let Some(index) = self.cyberspace_room_selected {
            RoomSlot::CyberspaceRoom(index)
        } else if let Some(index) = self.cyberspace_mail_selected {
            RoomSlot::CyberspaceMail(index)
        } else if self.notifications_selected {
            RoomSlot::Notifications
        } else if self.discover_selected {
            RoomSlot::Discover
        } else if self.showcase_selected {
            RoomSlot::Showcase
        } else if self.work_selected {
            RoomSlot::Work
        } else if self.news_selected {
            RoomSlot::News
        } else {
            self.selected_room_id
                .map(RoomSlot::Room)
                .unwrap_or(RoomSlot::News)
        };
        let current = order
            .iter()
            .position(|item| *item == current_item)
            .unwrap_or(0) as isize;
        let next = wrapped_index(current, delta, order.len());
        self.select_room_slot(order[next])
    }

    pub fn activate_room_jump(&mut self) {
        self.room_jump_active = !self.composing && !self.rooms.is_empty();
    }

    pub fn cancel_room_jump(&mut self) {
        self.room_jump_active = false;
    }

    pub fn handle_room_jump_key(&mut self, byte: u8) -> bool {
        let targets = self.room_jump_targets();
        let Some(slot) = resolve_room_jump_target(&targets, byte) else {
            self.room_jump_active = false;
            return false;
        };

        self.room_jump_active = false;
        self.select_room_slot(slot)
    }

    pub fn stop_composing(&mut self) {
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reaction_leader_active = false;
        self.reply_target = None;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, false);
    }

    pub fn reset_composer(&mut self) {
        self.composer = new_chat_textarea();
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reaction_leader_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
        self.mention_ac = MentionAutocomplete::default();
    }

    fn clear_composer_after_submit(&mut self) {
        self.composer = new_chat_textarea();
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reaction_leader_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
    }

    fn clear_composer_after_send(&mut self) {
        self.composer = new_chat_textarea();
        composer::set_themed_textarea_cursor_visible(&mut self.composer, self.composing);
        self.room_jump_active = false;
        self.reaction_leader_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
    }

    fn open_overlay(&mut self, title: &str, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        self.overlay = Some(Overlay::new(title, lines));
    }

    fn open_members_overlay(&mut self, title: &str, members: Vec<RoomMemberListItem>) {
        self.overlay = Some(Overlay::styled(
            title,
            format_member_overlay_lines(&members, self.active_users.as_ref()),
        ));
    }

    /// The `ff` overlay: gilds first, one block per tier held (best tier
    /// first, since that is what the buyers paid for), then one block per
    /// reaction icon. Gilds are per buyer, so a tier block's names are the
    /// people holding exactly that tier on this message.
    fn reaction_owner_lines(
        &self,
        gilds: &[ChatMessageGild],
        owners: &[ChatMessageReactionOwners],
    ) -> Vec<String> {
        if gilds.is_empty() && owners.is_empty() {
            return vec!["No reactions yet".to_string()];
        }

        let mut lines = Vec::new();
        for tier in GildTier::ALL.iter().rev() {
            let buyers: Vec<Uuid> = gilds
                .iter()
                .filter(|gild| gild.tier == *tier)
                .map(|gild| gild.user_id)
                .collect();
            if buyers.is_empty() {
                continue;
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            let count = buyers.len();
            let noun = if count == 1 { "gild" } else { "gilds" };
            lines.push(format!(
                "{} {} {} {}",
                tier.marker(),
                count,
                tier.label(),
                noun
            ));
            lines.extend(self.owner_name_rows(&buyers));
        }
        for reaction in owners {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            let count = reaction.user_ids.len();
            let noun = if count == 1 { "reaction" } else { "reactions" };
            lines.push(format!("{} {} {}", reaction.icon, count, noun));

            if reaction.user_ids.is_empty() {
                lines.push("  unknown".to_string());
                continue;
            }
            lines.extend(self.owner_name_rows(&reaction.user_ids));
        }
        lines
    }

    /// `@name` labels for one block of the `ff` overlay, capped at
    /// `REACTION_OWNER_DISPLAY_LIMIT` with a `[+N more]` tail, wrapped
    /// `REACTION_OWNER_COLUMNS` per row.
    fn owner_name_rows(&self, user_ids: &[Uuid]) -> Vec<String> {
        let mut labels: Vec<String> = user_ids
            .iter()
            .take(REACTION_OWNER_DISPLAY_LIMIT)
            .map(|user_id| {
                self.usernames
                    .get(user_id)
                    .map(|name| name.trim())
                    .filter(|name| !name.is_empty())
                    .map(|name| format!("@{name}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(*user_id)))
            })
            .collect();
        let hidden_count = user_ids.len().saturating_sub(REACTION_OWNER_DISPLAY_LIMIT);
        if hidden_count > 0 {
            labels.push(format!("[+{hidden_count} more]"));
        }
        labels
            .chunks(REACTION_OWNER_COLUMNS)
            .map(|row| format!("  {}", row.join(" ")))
            .collect()
    }

    fn ignore_list_lines(&self) -> Vec<String> {
        if self.ignored_user_ids.is_empty() {
            return vec!["Ignore list is empty".to_string()];
        }

        let mut labels: Vec<String> = self
            .ignored_user_ids
            .iter()
            .map(|id| {
                self.usernames
                    .get(id)
                    .map(|name| format!("@{name}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(*id)))
            })
            .collect();
        labels.sort();
        labels
    }

    fn friend_list_lines(&self) -> Vec<String> {
        if self.friend_user_ids.is_empty() {
            return vec!["Friends list is empty".to_string()];
        }

        let active_users = self.active_users.as_ref().map(|users| users.lock_recover());
        let mut labels: Vec<String> = self
            .friend_user_ids
            .iter()
            .map(|id| {
                let username = self.usernames.get(id).cloned().or_else(|| {
                    active_users
                        .as_ref()
                        .and_then(|users| users.get(id))
                        .map(|user| user.username.clone())
                });
                let username =
                    username.unwrap_or_else(|| format!("<unknown:{}>", short_user_id(*id)));
                if active_users
                    .as_ref()
                    .is_some_and(|users| users.contains_key(id))
                {
                    format!("★ @{username} online")
                } else {
                    format!("★ @{username}")
                }
            })
            .collect();
        labels.sort();
        labels
    }

    fn active_user_lines(&self) -> Vec<String> {
        format_active_user_lines(self.active_users.as_ref(), &self.friend_user_ids)
    }

    pub(crate) fn open_active_users_overlay(&mut self) {
        self.open_overlay("Active Users", self.active_user_lines());
    }

    pub fn submit_composer(&mut self, keep_open: bool, _from_dashboard: bool) -> Option<Banner> {
        let body = self.composer.lines().join("\n").trim_end().to_string();

        if body.trim() == "/binds" {
            self.clear_composer_after_submit();
            self.requested_help_topic = Some(HelpTopic::Chat);
            return None;
        }

        if body.trim() == "/settings" {
            self.clear_composer_after_submit();
            self.requested_settings_modal = true;
            return None;
        }

        if body.trim() == "/shop" {
            self.clear_composer_after_submit();
            self.requested_shop_modal = true;
            return None;
        }

        if let Some(command) = parse_cyberspace_command(&body) {
            self.clear_composer_after_submit();
            match command {
                // The pane belongs to linked accounts, and so does its rail
                // entry. An unlinked user gets the link modal over the room
                // they are already in, so nobody ends up inside a pane the
                // rail does not list.
                CyberspaceCommand::Open
                | CyberspaceCommand::Post
                | CyberspaceCommand::Chat
                | CyberspaceCommand::Mail
                | CyberspaceCommand::MailTo(_)
                    if !self.cyberspace.is_linked() =>
                {
                    self.cyberspace.open_link_modal();
                    return None;
                }
                CyberspaceCommand::Open => {
                    self.select_cyberspace();
                    self.pending_chat_screen_switch = true;
                    return None;
                }
                CyberspaceCommand::Post => {
                    self.select_cyberspace();
                    self.pending_chat_screen_switch = true;
                    return self.cyberspace.open_compose_modal();
                }
                CyberspaceCommand::Chat => {
                    self.select_cyberspace();
                    self.pending_chat_screen_switch = true;
                    return self.cyberspace.open_rooms_modal();
                }
                CyberspaceCommand::Mail => {
                    self.select_cyberspace();
                    self.pending_chat_screen_switch = true;
                    return self.cyberspace.open_cmail_modal();
                }
                // The row appears in the rail when their API answers, and
                // the chat tick walks the user into it: a conversation
                // started by name is one they asked to write in.
                CyberspaceCommand::MailTo(username) => {
                    return self.cyberspace.start_cmail(username);
                }
                CyberspaceCommand::Link => {
                    self.cyberspace.open_link_modal();
                    return None;
                }
                CyberspaceCommand::Unlink => {
                    self.cyberspace.unlink();
                    // The rail entry goes with the link, so the pane cannot
                    // stay selected behind it.
                    if self.cyberspace_selected || self.cyberspace_notifications_selected {
                        self.leave_selected_synthetic_entry();
                    }
                    return None;
                }
                CyberspaceCommand::Invalid => {
                    return Some(Banner::error(
                        "Usage: /cs [post|chat|mail|mail @user|link|unlink]",
                    ));
                }
            }
        }

        if body.trim() == "/mod" {
            self.clear_composer_after_submit();
            self.requested_mod_modal = true;
            return None;
        }

        if body.trim() == "/ultimate" {
            self.clear_composer_after_submit();
            self.requested_ultimate_modal = true;
            return None;
        }

        if body.trim() == "/icons" {
            self.clear_composer_after_submit();
            self.requested_icon_picker = true;
            return None;
        }

        if let Some(rest) = body.trim().strip_prefix("/search")
            && (rest.is_empty() || rest.starts_with(' '))
        {
            let query = rest.trim().to_string();
            self.clear_composer_after_submit();
            self.requested_message_search = Some(query);
            return None;
        }

        if let Some(rest) = body.trim().strip_prefix("/summary")
            && (rest.is_empty() || rest.starts_with(' '))
        {
            self.clear_composer_after_submit();
            let Some(room_id) = self.visible_room_id else {
                return Some(Banner::error("open a room first"));
            };
            let Some((room, _)) = self.rooms.iter().find(|(room, _)| room.id == room_id) else {
                return Some(Banner::error("open a room first"));
            };
            // Public rooms only, checked again in the SQL; this one is for
            // an immediate, honest banner.
            if room.visibility != "public" {
                return Some(Banner::error("Summaries cover public rooms only"));
            }
            let window = match parse_summary_arg(rest) {
                // Always the device mark, never the room's AFK line: the
                // catch-up answers "what happened since I was last here",
                // and here is this device. The service only clamps it.
                SummaryArg::CatchUp => catch_up_window(self.device_left_at),
                SummaryArg::Window(back) => SummaryWindow::Explicit(back),
                SummaryArg::Unparseable => {
                    return Some(Banner::error("Use /summary, or a window like /summary 6h"));
                }
                SummaryArg::TooShort => {
                    return Some(Banner::error("Summary windows start at 1m"));
                }
                SummaryArg::TooLong => {
                    return Some(Banner::error(&format!(
                        "Summaries reach back at most {}h",
                        crate::app::ai::summary::SUMMARY_MAX_WINDOW_HOURS
                    )));
                }
            };
            let exclude_user_ids: Vec<Uuid> = self.ignored_user_ids.iter().copied().collect();
            self.summary_service.request(
                self.user_id,
                room_id,
                self.history_room_label(room_id),
                window,
                exclude_user_ids,
            );
            // The line stays: the summary says what is below it, it does
            // not move where it is. Only speaking in the room does that.
            return Some(Banner::info("Summarizing…"));
        }

        if body.trim() == "/history" {
            self.clear_composer_after_submit();
            let Some(room_id) = self.visible_room_id else {
                return Some(Banner::error("open a room first"));
            };
            // With unread messages waiting, land on the first of them
            // instead of the tail; the cutoff is the session's pre-mark
            // unread marker, since the server cursor advanced the moment
            // the room was opened.
            match self.history_unread_cutoff(room_id) {
                Some(cutoff) => self.open_history_at_unread(room_id, cutoff),
                None => self.open_history_at_tail(room_id),
            }
            return None;
        }

        if let Some(parsed) = parse_pair_command(&body) {
            self.clear_composer_after_submit();
            match parsed {
                Some(request) => {
                    self.requested_pair = Some(request);
                    return None;
                }
                None => {
                    return Some(Banner::error("Usage: /pair @user"));
                }
            }
        }

        if let Some(parsed) = parse_golive_command(&body) {
            self.clear_composer_after_submit();
            self.requested_golive = Some(parsed);
            return None;
        }

        if let Some(parsed) = parse_crown_command(&body) {
            self.clear_composer_after_submit();
            let Some(command) = parsed else {
                return Some(Banner::error("Usage: /crown, or /crown take"));
            };
            self.requested_crown = Some(command);
            return None;
        }

        if let Some(parsed) = parse_pot_command(&body) {
            self.clear_composer_after_submit();
            let Some(command) = parsed else {
                return Some(Banner::error(&format!(
                    "Usage: /pot, or /pot buy N (1 to {})",
                    late_core::models::pot::POT_MAX_TICKETS_PER_DAY
                )));
            };
            self.requested_pot = Some(command);
            return None;
        }

        if let Some(target) = parse_user_command(&body, "/watch") {
            self.clear_composer_after_submit();
            match target {
                Some(name) => {
                    self.requested_watch = Some(name.to_string());
                    return None;
                }
                None => {
                    return Some(Banner::error("Usage: /watch @user"));
                }
            }
        }

        if let Some(parsed) = parse_pomodoro_command(&body) {
            self.clear_composer_after_submit();
            match parsed {
                PomodoroParse::Request(request) => {
                    self.requested_pomodoro = Some(request);
                    return None;
                }
                PomodoroParse::Invalid => {
                    return Some(Banner::error(
                        "Usage: /pomodoro [minutes] [label...] or /pomodoro stop",
                    ));
                }
            }
        }

        if body.trim() == "/poll" {
            let room_id = self.visible_real_room_id_for_poll();
            self.clear_composer_after_submit();
            let Some(room_id) = room_id else {
                return Some(Banner::error("Open a real room before starting a poll"));
            };
            self.service.check_poll_start_task(self.user_id, room_id);
            return Some(Banner::success("Checking poll availability..."));
        }

        if let Some(parsed) = parse_petname_command(&body) {
            self.clear_composer_after_submit();
            match parsed {
                PetnameParse::Invalid => {
                    return Some(Banner::error(
                        "Usage: /petname <name> (up to 24 chars), or /petname clear",
                    ));
                }
                PetnameParse::Request(request) => {
                    self.requested_petname = Some(request);
                    return None;
                }
            }
        }

        if let Some(target) = parse_user_command(&body, "/profile") {
            self.clear_composer_after_submit();
            match target {
                None => {
                    let username = self
                        .usernames
                        .get(&self.user_id)
                        .map(|name| name.trim())
                        .filter(|name| !name.is_empty())
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| short_user_id(self.user_id));
                    self.requested_open_profile = Some((self.user_id, username));
                }
                Some(name) => {
                    self.service
                        .open_profile_by_username_task(self.user_id, name.to_string());
                }
            }
            return None;
        }

        if body.trim().starts_with("/mod ") {
            self.clear_composer_after_submit();
            return Some(Banner::error(
                "open /mod first; moderation commands only run in the modal",
            ));
        }

        if body.trim() == "/exit" {
            self.clear_composer_after_submit();
            self.requested_quit = true;
            return None;
        }

        if let Some(command) = match body.trim() {
            "/voice" => Some(VoiceCommand::Join),
            "/mute" => Some(VoiceCommand::Mute),
            _ => None,
        } {
            self.clear_composer_after_submit();
            self.requested_voice_command = Some(command);
            return None;
        }

        if let Some(command) = match body.trim() {
            "/aquarium" | "/aq" => Some(AquariumCommand::Toggle),
            "/aquarium feed" | "/aq feed" => Some(AquariumCommand::Feed),
            _ => None,
        } {
            self.clear_composer_after_submit();
            self.requested_aquarium_command = Some(command);
            return None;
        }

        if let Some(command) = match body.trim() {
            "/pet" => Some(PetCommand::Toggle),
            "/pet feed" => Some(PetCommand::Feed),
            "/pet water" => Some(PetCommand::Water),
            _ => None,
        } {
            self.clear_composer_after_submit();
            self.requested_pet_command = Some(command);
            return None;
        }

        if body.trim() == "/audio skip" {
            self.clear_composer_after_submit();
            if !self.is_admin && !self.is_moderator {
                return Some(Banner::error("/audio is staff-only"));
            }
            self.requested_audio_skip = true;
            return None;
        }

        if let Some(url) = body.trim().strip_prefix("/audio fallback ") {
            let url = url.trim().to_string();
            self.clear_composer_after_submit();
            if !self.is_admin && !self.is_moderator {
                return Some(Banner::error("/audio is staff-only"));
            }
            if url.is_empty() {
                return Some(Banner::error("Usage: /audio fallback <youtube-url>"));
            }
            self.requested_audio_fallback_url = Some(url);
            return None;
        }

        if let Some(url) = body.trim().strip_prefix("/audio ") {
            let url = url.trim().to_string();
            self.clear_composer_after_submit();
            if !self.is_admin && !self.is_moderator {
                return Some(Banner::error("/audio is staff-only"));
            }
            if url.is_empty() {
                return Some(Banner::error("Usage: /audio <youtube-url>"));
            }
            self.requested_audio_url = Some(url);
            return None;
        }

        if let Some(url) = body.trim().strip_prefix("/upload ") {
            let url = url.trim().to_string();
            if url.is_empty() {
                return Some(Banner::error("Usage: /upload <url>"));
            }
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Some(Banner::error("/upload: URL must start with http(s)://"));
            }
            if self.files.is_none() {
                return Some(Banner::error("File uploads are disabled"));
            }
            let room_id = self.upload_target_room_id();
            let reply_target = self.reply_target.clone();
            self.clear_composer_after_submit();
            self.requested_url_upload = Some(PendingUrlUpload {
                url,
                room_id,
                reply_target,
            });
            return None;
        }

        if body.trim() == "/paste-image" {
            if self.files.is_none() {
                return Some(Banner::error("File uploads are disabled"));
            }
            self.clear_expired_pending_clipboard_image_upload();
            if self.pending_clipboard_image_upload.is_some()
                || self.requested_clipboard_image_upload.is_some()
            {
                return Some(Banner::error(
                    "A clipboard image request is already in progress",
                ));
            }
            let room_id = self.upload_target_room_id();
            let reply_target = self.reply_target.clone();
            self.clear_composer_after_submit();
            self.requested_clipboard_image_upload =
                Some(PendingClipboardImageUpload::new(room_id, reply_target));
            return None;
        }

        if body.trim() == "/active" {
            self.clear_composer_after_submit();
            self.open_active_users_overlay();
            return None;
        }

        if let Some(msg) = parse_brb_command(&body) {
            let chat_body = if msg.is_empty() {
                "🌙 brb".to_string()
            } else {
                format!("🌙 brb — {msg}")
            };
            let room_id = self.composer_room_id;
            if let Some(room_id) = room_id {
                self.service
                    .send_message_with_reply_task(super::svc::SendMessageTask {
                        user_id: self.user_id,
                        room_id,
                        room_slug: self.room_slug(room_id),
                        body: chat_body,
                        reply_to_message_id: None,
                        request_id: Uuid::now_v7(),
                        is_admin: self.is_admin,
                    });
            }
            self.requested_brb = Some(msg);
            self.clear_composer_after_submit();
            return None;
        }

        if let Some((kind, text)) = parse_report_command(&body) {
            self.clear_composer_after_submit();
            let Some(text) = text else {
                return Some(Banner::error(&format!(
                    "Usage: {} <describe it in a few words or more>",
                    kind.command()
                )));
            };
            let request_id = Uuid::now_v7();
            self.pending_send_notices.push_back(request_id);
            self.service
                .send_report_task(self.user_id, kind, text, request_id);
            return None;
        }

        if body.trim() == "/friends" {
            self.clear_composer_after_submit();
            self.open_overlay("Friends", self.friend_list_lines());
            return None;
        }

        if body.trim() == "/members" {
            // Resolve the target room BEFORE clearing the composer.
            // Synthetic entries can retain a stale `selected_room_id`, so
            // membership commands must go through the shared resolver.
            let target = self.room_membership_command_target();
            self.clear_composer_after_submit();
            let Some(room_id) = target else {
                return Some(Banner::error("No member-list room selected"));
            };
            self.service.list_room_members_task(self.user_id, room_id);
            return None;
        }

        if let Some(parsed) = parse_gift_command(&body) {
            self.clear_composer_after_submit();
            match parsed {
                GiftParse::Invalid => {
                    return Some(Banner::error("Usage: /gift @user <amount>"));
                }
                GiftParse::Gift {
                    username,
                    amount,
                    message,
                } => {
                    self.service
                        .gift_chips_task(self.user_id, username.clone(), amount, message);
                    return Some(Banner::success(&format!(
                        "Sending {amount} chips to @{username}..."
                    )));
                }
            }
        }

        if body.trim() == "/list" {
            self.clear_composer_after_submit();
            self.service.list_public_rooms_task(self.user_id);
            return None;
        }

        if let Some(target) = parse_user_command(&body, "/ignore") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Ignored Users", self.ignore_list_lines()),
                Some(name) => self
                    .service
                    .ignore_user_task(self.user_id, name.to_string()),
            }
            return None;
        }
        if let Some(target) = parse_user_command(&body, "/unignore") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Ignored Users", self.ignore_list_lines()),
                Some(name) => self
                    .service
                    .unignore_user_task(self.user_id, name.to_string()),
            }
            return None;
        }
        if let Some(target) = parse_user_command(&body, "/friend") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Friends", self.friend_list_lines()),
                Some(name) => self
                    .service
                    .friend_user_task(self.user_id, name.to_string()),
            }
            return None;
        }
        if let Some(target) = parse_user_command(&body, "/unfriend") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Friends", self.friend_list_lines()),
                Some(name) => self
                    .service
                    .unfriend_user_task(self.user_id, name.to_string()),
            }
            return None;
        }

        if let Some(target) = parse_dm_command(&body) {
            self.service.start_dm_task(self.user_id, target.to_string());
            self.clear_composer_after_submit();
            return Some(Banner::success(&format!("Opening DM with {target}...")));
        }

        // Public rooms are hosted, not owned: `/public` only ever opens or
        // joins one, and a mod sets its topic and rules afterwards.
        if let Some(room) = parse_public_room_command(&body) {
            if user_created_channel_name_too_long(room) {
                return Some(user_created_channel_name_length_error());
            }
            self.clear_composer_after_submit();
            self.service
                .open_public_room_task(self.user_id, room.to_string());
            return Some(Banner::success(&format!("Opening public #{room}...")));
        }

        // A private room is owned by whoever opens it, so it gets the form up
        // front and the creator's answers are saved with the room.
        if let Some(room) = parse_room_command(&body, "/private") {
            if user_created_channel_name_too_long(room) {
                return Some(user_created_channel_name_length_error());
            }
            self.clear_composer_after_submit();
            self.requested_room_info_modal = Some(RoomInfoRequest::Create {
                slug: room.to_string(),
            });
            return None;
        }

        if is_room_info_command(&body) {
            self.clear_composer_after_submit();
            let Some(room_id) = self.selected_room_id else {
                return Some(Banner::error("Select a room first"));
            };
            let Some(room) = self.room_by_id(room_id) else {
                return Some(Banner::error("No room selected"));
            };
            match self.room_info_authority(room) {
                RoomInfoAuthority::Owner | RoomInfoAuthority::Moderator => {}
                RoomInfoAuthority::None => {
                    // Private rooms have an owner who can; public ones do not.
                    return Some(Banner::error(
                        if room.kind == "topic" && room.visibility == "private" {
                            "Only this room's owner can set its topic and rules"
                        } else {
                            "Only a moderator can set this room's topic and rules"
                        },
                    ));
                }
                RoomInfoAuthority::NotEditable => {
                    return Some(Banner::error("This room has no topic or rules"));
                }
            }
            self.requested_room_info_modal = Some(RoomInfoRequest::Edit {
                room_id,
                room_label: room
                    .slug
                    .as_deref()
                    .map(|slug| format!("#{slug}"))
                    .unwrap_or_else(|| "this room".to_string()),
                owner_label: self.room_owner_label(room),
                topic: room.topic.clone(),
                rules: room.rules.clone(),
            });
            return None;
        }

        // Rules are multi-line by design, so they go in the same overlay
        // `/active` uses (scrollable, wraps long lines) rather than a banner,
        // which is one line and would eat the line breaks.
        if body.trim() == "/rules" {
            self.clear_composer_after_submit();
            let overlay = {
                let room = self
                    .selected_room_id
                    .and_then(|room_id| self.room_by_id(room_id));
                let rules = room
                    .and_then(|room| room.rules.as_deref())
                    .map(str::trim)
                    .filter(|rules| !rules.is_empty());
                match rules {
                    Some(rules) => {
                        let title = match room.and_then(|room| room.slug.as_deref()) {
                            Some(slug) => format!("#{slug} rules"),
                            None => "Rules".to_string(),
                        };
                        Some((title, rules.lines().map(str::to_string).collect::<Vec<_>>()))
                    }
                    None => None,
                }
            };
            let Some((title, lines)) = overlay else {
                return Some(Banner::info("No rules set for this room"));
            };
            self.open_overlay(&title, lines);
            return None;
        }

        // Kicking answers to the room, not the composer: a private room's owner
        // may remove a regular from it, staff may remove anyone anywhere, and
        // the service decides which of those applies.
        if let Some(target) = parse_user_command(&body, "/kick") {
            let room_id = self.room_membership_command_target();
            self.clear_composer_after_submit();
            let Some(room_id) = room_id else {
                return Some(Banner::error("No room selected"));
            };
            let Some(target) = target else {
                return Some(Banner::error("Usage: /kick @user"));
            };
            // A slug-less room is a DM: nothing to moderate there. The room
            // itself travels as its id, since slugs are not globally unique.
            if self.room_slug(room_id).is_none() {
                return Some(Banner::error("This room has no members to kick"));
            }
            self.service.room_mod_task(
                self.user_id,
                self.permissions,
                RoomModRequest {
                    action: RoomModAction::Kick,
                    room: RoomRef::Id(room_id),
                    username: target.to_string(),
                    duration: None,
                    reason: String::new(),
                },
            );
            return Some(Banner::success(&format!("Kicking @{target}...")));
        }

        // Banning is the same authorization path as kicking, and the one that
        // actually holds: a public room (a streamer's, above all) can be
        // re-entered from the rail the moment a kick lands.
        if let Some(request) = parse_room_ban_command(&body, "/ban") {
            let room_id = self.room_membership_command_target();
            self.clear_composer_after_submit();
            let Some(room_id) = room_id else {
                return Some(Banner::error("No room selected"));
            };
            let request = match request {
                Ok(request) => request,
                Err(usage) => return Some(Banner::error(usage)),
            };
            if self.room_slug(room_id).is_none() {
                return Some(Banner::error("This room has no members to ban"));
            }
            let target = request.username.to_string();
            self.service.room_mod_task(
                self.user_id,
                self.permissions,
                RoomModRequest {
                    action: RoomModAction::Ban,
                    room: RoomRef::Id(room_id),
                    username: target.clone(),
                    duration: request.duration,
                    reason: request.reason,
                },
            );
            return Some(Banner::success(&format!("Banning @{target}...")));
        }

        if let Some(request) = parse_room_ban_command(&body, "/unban") {
            let room_id = self.room_membership_command_target();
            self.clear_composer_after_submit();
            let Some(room_id) = room_id else {
                return Some(Banner::error("No room selected"));
            };
            let request = match request {
                Ok(request) => request,
                Err(_) => return Some(Banner::error("Usage: /unban @user")),
            };
            if self.room_slug(room_id).is_none() {
                return Some(Banner::error("This room has no bans to lift"));
            }
            let target = request.username.to_string();
            self.service.room_mod_task(
                self.user_id,
                self.permissions,
                RoomModRequest {
                    action: RoomModAction::Unban,
                    room: RoomRef::Id(room_id),
                    username: target.clone(),
                    duration: None,
                    reason: request.reason,
                },
            );
            return Some(Banner::success(&format!("Unbanning @{target}...")));
        }

        if let Some(target) = parse_user_command(&body, "/invite") {
            let room_id = self.room_membership_command_target();
            self.clear_composer_after_submit();
            let Some(room_id) = room_id else {
                return Some(Banner::error("No inviteable room selected"));
            };
            let Some(target) = target else {
                return Some(Banner::error("Usage: /invite @user"));
            };
            self.service
                .invite_user_to_room_task(self.user_id, room_id, target.to_string());
            return Some(Banner::success(&format!("Inviting @{target}...")));
        }

        if parse_leave_command(&body) {
            let target = self.room_membership_command_target();
            let slug = target
                .and_then(|room_id| self.room_slug(room_id))
                .unwrap_or_else(|| "room".to_string());
            self.clear_composer_after_submit();
            if let Some(room_id) = target {
                self.service
                    .leave_room_task(self.user_id, room_id, slug.clone());
                return Some(Banner::success(&format!("Leaving #{slug}...")));
            } else if let Some(label) = self.leave_selected_synthetic_entry() {
                return Some(Banner::success(&format!("Left #{label}")));
            } else {
                return Some(Banner::error("No leaveable room selected"));
            }
        }

        if let Some(slug) = parse_create_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.is_admin {
                return Some(Banner::error("Admin only: /create-room"));
            }
            self.service
                .create_permanent_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Creating #{slug}...")));
        }

        if let Some(slug) = parse_delete_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.is_admin {
                return Some(Banner::error("Admin only: /delete-room"));
            }
            self.service
                .delete_permanent_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Deleting #{slug}...")));
        }

        if let Some(slug) = parse_fill_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.is_admin {
                return Some(Banner::error("Admin only: /fill-room"));
            }
            self.service.fill_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Filling #{slug}...")));
        }

        if let Some(parsed) = parse_roll_command(&body) {
            let room_id = self.composer_room_id;
            self.clear_composer_after_submit();
            let specs = match parsed {
                RollParse::Invalid => {
                    return Some(Banner::error("Usage: /roll [NdM ...]"));
                }
                RollParse::Specs(specs) => specs,
            };
            let Some(room_id) = room_id else {
                return Some(Banner::error("Roll from inside a room"));
            };
            let rolls = roll_dice(&specs, &mut OsRng);
            let request_id = Uuid::now_v7();
            self.service
                .send_message_with_reply_task(super::svc::SendMessageTask {
                    user_id: self.user_id,
                    room_id,
                    room_slug: self.room_slug(room_id),
                    body: format_roll_result(&specs, &rolls),
                    reply_to_message_id: None,
                    request_id,
                    is_admin: self.is_admin,
                });
            self.pending_send_notices.push_back(request_id);
            return None;
        }

        if let Some(kind) = parse_cup_command(&body) {
            // Snapshot the composer's room before `clear_composer_after_submit`
            // wipes it — otherwise the send below has no room to target and
            // the ritual silently no-ops.
            let room_id = self.composer_room_id;
            self.clear_composer_after_submit();
            let room_id = room_id?;
            let variant = self.next_cup_variant;
            self.next_cup_variant = (variant + 1) % CUP_VARIANT_COUNT;
            let art = cup_art(kind, variant);
            let request_id = Uuid::now_v7();
            self.service
                .send_message_with_reply_task(super::svc::SendMessageTask {
                    user_id: self.user_id,
                    room_id,
                    room_slug: self.room_slug(room_id),
                    body: art,
                    reply_to_message_id: None,
                    request_id,
                    is_admin: self.is_admin,
                });
            self.pending_send_notices.push_back(request_id);
            return None;
        }

        if let Some(parsed) = parse_me_command(&body) {
            let Some(action_body) = parsed else {
                self.clear_composer_after_submit();
                return Some(Banner::error("Usage: /me <action>"));
            };
            let room_id = self.composer_room_id;
            self.clear_composer_after_submit();
            let Some(room_id) = room_id else {
                return Some(Banner::error("Send actions from inside a room"));
            };
            let request_id = Uuid::now_v7();
            self.service
                .send_message_with_reply_task(super::svc::SendMessageTask {
                    user_id: self.user_id,
                    room_id,
                    room_slug: self.room_slug(room_id),
                    body: action_body,
                    reply_to_message_id: None,
                    request_id,
                    is_admin: self.is_admin,
                });
            self.pending_send_notices.push_back(request_id);
            return None;
        }

        if let Some(target) = parse_user_command(&body, "/sheet")
            && self.composer_room_owns_command(RoomScopedCommand::Sheet)
        {
            let room_id = self.composer_room_id;
            self.clear_composer_after_submit();
            let room_id = room_id?;
            self.service
                .open_sheet_task(self.user_id, room_id, target.map(ToOwned::to_owned));
            return None;
        }

        if let Some(command) = unknown_slash_command(&body) {
            self.clear_composer_after_submit();
            return Some(Banner::error(&format!("Unknown command: {command}")));
        }

        let mut cooldown_banner = None;
        if let Some(room_id) = self.composer_room_id
            && !body.is_empty()
        {
            let request_id = Uuid::now_v7();
            let reply_to_message_id = self.reply_target.as_ref().map(|reply| reply.message_id);
            // Peek the ghost-bot ladders on the typed body, before the reply
            // quote is prepended, matching what the responders react to
            // (quoted lines never count as mentions on their side either).
            if self.edited_message_id.is_none() {
                cooldown_banner = self.bot_cooldown_banner(room_id, &body);
            }
            let body = if let Some(reply) = &self.reply_target {
                format!("> @{}: {}\n{}", reply.author, reply.preview, body)
            } else {
                body
            };
            self.sent_regular_message = true;
            if let Some(message_id) = self.edited_message_id {
                self.service.edit_message_task(
                    self.user_id,
                    message_id,
                    body,
                    request_id,
                    self.permissions,
                );
            } else {
                self.service
                    .send_message_with_reply_task(super::svc::SendMessageTask {
                        user_id: self.user_id,
                        room_id,
                        room_slug: self.room_slug(room_id),
                        body,
                        reply_to_message_id,
                        request_id,
                        is_admin: self.is_admin,
                    });
            }
            self.pending_send_notices.push_back(request_id);
        }
        if keep_open {
            self.clear_composer_after_send();
        } else {
            self.clear_composer_after_submit();
        }
        cooldown_banner
    }

    /// A submit-time heads-up that a mentioned ghost bot is still cooling
    /// down for this user in this room. The message still sends; the banner
    /// only says no reply is coming yet and when to retry.
    fn bot_cooldown_banner(&self, room_id: Uuid, body: &str) -> Option<Banner> {
        let mentioned = crate::app::common::mentions::extract_mentions(body);
        for bot in MentionLadders::ALL_BOTS {
            if mentioned.iter().any(|name| name == bot.handle())
                && let Some(remaining) = self.mention_ladders.remaining(bot, self.user_id, room_id)
            {
                return Some(Banner::info(&format!(
                    "@{} is cooling down, try again in {}",
                    bot.handle(),
                    format_cooldown(remaining)
                )));
            }
        }
        None
    }

    pub fn composer_clear(&mut self) {
        let composing = self.composing;
        self.composer = new_chat_textarea();
        composer::set_themed_textarea_cursor_visible(&mut self.composer, composing);
    }

    pub fn composer_backspace(&mut self) {
        self.composer.delete_char();
    }

    pub fn composer_delete_right(&mut self) {
        self.composer.delete_next_char();
    }

    pub fn composer_delete_word_right(&mut self) {
        self.composer.delete_next_word();
    }

    pub fn composer_delete_word_left(&mut self) {
        self.composer.delete_word();
    }

    pub fn composer_push(&mut self, ch: char) {
        self.composer.insert_char(ch);
    }

    pub fn composer_push_str(&mut self, s: &str) {
        self.composer.insert_str(s);
    }

    pub fn composer_cursor_left(&mut self) {
        self.composer.move_cursor(CursorMove::Back);
    }

    pub fn composer_cursor_right(&mut self) {
        self.composer.move_cursor(CursorMove::Forward);
    }

    pub fn composer_cursor_word_left(&mut self) {
        self.composer.move_cursor(CursorMove::WordBack);
    }

    pub fn composer_cursor_word_right(&mut self) {
        self.composer.move_cursor(CursorMove::WordForward);
    }

    pub fn composer_cursor_home(&mut self) {
        self.composer.move_cursor(CursorMove::Head);
    }

    pub fn composer_cursor_end(&mut self) {
        self.composer.move_cursor(CursorMove::End);
    }

    pub fn composer_cursor_up(&mut self) {
        self.composer.move_cursor(CursorMove::Up);
    }

    pub fn composer_cursor_down(&mut self) {
        self.composer.move_cursor(CursorMove::Down);
    }

    /// Move the composer cursor to the screen cell the user clicked inside the
    /// composer text area. `rect` is the composer block rect captured during
    /// render (`last_composer_rect`, including the top/bottom border rows);
    /// `x`/`y` are 0-based screen coordinates from the mouse event.
    ///
    /// The text is drawn one row below the top border and inset by one column
    /// on each side, mirroring `draw_composer_block` (which renders into
    /// `block.inner(TOP|BOTTOM)` then `horizontal_inset(1)`). We reuse the same
    /// word-wrap model the height estimator uses (`build_composer_rows`) so the
    /// clicked row lines up with what is painted, then translate the wrapped
    /// row + display column into a logical `(line, char)` cursor for `Jump`,
    /// which clamps anything past the end of the text.
    ///
    /// Known limitation: `build_composer_rows` wraps by char count and
    /// hard-splits long words, while the widget's `WrapMode::Word` wraps by
    /// display width and never splits a word wider than the bar. The two
    /// models agree on typical ASCII prose, but for multi-row CJK/emoji
    /// drafts or a pasted token longer than the composer width (e.g. a URL)
    /// the row boundaries diverge and the caret can land on a neighboring
    /// row. The same mismatch already affects composer height estimation;
    /// the real fix is a screen-to-cursor API on `ratatui-textarea` itself.
    pub(crate) fn composer_click_to_cursor(&mut self, rect: Rect, x: u16, y: u16) {
        let text_x = rect.x.saturating_add(1);
        let text_y = rect.y.saturating_add(1);
        let text_width = rect.width.saturating_sub(2) as usize;
        if text_width == 0 {
            return;
        }
        // Clicks on the top border or left padding clamp to the first row /
        // column 0 rather than bailing, so edge clicks still land sensibly.
        let viewport_top = self.last_composer_viewport_top.get().unwrap_or(0);
        let rel_row = viewport_top.saturating_add(y.saturating_sub(text_y) as usize);
        let rel_col = x.saturating_sub(text_x) as usize;

        let text = self.composer.lines().join("\n");
        let rows = composer::build_composer_rows(&text, text_width);
        let Some(row) = rows.get(rel_row.min(rows.len().saturating_sub(1))) else {
            return;
        };
        let within = char_offset_for_display_col(&row.text, rel_col);
        let global_char = row.start + within;
        let (line, col) = global_char_to_line_col(&text, global_char);
        self.composer
            .move_cursor(CursorMove::Jump(line as u16, col as u16));
    }

    pub fn composer_paste(&mut self) {
        self.composer.paste();
    }

    pub fn composer_undo(&mut self) {
        self.composer.undo();
    }

    /// Readline ^U: drop everything from the cursor back to the start of the
    /// current line, leaving later lines intact. Replaces the earlier
    /// clear-the-whole-composer behavior.
    pub fn composer_kill_to_head(&mut self) {
        self.composer.delete_line_by_head();
    }

    /// Forward a synthesized `Input` to the TextArea so it can dispatch via
    /// its built-in emacs/readline keymap (^A/^E/^K/^F/^B/...).
    pub fn composer_input(&mut self, input: Input) {
        self.composer.input(input);
    }

    /// Upload bytes that arrived while the composer is still open (a terminal
    /// image paste), so whatever reply it was aiming at is still live here.
    pub fn start_image_upload(&mut self, bytes: Vec<u8>) -> Option<Banner> {
        self.start_image_upload_in_room(
            bytes,
            self.upload_target_room_id(),
            self.reply_target.clone(),
        )
    }

    pub(crate) fn start_image_upload_in_room(
        &mut self,
        bytes: Vec<u8>,
        room_id: Option<Uuid>,
        reply_target: Option<ReplyTarget>,
    ) -> Option<Banner> {
        let Some(mime) = crate::app::files::image_upload::detect_image_mime(&bytes) else {
            return Some(Banner::error("Unsupported image type"));
        };
        let Some(files) = self.files.clone() else {
            return Some(Banner::error("File uploads are disabled"));
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Some(banner) = self.begin_image_upload(room_id, reply_target, rx) {
            return Some(banner);
        }
        let mime = mime.to_string();

        tokio::spawn(async move {
            let result = crate::app::files::image_upload::upload_image_bytes(&files, bytes, &mime)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });

        None
    }

    /// Upload storage for features outside chat state (URL uploads in input
    /// handling); `None` means uploads are disabled in this environment.
    pub(crate) fn files_config(&self) -> Option<&crate::config::FilesConfig> {
        self.files.as_ref()
    }

    pub(crate) fn upload_target_room_id(&self) -> Option<Uuid> {
        self.composer_room_id
            .or(self.visible_room_id)
            .or(self.selected_room_id)
    }

    pub(crate) fn begin_image_upload(
        &mut self,
        room_id: Option<Uuid>,
        reply_target: Option<ReplyTarget>,
        rx: tokio::sync::oneshot::Receiver<Result<String, String>>,
    ) -> Option<Banner> {
        if self.image_upload_pending {
            return Some(Banner::error("An image upload is already in progress"));
        }

        if !self.is_admin
            && let Some(last) = self.last_image_upload_at
            && last.elapsed() < std::time::Duration::from_secs(30)
        {
            let wait = 30 - last.elapsed().as_secs();
            return Some(Banner::error(&format!(
                "Please wait {}s before uploading another image",
                wait
            )));
        }

        self.image_upload_rx = Some(rx);
        self.image_upload_pending = true;
        self.image_upload_target_room_id = room_id;
        self.image_upload_reply_target = reply_target;
        self.last_image_upload_at = Some(std::time::Instant::now());
        None
    }

    pub(crate) fn take_image_upload_target_room_id(&mut self) -> Option<Uuid> {
        self.image_upload_target_room_id.take()
    }

    /// Put the finished upload's reply back on the composer. Reopening the
    /// composer with the URL goes through `start_composing_in_room`, which
    /// drops the reply target, so this runs after it.
    pub(crate) fn restore_image_upload_reply_target(&mut self) {
        self.reply_target = self.image_upload_reply_target.take();
    }

    pub(crate) fn clear_image_upload_reply_target(&mut self) {
        self.image_upload_reply_target = None;
    }

    pub(crate) fn take_requested_url_upload(&mut self) -> Option<PendingUrlUpload> {
        self.requested_url_upload.take()
    }

    pub(crate) fn take_requested_clipboard_image_upload(
        &mut self,
    ) -> Option<PendingClipboardImageUpload> {
        self.requested_clipboard_image_upload.take()
    }

    pub(crate) fn begin_pending_clipboard_image_upload(
        &mut self,
        room_id: Option<Uuid>,
        reply_target: Option<ReplyTarget>,
    ) {
        self.pending_clipboard_image_upload =
            Some(PendingClipboardImageUpload::new(room_id, reply_target));
    }

    pub(crate) fn take_pending_clipboard_image_upload(
        &mut self,
    ) -> Option<PendingClipboardImageUpload> {
        self.pending_clipboard_image_upload.take()
    }

    pub(crate) fn clear_pending_clipboard_image_upload(&mut self) {
        self.pending_clipboard_image_upload = None;
    }

    fn clear_expired_pending_clipboard_image_upload(&mut self) -> bool {
        if self
            .pending_clipboard_image_upload
            .as_ref()
            .is_some_and(PendingClipboardImageUpload::is_expired)
        {
            self.pending_clipboard_image_upload = None;
            return true;
        }
        false
    }

    pub(crate) fn expire_pending_clipboard_image_upload(&mut self) -> Option<Banner> {
        if self.clear_expired_pending_clipboard_image_upload() {
            return Some(Banner::error("Clipboard image request timed out"));
        }
        None
    }

    pub(crate) fn poll_image_upload(&mut self) -> Option<Result<String, String>> {
        let rx = self.image_upload_rx.as_mut()?;
        match rx.try_recv() {
            Ok(result) => {
                self.image_upload_rx = None;
                self.image_upload_pending = false;
                Some(result)
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.image_upload_rx = None;
                self.image_upload_pending = false;
                Some(Err("Upload cancelled".to_string()))
            }
        }
    }

    pub(crate) fn poll_inline_images(&mut self, settings: InlineImageRenderSettings) {
        if settings != self.inline_image_render_settings {
            self.clear_inline_image_previews();
            self.inline_image_render_settings = settings;
        }

        let Some(rx) = self.inline_image_rx.as_mut() else {
            return;
        };
        let now = Instant::now();
        let mut completed = Vec::new();
        while let Ok(result) = rx.try_recv() {
            completed.push(result);
        }

        let mut received_ids = Vec::new();
        for (msg_id, completed_settings, result) in completed {
            if completed_settings != settings {
                continue;
            }
            self.inline_image_requested.remove(&msg_id);
            match result {
                Ok(lines) => {
                    self.inline_image_failures.remove(&msg_id);
                    self.inline_image_cache.insert(msg_id, lines);
                    self.context_epoch += 1;
                }
                Err(error) => {
                    let attempts = self
                        .inline_image_failures
                        .get(&msg_id)
                        .map(|failure| failure.attempts)
                        .unwrap_or(0)
                        .saturating_add(1);
                    let next_retry_at = now + inline_image_retry_delay(attempts);
                    self.inline_image_failures.insert(
                        msg_id,
                        InlineImageFailure {
                            attempts,
                            next_retry_at,
                        },
                    );
                    tracing::trace!(
                        message_id = %msg_id,
                        attempts,
                        error,
                        "inline image render failed"
                    );
                }
            }
            received_ids.push(msg_id);
        }
        for msg_id in received_ids {
            self.track_inline_image_id(msg_id);
        }

        // Request missing images for currently visible room
        let Some(room_id) = self.visible_room_id else {
            return;
        };
        let Some(tx) = self.inline_image_tx.clone() else {
            return;
        };

        let messages = self.messages_for_room(room_id);
        if messages.is_empty() {
            return;
        }

        let requests = inline_image_request_candidates(
            messages,
            &self.inline_image_requested,
            &self.inline_image_cache,
            &self.inline_image_failures,
            now,
        );

        for (msg_id, url) in requests {
            self.inline_image_requested.insert(msg_id);
            self.track_inline_image_id(msg_id);
            if !url.is_empty() {
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    let result = crate::app::files::inline_image::fetch_and_render_image(
                        url,
                        INLINE_IMAGE_MAX_WIDTH,
                        INLINE_IMAGE_MAX_ROWS,
                        settings,
                    )
                    .await
                    .map_err(|e| e.to_string());
                    let _ = tx_clone.send((msg_id, settings, result));
                });
            }
        }
    }

    /// Returns true when any fetch completed; the image modal renders from
    /// this cache, so a completion must count as a render-visible change.
    pub(crate) fn poll_terminal_images(&mut self) -> bool {
        let Some(rx) = self.terminal_image_rx.as_mut() else {
            return false;
        };

        let mut completed = Vec::new();
        while let Ok(result) = rx.try_recv() {
            completed.push(result);
        }
        let any_completed = !completed.is_empty();

        for (msg_id, result) in completed {
            self.terminal_image_requested.remove(&msg_id);
            match result {
                Ok(image) => {
                    self.terminal_image_failed.remove(&msg_id);
                    self.terminal_image_cache.insert(msg_id, image);
                }
                Err(error) => {
                    self.terminal_image_failed.insert(msg_id);
                    tracing::trace!(
                        message_id = %msg_id,
                        error,
                        "terminal image render failed"
                    );
                }
            }
            self.track_inline_image_id(msg_id);
        }
        any_completed
    }

    pub(crate) fn request_image_modal_terminal_image(
        &mut self,
        protocol: Option<crate::app::files::terminal_image::TerminalImageProtocol>,
    ) {
        let Some(protocol) = protocol else {
            return;
        };
        let Some(modal) = self.image_modal.as_ref() else {
            return;
        };
        let msg_id = modal.message_id;
        // Sixel has no terminal-side scaling, so the encode must fit the
        // modal's image area or the payload is dropped at draw time. The
        // capacity is reported back from the previous frame's draw; until it
        // arrives (one frame after the modal opens), hold off fetching.
        let sixel = protocol == crate::app::files::terminal_image::TerminalImageProtocol::Sixel;
        let (max_cols, max_rows) = if sixel {
            let Some((cap_cols, cap_rows)) = self.image_modal_capacity else {
                return;
            };
            (
                TERMINAL_IMAGE_MAX_COLS.min(u32::from(cap_cols)),
                TERMINAL_IMAGE_MAX_ROWS.min(u32::from(cap_rows)),
            )
        } else {
            (TERMINAL_IMAGE_MAX_COLS, TERMINAL_IMAGE_MAX_ROWS)
        };
        let cached_fits = self.terminal_image_cache.get(&msg_id).is_some_and(|image| {
            image.supports_protocol(protocol)
                && (!sixel
                    || (u32::from(image.display_cols) <= max_cols
                        && u32::from(image.display_rows) <= max_rows))
        });
        if cached_fits
            || self.terminal_image_requested.contains(&msg_id)
            || self.terminal_image_failed.contains(&msg_id)
        {
            return;
        }
        self.terminal_image_cache.remove(&msg_id);
        let Some(tx) = self.terminal_image_tx.clone() else {
            return;
        };

        let url = modal.url.clone();
        self.terminal_image_requested.insert(msg_id);
        self.track_inline_image_id(msg_id);
        tokio::spawn(async move {
            let result = crate::app::files::terminal_image::fetch_terminal_image(
                url, max_cols, max_rows, protocol,
            )
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send((msg_id, result));
        });
    }

    pub(crate) fn terminal_image_for_message(
        &self,
        message_id: Uuid,
    ) -> Option<&crate::app::files::terminal_image::TerminalImageData> {
        self.terminal_image_cache.get(&message_id)
    }

    pub(crate) fn clear_inline_image_previews(&mut self) {
        if !self.inline_image_cache.is_empty() {
            self.context_epoch += 1;
        }
        self.inline_image_cache.clear();
        self.inline_image_requested.clear();
        self.inline_image_failures.clear();
    }

    fn track_inline_image_id(&mut self, msg_id: Uuid) {
        if !self.inline_image_cache.contains_key(&msg_id)
            && !self.inline_image_requested.contains(&msg_id)
            && !self.inline_image_failures.contains_key(&msg_id)
            && !self.terminal_image_cache.contains_key(&msg_id)
            && !self.terminal_image_requested.contains(&msg_id)
            && !self.terminal_image_failed.contains(&msg_id)
        {
            return;
        }
        if !self.inline_image_tracked_order.contains(&msg_id) {
            self.inline_image_tracked_order.push_back(msg_id);
        }
        while self.inline_image_tracked_order.len() > INLINE_IMAGE_TRACKED_LIMIT {
            if let Some(old_id) = self.inline_image_tracked_order.pop_front() {
                self.inline_image_requested.remove(&old_id);
                if self.inline_image_cache.remove(&old_id).is_some() {
                    self.context_epoch += 1;
                }
                self.inline_image_failures.remove(&old_id);
                self.terminal_image_requested.remove(&old_id);
                self.terminal_image_cache.remove(&old_id);
                self.terminal_image_failed.remove(&old_id);
            }
        }
    }

    pub fn tick(&mut self) -> ChatTick {
        self.sync_refresh_room_id();
        // Peek every event source before draining: anything queued may change
        // render-visible chat state (messages, unread badges, tab lists), so
        // it must count as changed. Over-reporting here only costs a frame;
        // a wrong "clean" freezes the UI. Snapshots are the exception: they
        // arrive on a fixed cadence whether or not anything changed, so
        // drain_snapshot reports real change itself instead of being peeked.
        let changed = self.username_rx.has_changed().unwrap_or(false)
            || !self.targeted_event_rx.is_empty()
            || !self.event_rx.is_empty()
            || !self.moderation_event_rx.is_empty()
            || !self.translation_rx.is_empty()
            || !self.summary_rx.is_empty();
        self.drain_username_directory();
        let changed = self.drain_snapshot() || changed;
        let banner = self.drain_events();
        let translation_banner = self.drain_translation_events();
        let (summary_banner, summary_overlay_promoted) = self.drain_summary_events();
        let moderation_banner = self.drain_moderation_events();
        let feeds_tick = self.feeds.tick();
        let news_tick = self.news.tick();
        let notif_tick = self.notifications.tick();
        let showcase_tick = self.showcase.tick();
        let work_tick = self.work.tick();
        let cyberspace_tick = self.cyberspace.tick();
        // The pinned list can change under the rail cursor (another session
        // of the same account pinning, unpinning, or unlinking), so the
        // selected index is re-derived from the open room's slug instead of
        // trusted. A room the rail can no longer name gets left, dropping
        // its stream and heartbeat, and the user lands back on the pane.
        if self.cyberspace_room_selected.is_some() {
            let derived = self.cyberspace.open_circ_slug().and_then(|slug| {
                self.cyberspace
                    .pinned_rooms()
                    .iter()
                    .position(|room| room == slug)
            });
            match derived {
                Some(index) => self.cyberspace_room_selected = Some(index),
                None => {
                    self.cyberspace.leave_room();
                    self.cyberspace_room_selected = None;
                    self.cyberspace_selected = true;
                }
            }
        }
        // A conversation the user started by name is one they asked to write
        // in, so it opens rather than only appearing in the rail. Pulling
        // them to Home matches every other `/cs` command that moves them.
        if let Some(id) = self.cyberspace.take_started_cmail()
            && let Some(index) = self
                .cyberspace
                .pinned_cmail()
                .iter()
                .position(|pin| pin.id == id)
        {
            self.select_cyberspace_mail(index);
            self.pending_chat_screen_switch = true;
        }
        // Same reconcile for the pinned conversations.
        if self.cyberspace_mail_selected.is_some() {
            let derived = self.cyberspace.open_cmail_id().and_then(|id| {
                self.cyberspace
                    .pinned_cmail()
                    .iter()
                    .position(|thread| thread.id == id)
            });
            match derived {
                Some(index) => self.cyberspace_mail_selected = Some(index),
                None => {
                    self.cyberspace.leave_room();
                    self.cyberspace_mail_selected = None;
                    self.cyberspace_selected = true;
                }
            }
        }
        // Unlinking in one session broadcasts to the others. The rail entry
        // and the navigation order both go with the link, so a session left
        // sitting in the pane would be on a slot neither of them has.
        if (self.cyberspace_selected || self.cyberspace_notifications_selected)
            && self.cyberspace.is_unlinked()
        {
            self.leave_selected_synthetic_entry();
        }
        self.flush_pending_read_cursors_if_due();
        let banner = moderation_banner
            .or(banner)
            .or(translation_banner)
            .or(summary_banner)
            .or(feeds_tick.banner)
            .or(news_tick.banner)
            .or(notif_tick.banner)
            .or(showcase_tick.banner)
            .or(work_tick.banner)
            .or(cyberspace_tick.banner);
        ChatTick {
            banner,
            changed: changed
                || summary_overlay_promoted
                || feeds_tick.changed
                || news_tick.changed
                || notif_tick.changed
                || showcase_tick.changed
                || work_tick.changed
                || cyberspace_tick.changed,
        }
    }

    /// Every Home synthetic entry is exclusive with the others, so selecting
    /// one clears the rest in one place. A new entry that forgets a line here
    /// would leave two panes claiming the center at once.
    fn clear_synthetic_selection(&mut self) {
        // Whatever the user is moving to, they are no longer in a cyberspace
        // chat room; dropping it stops its stream, its heartbeat, and
        // announces them out of the room on their side.
        self.cyberspace.leave_room();
        self.room_jump_active = false;
        self.feeds_selected = false;
        self.news_selected = false;
        self.cyberspace_selected = false;
        self.cyberspace_notifications_selected = false;
        self.cyberspace_room_selected = None;
        self.cyberspace_mail_selected = None;
        self.notifications_selected = false;
        self.discover_selected = false;
        self.showcase_selected = false;
        self.work_selected = false;
        self.selected_message_id = None;
        self.selection_scroll.reset();
        self.highlighted_message_id = None;
    }

    pub fn select_feeds(&mut self) {
        self.clear_synthetic_selection();
        self.feeds_selected = true;
        self.feeds.list();
        self.feeds.mark_read();
    }

    pub fn select_cyberspace(&mut self) {
        // Only an actual entry loads the feed. Re-selecting the slot you are
        // already on (clicking the row, cycling the rail back around) must not
        // spend another authenticated call on a third-party API.
        let entering = !self.cyberspace_selected;
        self.clear_synthetic_selection();
        self.cyberspace_selected = true;
        if entering {
            self.cyberspace.opened();
        }
    }

    /// Their notification list, the row beside `feeds`. Same rule: only an
    /// actual entry loads, since a load is also a read on their side.
    pub fn select_cyberspace_notifications(&mut self) {
        let entering = !self.cyberspace_notifications_selected;
        self.clear_synthetic_selection();
        self.cyberspace_notifications_selected = true;
        if entering {
            self.cyberspace.opened_notifications();
        }
    }

    /// Leaving the Home surface entirely (a screen switch), not just moving
    /// within the rail. The open room's session drops with the selection:
    /// its stream and presence heartbeat must not outlive the user's
    /// presence on the surface. The rail lands back on the cyberspace slot,
    /// same as Esc, but without `select_cyberspace`'s feed load, since the
    /// user is on their way out, not in.
    pub fn close_cyberspace_room(&mut self) {
        if self.cyberspace_room_selected.is_none()
            && self.cyberspace_mail_selected.is_none()
            && self.cyberspace.open_room_name().is_none()
        {
            return;
        }
        self.cyberspace.leave_room();
        self.cyberspace_room_selected = None;
        self.cyberspace_mail_selected = None;
        self.cyberspace_selected = true;
    }

    /// Select a pinned C-Mail conversation by its position in the pinned
    /// list. Same contract as a room: entering is what opens its stream.
    pub fn select_cyberspace_mail(&mut self, index: usize) {
        let Some(thread) = self.cyberspace.pinned_cmail().get(index).cloned() else {
            return;
        };
        if self.cyberspace_mail_selected == Some(index)
            && self.cyberspace.open_cmail_id() == Some(thread.id.as_str())
        {
            return;
        }
        self.remember_cyberspace_row();
        self.clear_synthetic_selection();
        self.cyberspace_mail_selected = Some(index);
        self.cyberspace.enter_cmail(thread);
    }

    /// Select a pinned chat room by its position in the pinned list. Entering
    /// the room is what opens its stream; a room nobody has selected holds
    /// nothing open.
    pub fn select_cyberspace_room(&mut self, index: usize) {
        let Some(slug) = self.cyberspace.pinned_rooms().get(index).cloned() else {
            return;
        };
        // Re-selecting the room you are already in (clicking its row, cycling
        // the rail around) must not tear the stream down and reconnect.
        if self.cyberspace_room_selected == Some(index)
            && self.cyberspace.open_circ_slug() == Some(slug.as_str())
        {
            return;
        }
        self.remember_cyberspace_row();
        self.clear_synthetic_selection();
        self.cyberspace_room_selected = Some(index);
        self.cyberspace.enter_room(slug);
    }

    /// Note which cyberspace row the user is standing on before a room takes
    /// the selection, so `select_cyberspace_return_row` can put them back on
    /// it. A hop straight from one room or conversation to another keeps the
    /// recorded origin: the user never stood on a pane row in between.
    fn remember_cyberspace_row(&mut self) {
        if self.cyberspace_room_selected.is_some() || self.cyberspace_mail_selected.is_some() {
            return;
        }
        self.cyberspace_return_row = match self.cyberspace_notifications_selected {
            true => CyberspaceRow::Notifications,
            false => CyberspaceRow::Feeds,
        };
    }

    /// Leaving a room or conversation: back to the row it was entered from.
    pub fn select_cyberspace_return_row(&mut self) {
        match self.cyberspace_return_row {
            CyberspaceRow::Feeds => self.select_cyberspace(),
            CyberspaceRow::Notifications => self.select_cyberspace_notifications(),
        }
    }

    pub fn select_news(&mut self) {
        self.clear_synthetic_selection();
        self.news_selected = true;
        self.news.list_articles();
        self.news.mark_read();
    }

    pub fn deselect_news(&mut self) {
        self.news_selected = false;
    }

    pub fn select_notifications(&mut self) {
        self.clear_synthetic_selection();
        self.notifications_selected = true;
        self.notifications.list();
        self.notifications.mark_read();
    }

    pub fn select_discover(&mut self) {
        self.clear_synthetic_selection();
        self.discover_selected = true;
        self.discover.start_loading();
        self.service.list_discover_rooms_task(self.user_id);
    }

    pub fn select_showcase(&mut self) {
        self.clear_synthetic_selection();
        self.showcase_selected = true;
        self.showcase.list();
        self.showcase.mark_read();
    }

    pub fn select_work(&mut self) {
        self.clear_synthetic_selection();
        self.work_selected = true;
        self.work.list();
        self.work.mark_read();
    }

    pub fn join_selected_discover_room(&mut self) -> Option<Banner> {
        let item = self.discover.selected_item()?.clone();
        self.service
            .join_public_room_task(self.user_id, item.room_id, item.slug.clone());
        Some(Banner::success(&format!("Joining #{}...", item.slug)))
    }

    pub fn cursor_visible(&self) -> bool {
        self.composing
    }

    pub fn is_autocomplete_active(&self) -> bool {
        self.mention_ac.active
    }

    pub(crate) fn username_mention_matches(&self, query_lower: &str) -> Vec<MentionMatch> {
        let active_users = self.active_users.as_ref();
        rank_mention_matches(self.all_usernames.as_ref(), query_lower, || {
            online_username_set(active_users)
        })
    }

    pub(crate) fn room_name_matches(&self, query_lower: &str) -> Vec<MentionMatch> {
        rank_room_name_matches(self.rooms.iter().map(|(room, _)| room), query_lower)
    }

    pub fn update_autocomplete(&mut self) {
        // Scan backward from end of composer to find a trigger in the current token.
        let text = self.composer.lines().join("\n");
        let bytes = text.as_bytes();
        let mut trigger = None;
        for i in (0..bytes.len()).rev() {
            // Valid if at start or preceded by whitespace (space or newline)
            let is_mention =
                matches!(bytes[i], b'@') && (i == 0 || bytes[i - 1].is_ascii_whitespace());
            let is_command = matches!(bytes[i], b'/') && i == 0;
            if is_mention || is_command {
                trigger = Some((i, bytes[i]));
                break;
            }

            // Stop scanning if we hit whitespace (no @ in this word)
            if bytes[i].is_ascii_whitespace() {
                break;
            }
        }

        let Some((offset, trigger_byte)) = trigger else {
            self.mention_ac.active = false;
            return;
        };

        let query = &text[offset + 1..];
        let query_lower = query.to_ascii_lowercase();
        let matches = if trigger_byte == b'@' {
            self.username_mention_matches(&query_lower)
        } else {
            let room = self.composer_room_id.and_then(|id| self.room_by_id(id));
            rank_command_matches(&query_lower, room)
        };

        if matches.is_empty() {
            self.mention_ac.active = false;
            return;
        }

        self.mention_ac.active = true;
        self.mention_ac.query = query.to_string();
        self.mention_ac.trigger_offset = offset;
        self.mention_ac.selected = self
            .mention_ac
            .selected
            .min(matches.len().saturating_sub(1));
        self.mention_ac.matches = matches;
    }

    pub fn ac_move_selection(&mut self, delta: isize) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let len = self.mention_ac.matches.len() as isize;
        let cur = self.mention_ac.selected as isize;
        self.mention_ac.selected = (cur + delta).clamp(0, len - 1) as usize;
    }

    pub fn ac_confirm(&mut self) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let selected = &self.mention_ac.matches[self.mention_ac.selected];
        let text = self.composer.lines().join("\n");
        let next = format!(
            "{}{}{} ",
            &text[..self.mention_ac.trigger_offset],
            selected.prefix,
            selected.name
        );
        let composing = self.composing;
        self.composer = new_chat_textarea();
        self.composer.insert_str(next);
        composer::set_themed_textarea_cursor_visible(&mut self.composer, composing);
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn ac_dismiss(&mut self) {
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn lounge_messages(&self) -> &[ChatMessage] {
        let Some(lounge_id) = self.lounge_room_id else {
            return &[];
        };
        self.messages_for_room(lounge_id)
    }

    /// Messages for any joined room — used by the dashboard chat card when
    /// the user pins favorites and cycles between them.
    pub fn messages_for_room(&self, room_id: Uuid) -> &[ChatMessage] {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(_, msgs)| msgs.as_slice())
            .unwrap_or(&[])
    }

    /// Recent #lounge system-feed lines for the activity ticker row,
    /// newest first.
    pub fn activity_ticker(&self) -> &[ActivityTickerEntry] {
        &self.activity_ticker
    }

    pub fn usernames(&self) -> &HashMap<Uuid, String> {
        &self.usernames
    }

    pub fn countries(&self) -> &HashMap<Uuid, String> {
        &self.countries
    }

    pub fn bonsai_glyphs(&self) -> &HashMap<Uuid, String> {
        &self.bonsai_glyphs
    }

    pub fn chat_badges(&self) -> &HashMap<Uuid, String> {
        &self.chat_badges
    }

    pub fn profile_award_badges(&self) -> &HashMap<Uuid, String> {
        &self.profile_award_badges
    }

    fn set_bonsai_glyph(&mut self, user_id: Uuid, glyph: Option<&str>) {
        let changed = set_context_value(&mut self.bonsai_glyphs, user_id, glyph);
        if changed {
            self.context_epoch += 1;
        }
    }

    pub fn set_chat_badge(&mut self, user_id: Uuid, badge: Option<&str>) {
        let changed = set_context_value(&mut self.chat_badges, user_id, badge);
        if changed {
            self.context_epoch += 1;
        }
    }

    fn set_profile_award_badge(&mut self, user_id: Uuid, badge: Option<&str>) {
        let changed = set_context_value(&mut self.profile_award_badges, user_id, badge);
        if changed {
            self.context_epoch += 1;
        }
    }

    /// Insert `username` for the author, bumping the context epoch only when
    /// the stored value actually changes.
    fn note_username(&mut self, user_id: Uuid, username: String) {
        match self.usernames.get(&user_id) {
            Some(existing) if *existing == username => {}
            _ => {
                self.usernames.insert(user_id, username);
                self.context_epoch += 1;
            }
        }
    }

    /// Merge a username map from a service payload, bumping the context
    /// epoch only on real changes.
    fn extend_usernames(&mut self, usernames: HashMap<Uuid, String>) {
        if extend_changed(&mut self.usernames, usernames) {
            self.context_epoch += 1;
        }
    }

    /// Current message-store version for a room; part of the row cache key.
    pub fn room_version(&self, room_id: Uuid) -> u64 {
        self.room_versions.get(&room_id).copied().unwrap_or(0)
    }

    /// All per-room message-store versions, for surfaces that resolve the
    /// rendered room inside the draw call.
    pub fn room_versions(&self) -> &HashMap<Uuid, u64> {
        &self.room_versions
    }

    /// Author-context epoch; part of the row cache key.
    pub fn context_epoch(&self) -> u64 {
        self.context_epoch
    }

    fn bump_room_version(&mut self, room_id: Uuid) {
        *self.room_versions.entry(room_id).or_insert(0) += 1;
    }

    pub fn friend_user_ids(&self) -> &HashSet<Uuid> {
        &self.friend_user_ids
    }

    pub fn ignored_user_ids(&self) -> &HashSet<Uuid> {
        &self.ignored_user_ids
    }

    pub fn active_friend_names(&self) -> Vec<String> {
        let Some(active_users) = &self.active_users else {
            return Vec::new();
        };
        let active_users = active_users.lock_recover();
        let mut friends: Vec<&ActiveUser> = self
            .friend_user_ids
            .iter()
            .filter_map(|id| active_users.get(id))
            .collect();
        friends.sort_by(|left, right| {
            right.last_login_at.cmp(&left.last_login_at).then_with(|| {
                left.username
                    .bytes()
                    .map(|b| b.to_ascii_lowercase())
                    .cmp(right.username.bytes().map(|b| b.to_ascii_lowercase()))
            })
        });
        friends
            .into_iter()
            .map(|user| user.username.clone())
            .collect()
    }

    pub fn note_friend_join(&mut self, user_id: Uuid, username: &str) -> Option<Banner> {
        if user_id == self.user_id || !self.friend_user_ids.contains(&user_id) {
            return None;
        }
        self.note_username(user_id, username.to_string());
        self.notifier.push(Notification::friend_online(username));
        Some(Banner::success(&format!("Friend online: @{username}")))
    }

    /// A friend's stream reported its first media (the `WentLive` edge, not
    /// `/golive` time), so the banner never points at a black screen.
    /// `title` is the raw `/golive` title, already clamped at the composer
    /// boundary; unlike a #lounge body it may keep its `@`, since nothing
    /// here runs the mention pipeline.
    pub fn note_friend_went_live(
        &mut self,
        user_id: Uuid,
        username: &str,
        title: Option<&str>,
    ) -> Option<Banner> {
        if user_id == self.user_id || !self.friend_user_ids.contains(&user_id) {
            return None;
        }
        self.note_username(user_id, username.to_string());
        self.notifier
            .push(Notification::friend_live(username, title));
        Some(Banner::success(&format!(
            "@{username} is live. /watch @{username} to open it."
        )))
    }

    pub fn message_reactions(&self) -> &HashMap<Uuid, Vec<ChatMessageReactionSummary>> {
        &self.message_reactions
    }

    pub fn message_gilds(&self) -> &HashMap<Uuid, ChatMessageGildSummary> {
        &self.message_gilds
    }

    /// Returns true when applying the snapshot changed anything
    /// render-visible. Snapshots arrive on a fixed cadence whether or not
    /// anything changed, so every write below detects real change before
    /// reporting; an unchanged snapshot must not invalidate caches or pay a
    /// frame.
    /// The friend and ignore lists have two writers: the snapshot, a database
    /// read taken at some instant, and the list events, writes confirmed after
    /// they commit. A read that began before the last applied write predates
    /// it and must not win.
    fn snapshot_lists_are_current(&self, read_started_at: Option<Instant>) -> bool {
        match (read_started_at, self.friend_ignore_applied_at) {
            (_, None) => true,
            (Some(read_started_at), Some(applied_at)) => read_started_at > applied_at,
            (None, Some(_)) => false,
        }
    }

    fn drain_snapshot(&mut self) -> bool {
        if !self.snapshot_rx.has_changed().unwrap_or(false) {
            return false;
        }

        let snapshot = self.snapshot_rx.borrow_and_update().clone();
        if snapshot.user_id != Some(self.user_id) {
            return false;
        }

        let mut changed = false;
        let mut context_changed = false;
        let refreshed_author_ids = snapshot
            .chat_rooms
            .iter()
            .flat_map(|(_, messages)| messages.iter().map(|message| message.user_id))
            .chain(snapshot.usernames.keys().copied())
            .collect::<HashSet<_>>();
        for user_id in &refreshed_author_ids {
            if !snapshot.bonsai_glyphs.contains_key(user_id) {
                context_changed |= self.bonsai_glyphs.remove(user_id).is_some();
            }
            if !snapshot.chat_badges.contains_key(user_id) {
                context_changed |= self.chat_badges.remove(user_id).is_some();
            }
            if !snapshot.profile_award_badges.contains_key(user_id) {
                context_changed |= self.profile_award_badges.remove(user_id).is_some();
            }
        }

        context_changed |= extend_changed(&mut self.usernames, snapshot.usernames);
        if self.countries != snapshot.countries {
            self.countries = snapshot.countries;
            context_changed = true;
        }
        if self.snapshot_lists_are_current(snapshot.read_started_at) {
            let ignored_user_ids: HashSet<Uuid> = snapshot.ignored_user_ids.into_iter().collect();
            if self.ignored_user_ids != ignored_user_ids {
                self.ignored_user_ids = ignored_user_ids;
                context_changed = true;
            }
            let friend_user_ids: HashSet<Uuid> = snapshot.friend_user_ids.into_iter().collect();
            if self.friend_user_ids != friend_user_ids {
                self.friend_user_ids = friend_user_ids;
                context_changed = true;
            }
        }
        if self.room_owner_ids != snapshot.room_owner_ids {
            self.room_owner_ids = snapshot.room_owner_ids;
            changed = true;
        }
        if self.voice_channels_by_room_id != snapshot.voice_channels_by_room_id {
            self.voice_channels_by_room_id = snapshot.voice_channels_by_room_id;
            changed = true;
        }
        for (_, messages) in &snapshot.chat_rooms {
            changed |= self.note_activity_ticker_from(messages);
        }
        let previous_room_signatures: HashMap<Uuid, Vec<(Uuid, DateTime<Utc>)>> = self
            .rooms
            .iter()
            .map(|(room, messages)| {
                (
                    room.id,
                    messages.iter().map(|m| (m.id, m.updated)).collect(),
                )
            })
            .collect();
        // Room metadata and list order matter to the room rail even when no
        // message signature moved: compare the room structs themselves.
        let previous_rooms_meta: Vec<ChatRoom> =
            self.rooms.iter().map(|(room, _)| room.clone()).collect();
        self.rooms = self.merge_rooms(snapshot.chat_rooms);
        changed |= self.rooms.len() != previous_rooms_meta.len()
            || self
                .rooms
                .iter()
                .zip(&previous_rooms_meta)
                .any(|((room, _), previous)| room != previous);
        let changed_room_ids: Vec<Uuid> = self
            .rooms
            .iter()
            .filter(|(room, messages)| {
                let signature: Vec<(Uuid, DateTime<Utc>)> =
                    messages.iter().map(|m| (m.id, m.updated)).collect();
                previous_room_signatures.get(&room.id) != Some(&signature)
            })
            .map(|(room, _)| room.id)
            .collect();
        changed |= !changed_room_ids.is_empty();
        for room_id in changed_room_ids {
            self.bump_room_version(room_id);
        }
        if self.lounge_room_id != snapshot.lounge_room_id {
            self.lounge_room_id = snapshot.lounge_room_id;
            changed = true;
        }
        let merged_unread_counts = self.merge_unread_counts(snapshot.unread_counts);
        if self.unread_counts != merged_unread_counts {
            self.unread_counts = merged_unread_counts;
            changed = true;
        }
        let merged_room_last_message_at =
            self.merge_room_last_message_at(snapshot.room_last_message_at);
        if self.room_last_message_at != merged_room_last_message_at {
            self.room_last_message_at = merged_room_last_message_at;
            changed = true;
        }
        if self.active_polls != snapshot.active_polls {
            self.active_polls = snapshot.active_polls;
            changed = true;
        }
        context_changed |= extend_changed(&mut self.bonsai_glyphs, snapshot.bonsai_glyphs);
        context_changed |= extend_changed(&mut self.chat_badges, snapshot.chat_badges);
        context_changed |= extend_changed(
            &mut self.profile_award_badges,
            snapshot.profile_award_badges,
        );
        let merged_reactions = self.merge_message_reactions(snapshot.message_reactions);
        if self.message_reactions != merged_reactions {
            self.message_reactions = merged_reactions;
            context_changed = true;
        }
        if context_changed {
            self.context_epoch += 1;
        }
        let selected_before = self.selected_room_id;
        self.sync_selection();
        changed |= self.selected_room_id != selected_before;
        changed || context_changed
    }

    fn drain_username_directory(&mut self) {
        if !self.username_rx.has_changed().unwrap_or(false) {
            return;
        }
        self.all_usernames = self.username_rx.borrow_and_update().clone();
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            // Point-to-point events first (they cannot lag), then the global
            // broadcast; both feed the same match below.
            let event = match self.targeted_event_rx.try_recv() {
                Ok(event) => event,
                Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                    match self.event_rx.try_recv() {
                        Ok(event) => event,
                        Err(TryRecvError::Lagged(_)) => {
                            if let Some(room_id) = self.visible_room_id {
                                self.request_room_tail(room_id);
                            }
                            continue;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                    }
                }
            };
            match event {
                ChatEvent::MessageCreated {
                    message,
                    target_user_ids,
                    author_username,
                    author_bonsai_glyph,
                    author_chat_badge,
                    author_profile_award_badges,
                } => {
                    let is_targeted = target_user_ids.is_some();
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    if is_targeted
                        && !self
                            .rooms
                            .iter()
                            .any(|(room, _)| room.id == message.room_id)
                    {
                        self.request_list();
                    }
                    // Desktop notification queueing. target_user_ids is Some for
                    // DM/private rooms, None for public rooms. Don't notify on
                    // messages we authored ourselves, or on ignored users
                    // (including DMs, so ignore silences DMs too).
                    let ignored_author = self.message_is_ignored(&message);
                    if message.user_id != self.user_id && !ignored_author {
                        let nickname = self
                            .usernames
                            .get(&message.user_id)
                            .cloned()
                            .unwrap_or_else(|| "someone".to_string());
                        let preview: String =
                            message.body.replace('\n', " ").chars().take(80).collect();

                        if is_targeted {
                            self.notifier.push(Notification::dm(&nickname, preview));
                        } else if let Some(me) = self.usernames.get(&self.user_id) {
                            let me_lc = me.to_ascii_lowercase();
                            if crate::app::common::mentions::extract_mentions(&message.body)
                                .iter()
                                .any(|m| m == &me_lc)
                            {
                                self.notifier
                                    .push(Notification::mention(&nickname, preview));
                            }
                        }
                    }
                    if let Some(username) = author_username {
                        self.note_username(message.user_id, username);
                    }
                    self.set_bonsai_glyph(message.user_id, author_bonsai_glyph.as_deref());
                    self.set_chat_badge(message.user_id, author_chat_badge.as_deref());
                    self.set_profile_award_badge(
                        message.user_id,
                        author_profile_award_badges.as_deref(),
                    );
                    // Auto-translate applies to live messages in the room on
                    // screen only; history stays on-demand (`t`). Decided
                    // before the push (which consumes the message), fired
                    // after it, and only if the message actually landed as a
                    // chat row (not ignored/system/duplicate).
                    let translate_candidate = (self.auto_translate
                        && message.user_id != self.user_id
                        && Some(message.room_id) == self.visible_room_id
                        && !self.translations.contains_key(&message.id)
                        && needs_translation(&message.body, self.translate_to))
                    .then(|| (message.id, message.room_id, message.body.clone()));
                    self.push_message(message);
                    if let Some((message_id, room_id, body)) = translate_candidate
                        && self.rooms.iter().any(|(room, messages)| {
                            room.id == room_id && messages.iter().any(|m| m.id == message_id)
                        })
                    {
                        // No Pending marker here: the "translating…"
                        // placeholder is manual-only (`t`). An auto-fired
                        // request renders nothing until a real translation
                        // lands, so same-language verdicts (most messages,
                        // now that English goes to the model) never flash a
                        // line that immediately vanishes. Duplicate requests
                        // are the service's single-flight problem, and a `t`
                        // pressed mid-flight just joins the same call.
                        self.translation_cache_checked.insert(message_id);
                        self.translation_service.request(
                            message_id,
                            room_id,
                            body,
                            self.translate_to,
                        );
                    }
                }
                ChatEvent::SendSucceeded {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::success("Message sent"));
                }
                ChatEvent::DeltaSynced {
                    user_id,
                    room_id,
                    messages,
                } if self.user_id == user_id => {
                    for message in messages {
                        if message.room_id == room_id {
                            self.push_message(message);
                        }
                    }
                }
                ChatEvent::RoomTailLoaded {
                    user_id,
                    room_id,
                    messages,
                    message_reactions,
                    message_gilds,
                    usernames,
                    bonsai_glyphs,
                    chat_badges,
                    profile_award_badges,
                } if self.user_id == user_id => {
                    self.loading_tail_rooms.remove(&room_id);
                    self.extend_usernames(usernames);
                    let mut context_changed = false;
                    for message in &messages {
                        if !bonsai_glyphs.contains_key(&message.user_id) {
                            context_changed |=
                                self.bonsai_glyphs.remove(&message.user_id).is_some();
                        }
                        if !chat_badges.contains_key(&message.user_id) {
                            context_changed |= self.chat_badges.remove(&message.user_id).is_some();
                        }
                        if !profile_award_badges.contains_key(&message.user_id) {
                            context_changed |=
                                self.profile_award_badges.remove(&message.user_id).is_some();
                        }
                    }
                    context_changed |= extend_changed(&mut self.bonsai_glyphs, bonsai_glyphs);
                    context_changed |= extend_changed(&mut self.chat_badges, chat_badges);
                    context_changed |=
                        extend_changed(&mut self.profile_award_badges, profile_award_badges);
                    if context_changed {
                        self.context_epoch += 1;
                    }
                    self.merge_room_tail(room_id, messages);
                    let mut reactions_changed = false;
                    for (message_id, reactions) in message_reactions {
                        match self.message_reactions.get(&message_id) {
                            Some(existing) if *existing == reactions => {}
                            _ => {
                                self.message_reactions.insert(message_id, reactions);
                                reactions_changed = true;
                            }
                        }
                    }
                    let mut gilds_changed = false;
                    for (message_id, gild) in message_gilds {
                        match self.message_gilds.get(&message_id) {
                            Some(existing) if *existing == gild => {}
                            _ => {
                                self.message_gilds.insert(message_id, gild);
                                gilds_changed = true;
                            }
                        }
                    }
                    if reactions_changed || gilds_changed {
                        self.bump_room_version(room_id);
                    }
                    if self.visible_room_id == Some(room_id) {
                        self.mark_room_read(room_id);
                        self.request_cached_translations_for_visible_room();
                    }
                    if let Some((jump_room_id, message_id)) = self.pending_search_jump
                        && jump_room_id == room_id
                    {
                        self.pending_search_jump = None;
                        if self.message_is_loaded_in_room(room_id, message_id) {
                            self.select_message_by_id_in_room(room_id, message_id);
                        } else {
                            // Further back than the tail reaches. The room is
                            // already on screen underneath, so the modal opens
                            // over it and closing lands the user in the right
                            // room rather than nowhere.
                            self.open_history_at_message(room_id, message_id);
                        }
                    }
                }
                ChatEvent::RoomTailLoadFailed { user_id, room_id } if self.user_id == user_id => {
                    self.loading_tail_rooms.remove(&room_id);
                    if self
                        .pending_search_jump
                        .is_some_and(|(id, _)| id == room_id)
                    {
                        self.pending_search_jump = None;
                    }
                }
                ChatEvent::SendFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::DmOpened { user_id, room_id } if self.user_id == user_id => {
                    // Every synthetic entry drops, the open cyberspace room
                    // included: the user is being moved into a real room.
                    self.clear_synthetic_selection();
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_chat_screen_switch = true;
                    banner = Some(Banner::success("DM opened"));
                }
                ChatEvent::DmFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::OpenProfileResolved {
                    user_id,
                    target_user_id,
                    target_username,
                } if self.user_id == user_id => {
                    self.requested_open_profile = Some((target_user_id, target_username));
                }
                ChatEvent::OpenProfileFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&sentence_case(&message)));
                }
                ChatEvent::OpenSheetResolved {
                    user_id,
                    room_id,
                    target_user_id,
                    target_username,
                    name,
                    body,
                } if self.user_id == user_id => {
                    self.requested_open_sheet = Some(SheetOpenRequest {
                        room_id,
                        target_username,
                        name,
                        body,
                        editable: target_user_id == self.user_id,
                    });
                }
                ChatEvent::SheetError { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&sentence_case(&message)));
                }
                ChatEvent::RoomJoined {
                    user_id,
                    room_id,
                    slug,
                } if self.user_id == user_id => {
                    // Every synthetic entry drops, the open cyberspace room
                    // included: the user is being moved into a real room.
                    self.clear_synthetic_selection();
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_chat_screen_switch = true;
                    banner = Some(Banner::success(&format!("Joined #{slug}")));
                }
                ChatEvent::GameRoomJoined { user_id, room_id } if self.user_id == user_id => {
                    self.request_list();
                    // House tables join lazily, so the visible-room tail can be
                    // requested before membership lands and fail the member
                    // check, leaving room_id stuck in loading_tail_rooms. Clear
                    // it first so this post-join request actually issues instead
                    // of being suppressed as already-loading.
                    self.loading_tail_rooms.remove(&room_id);
                    self.request_room_tail(room_id);
                }
                ChatEvent::RoomFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomLeft { user_id, slug } if self.user_id == user_id => {
                    self.selected_room_id = None;
                    self.request_list();
                    banner = Some(Banner::success(&format!("Left #{slug}")));
                }
                ChatEvent::LeaveFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomCreated {
                    user_id,
                    room_id,
                    slug,
                } if self.user_id == user_id => {
                    // Every synthetic entry drops, the open cyberspace room
                    // included: the user is being moved into a real room.
                    self.clear_synthetic_selection();
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_chat_screen_switch = true;
                    banner = Some(Banner::success(&format!("Created #{slug}")));
                }
                ChatEvent::RoomCreateFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::PermanentRoomCreated { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Created permanent #{slug}")));
                }
                ChatEvent::PermanentRoomDeleted { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Deleted permanent #{slug}")));
                }
                ChatEvent::RoomFilled {
                    user_id,
                    slug,
                    users_added,
                } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!(
                        "Filled #{slug} ({users_added} users added)"
                    )));
                }
                ChatEvent::AdminFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::MessageDeleted {
                    user_id,
                    room_id,
                    message_id,
                } => {
                    self.remove_message(room_id, message_id);
                    if self.user_id == user_id {
                        banner = Some(Banner::success("Message deleted"));
                    }
                }
                ChatEvent::MessageRemoved {
                    room_id,
                    message_id,
                } => {
                    self.remove_message(room_id, message_id);
                }
                ChatEvent::MessageEdited {
                    message,
                    target_user_ids,
                    author_username,
                    author_bonsai_glyph,
                    author_chat_badge,
                    author_profile_award_badges,
                } => {
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    if let Some(username) = author_username {
                        self.note_username(message.user_id, username);
                    }
                    self.set_bonsai_glyph(message.user_id, author_bonsai_glyph.as_deref());
                    self.set_chat_badge(message.user_id, author_chat_badge.as_deref());
                    self.set_profile_award_badge(
                        message.user_id,
                        author_profile_award_badges.as_deref(),
                    );
                    self.replace_message(message);
                }
                ChatEvent::DiscoverRoomsLoaded { user_id, rooms } if self.user_id == user_id => {
                    self.discover.set_items(rooms);
                }
                ChatEvent::DiscoverRoomsFailed { user_id, message } if self.user_id == user_id => {
                    self.discover.finish_loading();
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::MessageSearchLoaded {
                    user_id,
                    request_id,
                    messages,
                    usernames,
                } if self.user_id == user_id => {
                    if !self.message_search.is_current(request_id) {
                        continue;
                    }
                    self.extend_usernames(usernames);
                    let query = self.message_search.query.clone();
                    let hits = messages
                        .into_iter()
                        .filter(|message| !self.message_is_ignored(message))
                        .map(|message| {
                            let (snippet_prefix, snippet_match, snippet_suffix) =
                                build_search_snippet(&message.body, &query);
                            MessageSearchHit {
                                message,
                                snippet_prefix,
                                snippet_match,
                                snippet_suffix,
                            }
                        })
                        .collect();
                    self.message_search.finish(hits);
                }
                ChatEvent::MessageSearchFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id && self.message_search.is_current(request_id) => {
                    self.message_search.fail(sentence_case(&message));
                }
                ChatEvent::HistoryPageLoaded {
                    user_id,
                    request_id,
                    direction,
                    messages,
                    usernames,
                } if self.user_id == user_id => {
                    self.history_modal
                        .apply_page(request_id, direction, messages, usernames);
                }
                ChatEvent::HistoryPageFailed {
                    user_id,
                    request_id,
                    direction,
                } if self.user_id == user_id => {
                    self.history_modal.apply_failed(request_id, direction);
                }
                ChatEvent::HistoryAnchorLoaded {
                    user_id,
                    request_id,
                    anchor_id,
                    messages,
                    usernames,
                } if self.user_id == user_id => {
                    self.history_modal
                        .apply_anchor(request_id, anchor_id, messages, usernames);
                }
                ChatEvent::HistoryAnchorMissing {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.history_modal.apply_anchor_missing(request_id);
                }
                ChatEvent::MessageContextLoaded {
                    user_id,
                    request_id,
                    message_id,
                    before,
                    after,
                    usernames,
                } if self.user_id == user_id => {
                    if self.message_search.context_in_flight != Some((request_id, message_id)) {
                        continue;
                    }
                    self.message_search.context_in_flight = None;
                    self.extend_usernames(usernames);
                    self.message_search
                        .context
                        .insert(message_id, MessageContext { before, after });
                }
                ChatEvent::MessageContextFailed {
                    user_id,
                    request_id,
                    message_id,
                } if self.user_id == user_id
                    && self.message_search.context_in_flight == Some((request_id, message_id)) =>
                {
                    self.message_search.context_in_flight = None;
                    // Cache an empty window so a persistent failure does not
                    // refire every tick; the hit still renders alone.
                    self.message_search
                        .context
                        .insert(message_id, MessageContext::default());
                }
                ChatEvent::MessageReactionsUpdated {
                    room_id,
                    message_id,
                    reactions,
                    target_user_ids,
                } => {
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    self.message_reactions.insert(message_id, reactions);
                    self.bump_room_version(room_id);
                }
                ChatEvent::MessageGildsUpdated {
                    room_id,
                    message_id,
                    summary,
                } => {
                    // Gilds only exist in public rooms, so there is no
                    // audience to filter: what one viewer of the room sees,
                    // every viewer sees.
                    match summary {
                        Some(summary) => {
                            self.message_gilds.insert(message_id, summary);
                        }
                        None => {
                            self.message_gilds.remove(&message_id);
                        }
                    }
                    self.bump_room_version(room_id);
                }
                ChatEvent::GildSucceeded {
                    user_id,
                    tier,
                    buyer_balance,
                    ..
                } if self.user_id == user_id => {
                    banner = Some(Banner::success(&format!(
                        "Gilded {} for {} chips ({buyer_balance} left)",
                        tier.label(),
                        tier.price()
                    )));
                }
                ChatEvent::GildSucceeded {
                    author_user_id,
                    tier,
                    buyer_username,
                    author_balance,
                    ..
                } if self.user_id == author_user_id => {
                    banner = Some(Banner::success(&format!(
                        "@{buyer_username} gilded your message {} (+{} chips, balance {author_balance})",
                        tier.marker(),
                        tier.author_share()
                    )));
                }
                ChatEvent::GildFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::EditSucceeded {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::success("Message edited"));
                }
                ChatEvent::EditFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::DeleteFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::IgnoreListUpdated {
                    user_id,
                    ignored_user_ids,
                    message,
                } if self.user_id == user_id => {
                    self.ignored_user_ids = ignored_user_ids.into_iter().collect();
                    self.friend_ignore_applied_at = Some(Instant::now());
                    self.refilter_local_messages();
                    self.notifications.list();
                    self.notifications.refresh_unread_count();
                    banner = Some(Banner::success(&message));
                }
                ChatEvent::IgnoreFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::FriendListUpdated {
                    user_id,
                    friend_user_ids,
                    target_user_id,
                    target_username,
                    message,
                } if self.user_id == user_id => {
                    let friend_user_ids: HashSet<Uuid> = friend_user_ids.into_iter().collect();
                    self.friend_ignore_applied_at = Some(Instant::now());
                    if self.friend_user_ids != friend_user_ids {
                        self.friend_user_ids = friend_user_ids;
                        self.context_epoch += 1;
                    }
                    self.note_username(target_user_id, target_username);
                    banner = Some(Banner::success(&message));
                }
                ChatEvent::FriendFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomMembersListed {
                    user_id,
                    title,
                    members,
                } if self.user_id == user_id => {
                    self.open_members_overlay(&title, members);
                }
                ChatEvent::GiftSucceeded {
                    user_id,
                    recipient_username,
                    amount,
                    sender_balance,
                    recipient_balance,
                    message,
                    ..
                } if self.user_id == user_id => {
                    let note = message
                        .as_deref()
                        .map(|m| format!(": \"{m}\""))
                        .unwrap_or_default();
                    banner = Some(Banner::success(&format!(
                        "Gifted {amount} chips to @{recipient_username} ({sender_balance} left, recipient {recipient_balance}){note}"
                    )));
                }
                ChatEvent::GiftSucceeded {
                    recipient_id,
                    sender_username,
                    amount,
                    recipient_balance,
                    message,
                    ..
                } if self.user_id == recipient_id => {
                    let note = message
                        .as_deref()
                        .map(|m| format!(": \"{m}\""))
                        .unwrap_or_default();
                    banner = Some(Banner::success(&format!(
                        "@{sender_username} gifted you {amount} chips (balance {recipient_balance}){note}"
                    )));
                }
                ChatEvent::GiftFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::PublicRoomsListed {
                    user_id,
                    title,
                    rooms,
                } if self.user_id == user_id => {
                    self.open_overlay(&title, rooms);
                }
                ChatEvent::InviteSucceeded {
                    user_id,
                    room_id,
                    room_slug,
                    username,
                } if self.user_id == user_id => {
                    if Some(room_id) == self.selected_room_id {
                        self.request_list();
                    }
                    banner = Some(Banner::success(&format!(
                        "Invited @{username} to #{room_slug}"
                    )));
                }
                ChatEvent::RoomMembersListFailed { user_id, message }
                    if self.user_id == user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::ReactionOwnersListed {
                    user_id,
                    message_id,
                    gilds,
                    owners,
                    usernames,
                } if self.user_id == user_id
                    && self.pending_reaction_owners_message_id == Some(message_id) =>
                {
                    self.pending_reaction_owners_message_id = None;
                    self.extend_usernames(usernames);
                    let lines = self.reaction_owner_lines(&gilds, &owners);
                    self.overlay = Some(Overlay::dismissible("Reactions", lines));
                }
                ChatEvent::ReactionOwnersListFailed { user_id, message }
                    if self.user_id == user_id
                        && self.pending_reaction_owners_message_id.is_some() =>
                {
                    self.pending_reaction_owners_message_id = None;
                    self.overlay = None;
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::PublicRoomsListFailed { user_id, message }
                    if self.user_id == user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::InviteFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomModSucceeded {
                    user_id,
                    room_slug,
                    username,
                    action,
                } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&match action {
                        RoomModAction::Kick => format!("Kicked @{username} from #{room_slug}"),
                        RoomModAction::Ban => format!("Banned @{username} from #{room_slug}"),
                        RoomModAction::Unban => format!("Unbanned @{username} in #{room_slug}"),
                    }));
                }
                ChatEvent::RoomModFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                // Not filtered to the editor: every session sitting in the room
                // is showing a stale header, and the room row is what the header
                // reads. Only the editor gets the banner.
                ChatEvent::RoomInfoUpdated {
                    user_id,
                    room_id,
                    room_slug,
                } => {
                    if self.rooms.iter().any(|(room, _)| room.id == room_id) {
                        self.request_list();
                    }
                    if self.user_id == user_id {
                        banner = Some(Banner::success(&format!("Updated #{room_slug}")));
                    }
                }
                ChatEvent::ModCommandOutput {
                    user_id,
                    request_id,
                    lines,
                    success,
                } if self.user_id == user_id => {
                    self.pending_mod_outputs.push_back(ModCommandOutput {
                        request_id,
                        lines,
                        success,
                    });
                }
                ChatEvent::PollUpdated {
                    actor_user_id,
                    room_id,
                    mut poll,
                    message,
                } => {
                    if self.user_id != actor_user_id {
                        poll.my_vote_option_id = self
                            .active_polls
                            .get(&room_id)
                            .filter(|existing| existing.poll.id == poll.poll.id)
                            .and_then(|existing| existing.my_vote_option_id);
                    }
                    // PollUpdated fires for votes too; only a previously
                    // unseen poll id in a room we're a member of is a fresh
                    // /poll start worth notifying about. The author is
                    // notified too, doubling as a delivery check.
                    let is_new_poll = self
                        .active_polls
                        .get(&room_id)
                        .is_none_or(|existing| existing.poll.id != poll.poll.id);
                    if is_new_poll && self.rooms.iter().any(|(room, _)| room.id == room_id) {
                        self.notifier
                            .push(Notification::poll_started(&poll.poll.question));
                    }
                    self.active_polls.insert(room_id, poll);
                    if self.user_id == actor_user_id {
                        banner = Some(Banner::success(&message));
                    }
                }
                ChatEvent::PollStartAllowed { user_id, room_id } if self.user_id == user_id => {
                    self.requested_poll_room = Some(room_id);
                }
                ChatEvent::PollFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }
        banner
    }

    fn drain_moderation_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            let event = match self.moderation_event_rx.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            };

            if let Some(message) = moderation_server_toast(&event) {
                banner = Some(Banner::success(&message));
            }
            if matches!(event, ModerationEvent::RoomRenamed { .. }) {
                self.request_list();
            }
        }
        banner
    }

    fn push_message(&mut self, message: ChatMessage) {
        let room_id = message.room_id;
        let created = message.created;
        if !self.rooms.iter().any(|(room, _)| room.id == room_id) {
            return;
        }

        // System-feed lines never become chat rows: they feed the activity
        // ticker instead. Unread counts already exclude the system author at
        // the SQL layer, so there is no cursor to keep aligned here.
        if let Some(text) = system_line_text_in(&self.usernames, &message) {
            self.note_activity_ticker(ActivityTickerEntry {
                id: message.id,
                text,
                at: created,
            });
            return;
        }

        // Speaking in a room clears its AFK line: you are in the conversation
        // now, whatever the keyboard was doing before. Done on the message
        // landing rather than at each of the several submit paths (`/me`,
        // `/roll`, `/cup`, a plain line) so there is one place to look, and
        // it is the authoritative one: the line goes when the message that
        // ends the silence actually exists.
        if message.user_id == self.user_id {
            self.clear_afk_line(room_id);
        }

        let is_viewing_room = Some(room_id) == self.visible_room_id;
        if self.message_is_ignored(&message) {
            if is_viewing_room {
                self.mark_room_read(room_id);
            }
            return;
        }

        self.note_room_message_activity(room_id, created);

        let Some((_, messages)) = self.rooms.iter_mut().find(|(room, _)| room.id == room_id) else {
            return;
        };

        if messages.iter().any(|existing| existing.id == message.id) {
            return;
        }

        // Service snapshots are newest-first; keep same order for cheap appends at the front.
        messages.insert(0, message);
        if messages.len() > 500 {
            let removed_ids: Vec<Uuid> = messages
                .iter()
                .skip(500)
                .map(|message| message.id)
                .collect();
            messages.truncate(500);
            for message_id in removed_ids {
                self.message_reactions.remove(&message_id);
                self.message_gilds.remove(&message_id);
                // Evicted messages can never render again this session, so
                // their translation state is dead weight; without this a
                // long-lived auto-translate session grows unbounded.
                self.forget_translation(message_id);
            }
        }
        self.bump_room_version(room_id);

        if is_viewing_room {
            // Keep the DB cursor aligned with the visible live stream. Without
            // this, the next snapshot can restore unread counts until the user
            // switches away and back into the room.
            self.mark_room_read(room_id);
        }
    }

    fn remove_message(&mut self, room_id: Uuid, message_id: Uuid) {
        let mut changed = false;
        if let Some((_, messages)) = self.rooms.iter_mut().find(|(room, _)| room.id == room_id) {
            let before = messages.len();
            messages.retain(|m| m.id != message_id);
            changed = messages.len() != before;
        }
        if self.message_reactions.remove(&message_id).is_some() {
            changed = true;
        }
        // The gild rows went with the message (`ON DELETE CASCADE`), so the
        // marker must go too.
        if self.message_gilds.remove(&message_id).is_some() {
            changed = true;
        }
        self.forget_translation(message_id);
        if changed {
            self.bump_room_version(room_id);
        }
    }

    /// Drop every per-message translation trace: the message was deleted or
    /// its body changed, so what we knew describes text that no longer
    /// exists. An edit can be re-translated fresh (`t` or auto).
    fn forget_translation(&mut self, message_id: Uuid) {
        self.translations.remove(&message_id);
        self.translation_hidden.remove(&message_id);
        self.translation_manual.remove(&message_id);
        self.translation_cache_checked.remove(&message_id);
    }

    /// Drop every Pending translation entry so `t` can re-request. Called
    /// when the event receiver lagged: the result for a Pending entry may
    /// have been among the dropped events, and no later event clears it.
    fn reset_pending_translations(&mut self) {
        let stuck: Vec<Uuid> = self
            .translations
            .iter()
            .filter(|(_, display)| **display == TranslationDisplay::Pending)
            .map(|(message_id, _)| *message_id)
            .collect();
        if stuck.is_empty() {
            return;
        }
        let rooms: HashSet<Uuid> = stuck
            .iter()
            .filter_map(|message_id| {
                self.rooms.iter().find_map(|(room, messages)| {
                    messages
                        .iter()
                        .any(|message| message.id == *message_id)
                        .then_some(room.id)
                })
            })
            .collect();
        for message_id in stuck {
            self.translations.remove(&message_id);
            self.translation_manual.remove(&message_id);
        }
        for room_id in rooms {
            self.bump_room_version(room_id);
        }
    }

    pub(crate) fn remove_room_for_moderation(&mut self, room_id: Uuid) {
        self.rooms.retain(|(room, _)| room.id != room_id);
        self.unread_counts.remove(&room_id);
        if self.selected_room_id == Some(room_id) {
            self.selected_room_id = None;
        }
        if self.visible_room_id == Some(room_id) {
            self.visible_room_id = None;
        }
        if self.composer_room_id == Some(room_id) {
            self.clear_composer_after_submit();
        }
        self.sync_selection();
    }

    fn merge_room_tail(&mut self, room_id: Uuid, messages: Vec<ChatMessage>) {
        self.note_activity_ticker_from(&messages);
        let Some((_, stored)) = self.rooms.iter_mut().find(|(room, _)| room.id == room_id) else {
            return;
        };

        let mut merged = Vec::with_capacity(stored.len() + messages.len());
        let mut seen = HashSet::new();
        for message in messages.into_iter().chain(stored.iter().cloned()) {
            if seen.insert(message.id) {
                merged.push(message);
            }
        }
        merged.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
        merged.truncate(500);

        let ignored = &self.ignored_user_ids;
        let usernames = &self.usernames;
        let before: Vec<(Uuid, DateTime<Utc>)> = stored.iter().map(|m| (m.id, m.updated)).collect();
        *stored = merged
            .into_iter()
            .filter(|message| {
                !message_is_ignored_in(ignored, message)
                    && system_line_text_in(usernames, message).is_none()
            })
            .collect();
        let changed = stored.len() != before.len()
            || stored
                .iter()
                .zip(&before)
                .any(|(m, (id, updated))| m.id != *id || m.updated != *updated);
        if changed {
            self.bump_room_version(room_id);
        }
    }

    fn replace_message(&mut self, message: ChatMessage) {
        let room_id = message.room_id;
        let message_id = message.id;
        let mut replaced = false;
        if let Some((_, messages)) = self
            .rooms
            .iter_mut()
            .find(|(room, _)| room.id == message.room_id)
            && let Some(existing) = messages.iter_mut().find(|m| m.id == message.id)
        {
            *existing = message;
            replaced = true;
        }
        if replaced {
            self.forget_translation(message_id);
            self.bump_room_version(room_id);
        }
    }

    fn merge_rooms(
        &self,
        incoming: Vec<(ChatRoom, Vec<ChatMessage>)>,
    ) -> Vec<(ChatRoom, Vec<ChatMessage>)> {
        let previous_by_room: HashMap<Uuid, &Vec<ChatMessage>> = self
            .rooms
            .iter()
            .map(|(room, msgs)| (room.id, msgs))
            .collect();

        incoming
            .into_iter()
            .map(|(room, messages)| {
                let messages = if messages.is_empty() {
                    previous_by_room
                        .get(&room.id)
                        .map(|previous| (*previous).clone())
                        .unwrap_or_default()
                } else {
                    messages
                };
                let messages = self.filter_messages(messages);
                (room, messages)
            })
            .collect()
    }

    fn merge_unread_counts(&mut self, mut incoming: HashMap<Uuid, i64>) -> HashMap<Uuid, i64> {
        self.pending_read_rooms
            .retain(|room_id| match incoming.get(room_id).copied() {
                Some(0) => false,
                Some(_) => {
                    incoming.insert(*room_id, 0);
                    true
                }
                None => true,
            });
        incoming
    }

    fn merge_room_last_message_at(
        &self,
        mut incoming: HashMap<Uuid, Option<DateTime<Utc>>>,
    ) -> HashMap<Uuid, Option<DateTime<Utc>>> {
        for (room_id, current) in &self.room_last_message_at {
            if let Some(incoming_value) = incoming.get_mut(room_id) {
                let current_value = *current;
                if current_value > *incoming_value {
                    *incoming_value = current_value;
                }
            }
        }
        incoming
    }

    fn note_room_message_activity(&mut self, room_id: Uuid, created: DateTime<Utc>) {
        let latest = self.room_last_message_at.entry(room_id).or_insert(None);
        let should_update = latest
            .as_ref()
            .map(|current| created > *current)
            .unwrap_or(true);
        if should_update {
            *latest = Some(created);
        }
    }

    fn merge_message_reactions(
        &self,
        incoming: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    ) -> HashMap<Uuid, Vec<ChatMessageReactionSummary>> {
        let visible_message_ids: HashSet<Uuid> = self
            .rooms
            .iter()
            .flat_map(|(_, messages)| messages.iter().map(|message| message.id))
            .collect();
        let mut merged: HashMap<Uuid, Vec<ChatMessageReactionSummary>> = self
            .message_reactions
            .iter()
            .filter(|(message_id, _)| visible_message_ids.contains(message_id))
            .map(|(message_id, reactions)| (*message_id, reactions.clone()))
            .collect();
        for (message_id, reactions) in incoming {
            merged.insert(message_id, reactions);
        }
        merged
    }

    fn filter_messages(&self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .filter(|message| {
                !self.message_is_ignored(message)
                    && system_line_text_in(&self.usernames, message).is_none()
            })
            .collect()
    }

    fn note_activity_ticker(&mut self, entry: ActivityTickerEntry) -> bool {
        note_ticker_entry(&mut self.activity_ticker, entry)
    }

    fn note_activity_ticker_from(&mut self, messages: &[ChatMessage]) -> bool {
        let mut changed = false;
        for message in messages {
            if let Some(text) = system_line_text_in(&self.usernames, message) {
                changed |= self.note_activity_ticker(ActivityTickerEntry {
                    id: message.id,
                    text,
                    at: message.created,
                });
            }
        }
        changed
    }

    fn message_is_ignored(&self, message: &ChatMessage) -> bool {
        message_is_ignored_in(&self.ignored_user_ids, message)
    }

    /// Strip already-stored messages from any newly-ignored author, including
    /// DMs and bot replies directed at the newly-ignored user.
    fn refilter_local_messages(&mut self) {
        let ignored = &self.ignored_user_ids;
        for (_, messages) in &mut self.rooms {
            messages.retain(|m| !message_is_ignored_in(ignored, m));
        }
        // Every room may have lost rows; the epoch invalidates all row caches.
        self.context_epoch += 1;
        self.sync_selection();
    }
}

/// One diverted #lounge system line held for the activity ticker.
pub struct ActivityTickerEntry {
    pub id: Uuid,
    pub text: String,
    pub at: DateTime<Utc>,
}

/// The ticker queue length: enough that packing left to right always fills
/// the row on any sane terminal width, without hoarding history.
const ACTIVITY_TICKER_CAP: usize = 10;

/// Insert into the newest-first ticker queue, deduped by message id (tails
/// and snapshots replay the same lines), capped at `ACTIVITY_TICKER_CAP`.
/// Returns true when the entry actually landed in the ticker (a duplicate
/// delivery leaves it untouched).
fn note_ticker_entry(entries: &mut Vec<ActivityTickerEntry>, entry: ActivityTickerEntry) -> bool {
    if entries.iter().any(|existing| existing.id == entry.id) {
        return false;
    }
    let pos = entries
        .iter()
        .position(|existing| entry.at >= existing.at)
        .unwrap_or(entries.len());
    entries.insert(pos, entry);
    entries.truncate(ACTIVITY_TICKER_CAP);
    true
}

/// The #lounge system-feed check (author is the `system` bot AND the body
/// carries the `· ` prefix — same spoof guard as `ui.rs::is_system_author`).
/// Returns the display text when the message is a system line.
fn system_line_text_in(usernames: &HashMap<Uuid, String>, message: &ChatMessage) -> Option<String> {
    usernames
        .get(&message.user_id)
        .filter(|name| crate::app::activity::lounge::is_system_username(name))
        .and_then(|_| super::ui_text::parse_system_line(&message.body))
        .map(str::to_string)
}

/// A message is ignored if its author is ignored, or if it is a bot/automated
/// reply directed at an ignored user (so an ignored user can't be heard by
/// proxy through a bot).
fn message_is_ignored_in(ignored: &HashSet<Uuid>, message: &ChatMessage) -> bool {
    ignored.contains(&message.user_id)
        || message
            .reply_to_user_id
            .is_some_and(|target| ignored.contains(&target))
}

fn inline_image_request_candidates(
    messages: &[ChatMessage],
    requested: &HashSet<Uuid>,
    cached: &HashMap<Uuid, InlineImagePreview>,
    failures: &HashMap<Uuid, InlineImageFailure>,
    now: Instant,
) -> Vec<(Uuid, String)> {
    let mut requests = Vec::new();
    for msg in messages.iter().take(INLINE_IMAGE_SCAN_LIMIT) {
        if requested.contains(&msg.id) || cached.contains_key(&msg.id) {
            continue;
        }
        if let Some(failure) = failures.get(&msg.id)
            && (failure.attempts >= INLINE_IMAGE_MAX_FAILURES || now < failure.next_retry_at)
        {
            continue;
        }
        if let Some(url) = inline_image_url_in_body(&msg.body) {
            tracing::trace!("found image url in chat: {}", url);
            requests.push((msg.id, url));
            if requests.len() >= INLINE_IMAGE_FETCHES_PER_TICK {
                break;
            }
        }
    }
    requests
}

fn inline_image_url_in_body(body: &str) -> Option<String> {
    let mut rest = body;
    while let Some(url_start) = rest.find("http") {
        let url_str = &rest[url_start..];
        let end_idx = url_str
            .find(|c: char| c.is_ascii_whitespace() || c == ')' || c == ']' || c == '}')
            .unwrap_or(url_str.len());
        let mut url = &url_str[..end_idx];
        while url.ends_with('.')
            || url.ends_with(',')
            || url.ends_with(';')
            || url.ends_with('!')
            || url.ends_with('?')
        {
            url = &url[..url.len() - 1];
        }

        if is_inline_image_url(url) {
            return Some(url.to_string());
        }

        rest = &url_str["http".len()..];
    }
    None
}

fn is_inline_image_url(url: &str) -> bool {
    let lower_url = url.to_ascii_lowercase();
    if lower_url.contains("uguu.se")
        || lower_url.contains("0x0.st")
        || lower_url.contains("catbox.moe")
    {
        return true;
    }

    let path = reqwest::Url::parse(url)
        .ok()
        .map(|parsed| parsed.path().to_ascii_lowercase())
        .unwrap_or(lower_url);

    [".jpg", ".jpeg", ".png", ".gif", ".webp"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

fn inline_image_retry_delay(attempts: u8) -> Duration {
    let exp = attempts.saturating_sub(1).min(5) as u32;
    Duration::from_secs((1_u64 << exp).min(30))
}

pub(crate) struct RoomVisualOrderInput<'a, U: UsernameResolver + ?Sized> {
    pub rooms: &'a [(ChatRoom, Vec<ChatMessage>)],
    pub user_id: Uuid,
    pub usernames: &'a U,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub room_last_message_at: &'a HashMap<Uuid, Option<DateTime<Utc>>>,
    pub feeds_available: bool,
    pub cyberspace_linked: bool,
    /// Pinned cyberspace chat rooms, in rail order. Slots carry the index
    /// into this list.
    pub cyberspace_rooms: &'a [String],
    /// Pinned C-Mail conversations, in rail order, after the rooms.
    pub cyberspace_mail: &'a [CmailThread],
    pub favorite_room_ids: &'a [Uuid],
    pub collapsed_sections: &'a HashSet<RoomSection>,
    pub ignored_user_ids: &'a HashSet<Uuid>,
    pub sticky_unread_dm: Option<Uuid>,
    /// Registered "watch me" streams, in registry order. Each contributes a
    /// `stream` section row whether or not this user joined its room yet.
    pub live_streams: &'a [crate::app::stream::registry::LiveStreamView],
}

pub(crate) fn visual_order_for_rooms<U: UsernameResolver + ?Sized>(
    input: RoomVisualOrderInput<'_, U>,
) -> Vec<RoomSlot> {
    let RoomVisualOrderInput {
        rooms,
        user_id,
        usernames,
        unread_counts,
        room_last_message_at,
        feeds_available,
        cyberspace_linked,
        cyberspace_rooms,
        cyberspace_mail,
        favorite_room_ids,
        collapsed_sections,
        ignored_user_ids,
        sticky_unread_dm,
        live_streams,
    } = input;

    let mut order = Vec::new();
    let mut pushed_rooms = HashSet::new();

    // `pushed_rooms` must track membership even for collapsed sections so a
    // room can't reappear later (e.g. a collapsed favorite leaking into
    // Channels). Each section computes its slots, records them as pushed,
    // then only appends to `order` when the section is expanded.
    let favorites_collapsed = collapsed_sections.contains(&RoomSection::Favorites);
    for favorite_id in favorite_room_ids {
        if rooms.iter().any(|(room, _)| {
            room.id == *favorite_id
                && is_chat_list_room(room)
                && !dm_peer_is_ignored(room, user_id, ignored_user_ids)
        }) && pushed_rooms.insert(*favorite_id)
            && !favorites_collapsed
        {
            order.push(RoomSlot::Room(*favorite_id));
        }
    }

    // Core: permanent rooms, hardcoded order
    let core_collapsed = collapsed_sections.contains(&RoomSection::Core);
    let core_order = ["lounge", "announcements", "suggestions", "bugs"];
    for slug in &core_order {
        if let Some((room, _)) = rooms
            .iter()
            .find(|(r, _)| is_chat_list_room(r) && r.permanent && r.slug.as_deref() == Some(slug))
            && pushed_rooms.insert(room.id)
            && !core_collapsed
        {
            order.push(RoomSlot::Room(room.id));
        }
    }
    if !core_collapsed {
        order.push(RoomSlot::Notifications);
        order.push(RoomSlot::News);
        if feeds_available {
            order.push(RoomSlot::Feeds);
        }
    }

    // Voice sits directly above Discover ("+ browse rooms") at the bottom of Core.
    if let Some((room, _)) = rooms
        .iter()
        .find(|(r, _)| is_chat_list_room(r) && r.permanent && r.slug.as_deref() == Some("voice"))
        && pushed_rooms.insert(room.id)
        && !core_collapsed
    {
        order.push(RoomSlot::Room(room.id));
    }
    if !core_collapsed {
        // Discover ("browse rooms") lives at the bottom of Core.
        order.push(RoomSlot::Discover);
    }

    // Stream: one row per registered "watch me" stream, directly under Core.
    // The section exists only while somebody is streaming. Stream rooms are
    // `kind='game'` so they can never leak into Channels/DMs below.
    let stream_collapsed = collapsed_sections.contains(&RoomSection::Stream);
    for stream in live_streams {
        if pushed_rooms.insert(stream.room_id) && !stream_collapsed {
            order.push(RoomSlot::Room(stream.room_id));
        }
    }

    // Cyberspace: the feeds pane, their notifications, then the chat rooms
    // and c-mail conversations this user pinned, under their own header.
    // Linked accounts only. Everyone else reaches the pitch
    // + login funnel through `/cs`, so the rail stays about places this user
    // actually has. Mirrored by the rail builder in `ui.rs`.
    if cyberspace_linked && !collapsed_sections.contains(&RoomSection::Cyberspace) {
        order.push(RoomSlot::Cyberspace);
        order.push(RoomSlot::CyberspaceNotifications);
        for index in 0..cyberspace_rooms.len() {
            order.push(RoomSlot::CyberspaceRoom(index));
        }
        for index in 0..cyberspace_mail.len() {
            order.push(RoomSlot::CyberspaceMail(index));
        }
    }

    // Unread DMs ride above Channels: at the bottom of the rail nobody was
    // finding them. Favorited DMs keep their favorites slot instead (they are
    // already in `pushed_rooms`), and the group ignores the DMs collapse
    // toggle, which is what makes collapsing DMs a way to hide the read ones
    // without losing the ones asking for an answer.
    let mut unread_dms: Vec<_> = rooms
        .iter()
        .filter(|(r, _)| is_chat_list_room(r) && r.kind == "dm")
        .filter(|(r, _)| !dm_peer_is_ignored(r, user_id, ignored_user_ids))
        .filter(|(r, _)| !pushed_rooms.contains(&r.id))
        .filter(|(r, _)| dm_is_promoted_unread(r.id, unread_counts, sticky_unread_dm))
        .collect();
    unread_dms.sort_by(|(a_room, _), (b_room, _)| {
        compare_dm_rooms_for_nav(
            a_room,
            b_room,
            user_id,
            usernames,
            unread_counts,
            room_last_message_at,
        )
    });
    order.extend(
        unread_dms
            .iter()
            .filter_map(|(r, _)| pushed_rooms.insert(r.id).then_some(RoomSlot::Room(r.id))),
    );

    // Channels: all non-DM rooms outside Core, public + private merged.
    let channels_collapsed = collapsed_sections.contains(&RoomSection::Channels);
    for (room, _) in rooms {
        if is_chat_list_room(room)
            && room.kind != "dm"
            && !core_order.contains(&room.slug.as_deref().unwrap_or(""))
            && room.slug.as_deref() != Some("voice")
            && pushed_rooms.insert(room.id)
            && !channels_collapsed
        {
            order.push(RoomSlot::Room(room.id));
        }
    }

    // DMs: unread rooms first, then newest message, then display name.
    // Hide DMs whose other participant is ignored so an ignored user can't
    // resurface the DM (and its unread badge) by sending again.
    let dms_collapsed = collapsed_sections.contains(&RoomSection::Dms);
    let mut dms: Vec<_> = rooms
        .iter()
        .filter(|(r, _)| is_chat_list_room(r) && r.kind == "dm")
        .filter(|(r, _)| !dm_peer_is_ignored(r, user_id, ignored_user_ids))
        .collect();
    dms.sort_by(|(a_room, _), (b_room, _)| {
        compare_dm_rooms_for_nav(
            a_room,
            b_room,
            user_id,
            usernames,
            unread_counts,
            room_last_message_at,
        )
    });
    order.extend(dms.iter().filter_map(|(r, _)| {
        (pushed_rooms.insert(r.id) && !dms_collapsed).then_some(RoomSlot::Room(r.id))
    }));

    order
}

pub(crate) struct NextStickyUnreadDm {
    pub current: Option<Uuid>,
    pub room_id: Uuid,
    pub is_dm: bool,
    pub unread: bool,
}

/// Which DM the promoted unread group holds after `room_id` is marked read.
///
/// Opening a DM zeroes its unread count on the same frame, so without this the
/// row (and its jump key) would move out from under the cursor the instant you
/// arrive. The DM you are reading stays put: every message landing in a
/// visible room re-marks it read, and that must not release it. Reading
/// anything else does release it, so the DM drops back down as soon as you
/// open another room. Nothing else releases it: a screen with no visible chat
/// room never marks anything read, so the DM keeps the promoted slot until you
/// come back and open something.
pub(crate) fn next_sticky_unread_dm(input: NextStickyUnreadDm) -> Option<Uuid> {
    let NextStickyUnreadDm {
        current,
        room_id,
        is_dm,
        unread,
    } = input;

    match current {
        Some(sticky) if sticky == room_id => Some(sticky),
        _ => (is_dm && unread).then_some(room_id),
    }
}

/// Whether a DM belongs in the promoted unread group above Channels: it has
/// unread messages, or it is the DM being read right now (see
/// `ChatState::note_sticky_unread_dm`). Shared by the navigation order and the
/// rail so the two cannot disagree about which DMs got promoted.
pub(crate) fn dm_is_promoted_unread(
    room_id: Uuid,
    unread_counts: &HashMap<Uuid, i64>,
    sticky_unread_dm: Option<Uuid>,
) -> bool {
    unread_counts.get(&room_id).copied().unwrap_or(0) > 0 || sticky_unread_dm == Some(room_id)
}

pub(crate) fn compare_dm_rooms_for_nav(
    a_room: &ChatRoom,
    b_room: &ChatRoom,
    user_id: Uuid,
    usernames: &(impl UsernameResolver + ?Sized),
    unread_counts: &HashMap<Uuid, i64>,
    room_last_message_at: &HashMap<Uuid, Option<DateTime<Utc>>>,
) -> Ordering {
    let a_unread = unread_counts.get(&a_room.id).copied().unwrap_or(0) > 0;
    let b_unread = unread_counts.get(&b_room.id).copied().unwrap_or(0) > 0;
    b_unread
        .cmp(&a_unread)
        .then_with(|| {
            room_activity_at(b_room.id, room_last_message_at)
                .cmp(&room_activity_at(a_room.id, room_last_message_at))
        })
        .then_with(|| {
            dm_sort_key(a_room, user_id, usernames).cmp(&dm_sort_key(b_room, user_id, usernames))
        })
        .then_with(|| a_room.id.cmp(&b_room.id))
}

pub(crate) fn room_activity_at(
    room_id: Uuid,
    room_last_message_at: &HashMap<Uuid, Option<DateTime<Utc>>>,
) -> Option<DateTime<Utc>> {
    room_last_message_at.get(&room_id).cloned().flatten()
}

/// The other participant in a DM room, from `user_id`'s perspective.
fn dm_peer_id(room: &ChatRoom, user_id: Uuid) -> Option<Uuid> {
    if room.dm_user_a == Some(user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    }
}

/// Whether `room` is a DM whose other participant is ignored. Such DMs are
/// hidden from every room-list section (favorites included) so an ignored peer
/// can't resurface the DM or its unread state by sending again.
pub(crate) fn dm_peer_is_ignored(room: &ChatRoom, user_id: Uuid, ignored: &HashSet<Uuid>) -> bool {
    room.kind == "dm" && dm_peer_id(room, user_id).is_some_and(|peer| ignored.contains(&peer))
}

/// Sort key for DMs: resolves the other participant's username.
fn dm_sort_key(
    room: &ChatRoom,
    user_id: Uuid,
    usernames: &(impl UsernameResolver + ?Sized),
) -> String {
    dm_peer_id(room, user_id)
        .and_then(|id| usernames.username(&id))
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| "DM".to_string())
}

fn moderation_server_toast(event: &ModerationEvent) -> Option<String> {
    let ModerationEvent::ServerUserAction {
        target_username,
        action,
        ..
    } = event
    else {
        return None;
    };

    match action {
        ServerUserAction::Kick => Some(format!("@{target_username} was kicked from the server")),
        ServerUserAction::Ban => Some(format!("@{target_username} was banned from the server")),
        ServerUserAction::Unban => None,
    }
}

/// A parsed `/petname` command, drained by `handle_post_submit_requests`
/// (which has the `App` access needed to update the cat).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PetnameRequest {
    /// `/petname` with no argument — show the current name.
    Show,
    /// `/petname <name>` — set it. Holds the normalised name.
    Set(String),
    /// `/petname clear` — remove the name.
    Clear,
}

/// Outcome of parsing a `/petname` line.
pub(crate) enum PetnameParse {
    Request(PetnameRequest),
    /// `/petname` with an argument that normalised to nothing.
    Invalid,
}

/// Parse a `/petname` command. Returns `None` if the input isn't a
/// `/petname` command so `/petnames` (typo) still falls through to the
/// unknown-command handler.
pub(crate) fn parse_petname_command(input: &str) -> Option<PetnameParse> {
    let rest = input.trim().strip_prefix("/petname")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let arg = rest.trim();
    if arg.is_empty() {
        return Some(PetnameParse::Request(PetnameRequest::Show));
    }
    if matches!(
        arg.to_ascii_lowercase().as_str(),
        "clear" | "remove" | "none" | "off"
    ) {
        return Some(PetnameParse::Request(PetnameRequest::Clear));
    }
    match late_core::models::pet::normalize_pet_name(arg) {
        Some(name) => Some(PetnameParse::Request(PetnameRequest::Set(name))),
        None => Some(PetnameParse::Invalid),
    }
}

/// Parse `/dm @username` or `/dm username` from the composer text.
/// Returns the target username if the input matches.
fn parse_dm_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/dm ")?.trim_start();
    let username = rest.strip_prefix('@').unwrap_or(rest).trim();
    if username.is_empty() {
        return None;
    }
    Some(username)
}

/// Max length of the optional note attached to a `/gift`.
const GIFT_MESSAGE_MAX_CHARS: usize = 120;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GiftParse {
    Invalid,
    Gift {
        username: String,
        amount: i64,
        /// Optional note: `/gift @user 100 happy birthday`.
        message: Option<String>,
    },
}

pub(crate) fn parse_gift_command(input: &str) -> Option<GiftParse> {
    let rest = input.trim().strip_prefix("/gift")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let mut parts = rest.split_whitespace();
    let Some(username) = parts.next() else {
        return Some(GiftParse::Invalid);
    };
    let Some(amount) = parts.next() else {
        return Some(GiftParse::Invalid);
    };
    // Everything after the amount is an optional single-line note. Rejoining
    // with single spaces drops any newlines/tabs; then strip control chars and
    // cap the length.
    let message: String = parts
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|ch| !ch.is_control())
        .take(GIFT_MESSAGE_MAX_CHARS)
        .collect();
    let message = {
        let trimmed = message.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };
    let username = username.strip_prefix('@').unwrap_or(username).trim();
    let Ok(amount) = amount.parse::<i64>() else {
        return Some(GiftParse::Invalid);
    };
    if username.is_empty() || amount <= 0 || amount > GIFT_MAX_AMOUNT {
        return Some(GiftParse::Invalid);
    }
    Some(GiftParse::Gift {
        username: username.to_string(),
        amount,
        message,
    })
}

/// A `/pair` request drained by `handle_post_submit_requests` (the composer
/// has no handle on `active_users`/the scratchpad registry of its own).
/// Only the directed form exists: `/pair` has no open/anyone-can-claim
/// lobby, unlike the daily challenge flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PairRequest {
    Directed(String),
}

/// `Some(Some(PairRequest::Directed(username)))` on `/pair @user`,
/// `Some(None)` on a malformed line (usage banner), `None` when it isn't
/// `/pair` at all.
fn parse_pair_command(input: &str) -> Option<Option<PairRequest>> {
    let trimmed = input.trim();
    if trimmed == "/pair" {
        return Some(None);
    }
    let rest = trimmed.strip_prefix("/pair ")?;
    let mut tokens = rest.split_whitespace();
    let Some(first) = tokens.next() else {
        return Some(None);
    };
    let Some(username) = first.strip_prefix('@').filter(|name| !name.is_empty()) else {
        return Some(None);
    };
    if tokens.next().is_some() {
        return Some(None);
    }
    Some(Some(PairRequest::Directed(username.to_string())))
}

/// One classic tomato when `/pomodoro` is given no duration.
const POMODORO_DEFAULT_MINUTES: u32 = 25;
/// Well past any real focus block, and short enough that the HUD badge stays
/// two-digit minutes.
const POMODORO_MAX_MINUTES: u32 = 180;
/// The badge shares the top border with mentions, voice, and chips, so the
/// label has to stay short. Display cells, not chars: a CJK or emoji label
/// costs two cells per char and would otherwise crowd the border out.
const POMODORO_LABEL_MAX_COLS: usize = 24;
const POMODORO_DEFAULT_LABEL: &str = "Pomodoro";

/// A `/pomodoro` request drained by `handle_post_submit_requests`. The timer
/// itself lives on `App` (not here): `tick.rs` fires it and the status HUD
/// draws it from every screen, not just chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PomodoroRequest {
    Start { minutes: u32, label: String },
    Stop,
}

/// Outcome of parsing a `/pomodoro` line. Same shape as [`PetnameParse`]: a
/// named variant per outcome instead of a nested `Option`, so a call site
/// cannot read "malformed" as "absent".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PomodoroParse {
    Request(PomodoroRequest),
    /// `/pomodoro` with an out-of-range duration or a `stop` carrying
    /// arguments.
    Invalid,
}

/// `None` when the line isn't `/pomodoro` at all.
///
/// A leading integer is the duration and everything after it is the label, so
/// `/pomodoro 50 deep work` and the label-only `/pomodoro deep work` (default
/// duration) both work. Only an out-of-range duration is an error.
fn parse_pomodoro_command(input: &str) -> Option<PomodoroParse> {
    let rest = input.trim().strip_prefix("/pomodoro")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let mut words = rest.split_whitespace().peekable();
    // `stop` is checked before the label path so it can never be read as one.
    if words
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("stop"))
    {
        words.next();
        return Some(match words.next() {
            None => PomodoroParse::Request(PomodoroRequest::Stop),
            Some(_) => PomodoroParse::Invalid,
        });
    }
    let mut minutes = POMODORO_DEFAULT_MINUTES;
    if words
        .peek()
        .is_some_and(|word| word.chars().all(|ch| ch.is_ascii_digit()))
    {
        let digits = words.next().unwrap_or_default();
        // Out of range (including a digit run too long for u32) is a usage
        // banner rather than a silent clamp.
        match digits.parse::<u32>() {
            Ok(parsed) if (1..=POMODORO_MAX_MINUTES).contains(&parsed) => minutes = parsed,
            _ => return Some(PomodoroParse::Invalid),
        }
    }
    // Rejoining with single spaces drops any tabs/newlines; then strip control
    // chars (the label reaches a desktop notification and the top border) and
    // cap the width, same shape as the `/gift` note.
    use unicode_width::UnicodeWidthChar;
    let mut cols = 0usize;
    let label: String = words
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|ch| !ch.is_control())
        .take_while(|ch| {
            cols += ch.width().unwrap_or(0);
            cols <= POMODORO_LABEL_MAX_COLS
        })
        .collect();
    let label = label.trim();
    let label = if label.is_empty() {
        POMODORO_DEFAULT_LABEL.to_string()
    } else {
        label.to_string()
    };
    Some(PomodoroParse::Request(PomodoroRequest::Start {
        minutes,
        label,
    }))
}

fn parse_me_command(input: &str) -> Option<Option<String>> {
    let trimmed = input.trim();
    if trimmed == "/me" {
        return Some(None);
    }
    let rest = trimmed.strip_prefix("/me ")?;
    Some(super::action::encode_action_body(rest))
}

fn format_member_overlay_lines(
    members: &[RoomMemberListItem],
    active_users: Option<&ActiveUsers>,
) -> Vec<Line<'static>> {
    let online_ids = active_users
        .map(|users| users.lock_recover().keys().copied().collect::<HashSet<_>>())
        .unwrap_or_default();
    let mut rows = members
        .iter()
        .map(|member| {
            let online = online_ids.contains(&member.user_id);
            let label = member
                .username
                .as_deref()
                .map(|username| format!("@{username}"))
                .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(member.user_id)));
            (online, label.to_ascii_lowercase(), label)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    rows.into_iter()
        .map(|(online, _, label)| {
            let (status, status_style, name_style) = if online {
                (
                    "[on ]",
                    Style::default()
                        .fg(theme::SUCCESS())
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(theme::TEXT()),
                )
            } else {
                (
                    "[off]",
                    Style::default().fg(theme::TEXT_DIM()),
                    Style::default().fg(theme::TEXT_DIM()),
                )
            };
            Line::from(vec![
                Span::raw(" "),
                Span::styled(status, status_style),
                Span::raw(" "),
                Span::styled(label, name_style),
            ])
        })
        .collect()
}

/// Parse `/leave` from the composer text.
fn parse_leave_command(input: &str) -> bool {
    input.trim() == "/leave"
}

/// `/roominfo` (or `/roomedit`) opens the room-info form for the current room.
fn is_room_info_command(input: &str) -> bool {
    matches!(input.trim(), "/roominfo" | "/roomedit")
}

/// Parse `/public <slug>` or `/private <slug>` style commands.
fn parse_room_command<'a>(input: &'a str, command: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(&format!("{command} "))?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse the composer text for a command that opens a public room. `/join` is
/// an alias for `/public`, not a second behavior: it is the name an IRC user
/// reaches for first, and the IRC bridge already answers to it.
fn parse_public_room_command(input: &str) -> Option<&str> {
    match parse_room_command(input, "/public") {
        Some(slug) => Some(slug),
        None => parse_room_command(input, "/join"),
    }
}

fn user_created_channel_name_too_long(slug: &str) -> bool {
    slug.chars().count() > USER_CREATED_CHANNEL_NAME_MAX_CHARS
}

fn user_created_channel_name_length_error() -> Banner {
    Banner::error(&format!(
        "Channel names must be {USER_CREATED_CHANNEL_NAME_MAX_CHARS} characters or fewer"
    ))
}

/// Parse `/create-room <slug>` from the composer text (admin only).
fn parse_create_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/create-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse `/delete-room <slug>` from the composer text (admin only).
fn parse_delete_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/delete-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse `/fill-room <slug>` from the composer text (admin only).
fn parse_fill_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/fill-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DieSpec {
    pub count: u32,
    pub sides: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RollParse {
    Invalid,
    Specs(Vec<DieSpec>),
}

const ROLL_MAX_DICE_PER_GROUP: u32 = 100;
const ROLL_MAX_SIDES: u32 = 1000;

/// Parse `/roll [NdM ...]` from the composer text.
/// `/roll` alone defaults to a single d20.
pub(crate) fn parse_roll_command(input: &str) -> Option<RollParse> {
    let rest = input.trim().strip_prefix("/roll")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let args = rest.trim();
    if args.is_empty() {
        return Some(RollParse::Specs(vec![DieSpec {
            count: 1,
            sides: 20,
        }]));
    }
    let mut specs = Vec::new();
    for token in args.split_whitespace() {
        let Some(spec) = parse_die_spec(token) else {
            return Some(RollParse::Invalid);
        };
        specs.push(spec);
    }
    Some(RollParse::Specs(specs))
}

fn parse_die_spec(token: &str) -> Option<DieSpec> {
    let (count_part, sides_part) = token.split_once('d')?;
    let count = if count_part.is_empty() {
        1
    } else {
        count_part.parse::<u32>().ok()?
    };
    let sides = sides_part.parse::<u32>().ok()?;
    if count == 0 || count > ROLL_MAX_DICE_PER_GROUP || !(2..=ROLL_MAX_SIDES).contains(&sides) {
        return None;
    }
    Some(DieSpec { count, sides })
}

pub(crate) fn roll_dice<R: RngCore>(specs: &[DieSpec], rng: &mut R) -> Vec<Vec<u32>> {
    specs
        .iter()
        .map(|spec| {
            (0..spec.count)
                .map(|_| (rng.next_u32() % spec.sides) + 1)
                .collect()
        })
        .collect()
}

pub(crate) fn format_formula(specs: &[DieSpec]) -> String {
    specs
        .iter()
        .map(|s| {
            if s.count == 1 {
                format!("d{}", s.sides)
            } else {
                format!("{}d{}", s.count, s.sides)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn format_roll_result(specs: &[DieSpec], rolls: &[Vec<u32>]) -> String {
    let formula = format_formula(specs);
    let groups = rolls
        .iter()
        .map(|group| {
            let inner = group
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            format!("[{inner}]")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let total: u32 = rolls.iter().flatten().sum();
    format!("{formula}: {groups} = {total}")
}

fn room_slug_for(rooms: &[(ChatRoom, Vec<ChatMessage>)], room_id: Uuid) -> Option<String> {
    rooms
        .iter()
        .find(|(room, _)| room.id == room_id)
        .and_then(|(room, _)| room.slug.clone())
}

/// Parse `/brb [optional message]` from the composer.
/// Returns `Some(message)` where message is empty if no custom text was given.
fn parse_brb_command(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed == "/brb" {
        return Some(String::new());
    }
    let rest = trimmed.strip_prefix("/brb ")?.trim();
    Some(rest.to_string())
}

/// Minimum characters of report text, so `/bug lol` bounces with usage help
/// instead of posting a useless card.
const REPORT_MIN_CHARS: usize = 10;

/// Parse `/bug <text>` or `/suggest <text>` from the composer. Outer `None`
/// means not a report command; inner `None` means missing or too-short text
/// (show usage).
fn parse_report_command(input: &str) -> Option<(ReportKind, Option<String>)> {
    let trimmed = input.trim();
    for kind in [ReportKind::Bug, ReportKind::Suggestion] {
        if trimmed == kind.command() {
            return Some((kind, None));
        }
        if let Some(rest) = trimmed.strip_prefix(kind.command())
            && let Some(rest) = rest.strip_prefix(' ')
        {
            let text = rest.trim();
            let text = (text.chars().count() >= REPORT_MIN_CHARS).then(|| text.to_string());
            return Some((kind, text));
        }
    }
    None
}

/// Which cup the user asked for. Coffee gets the mug-with-handle silhouette
/// (`c[_]`), tea gets the handle-less cup (`\_/`); steam patterns are shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CupKind {
    Coffee,
    Tea,
}

/// Number of distinct steam patterns in `CUP_STEAM_VARIANTS`. Cycled per
/// invocation via `ChatState::next_cup_variant` so rapid back-to-back
/// rituals don't all look identical.
pub(crate) const CUP_VARIANT_COUNT: u8 = 4;

const CUP_STEAM_VARIANTS: &[&str] = &[
    "  )  )\n ( ( (",
    "   ) )\n  ( ( (",
    "  ) ) (\n   ( )",
    "    )\n   ( )\n  ) ( (",
];

/// Parse `/coffee` or `/tea` (case-insensitive, no arguments) from the
/// composer body. Returns `None` for anything else, including arguments
/// like `/coffee please` so the unknown-command handler can still flag
/// typos. Same shape as [`parse_petname_command`].
pub(crate) fn parse_cup_command(input: &str) -> Option<CupKind> {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "/coffee" => Some(CupKind::Coffee),
        "/tea" => Some(CupKind::Tea),
        _ => None,
    }
}

/// Build the multi-line ASCII body for `/coffee` or `/tea`. `variant`
/// selects the steam pattern; out-of-range values wrap via modulo.
pub(crate) fn cup_art(kind: CupKind, variant: u8) -> String {
    let steam = CUP_STEAM_VARIANTS[(variant as usize) % CUP_STEAM_VARIANTS.len()];
    let cup = match kind {
        CupKind::Coffee => "  c[_]",
        CupKind::Tea => "  \\___/",
    };
    format!("{steam}\n{cup}")
}

fn unknown_slash_command(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || !trimmed.starts_with('/') {
        return None;
    }

    let command = trimmed.split_whitespace().next()?;
    if command.len() <= 1 || command == "//" {
        return None;
    }

    Some(command)
}

fn online_username_set(active_users: Option<&ActiveUsers>) -> HashSet<String> {
    let Some(active_users) = active_users else {
        return HashSet::new();
    };
    let guard = active_users.lock_recover();
    guard
        .values()
        .map(|u| u.username.to_ascii_lowercase())
        .collect()
}

pub(crate) fn rank_mention_matches(
    all_usernames: &[String],
    query_lower: &str,
    online_set: impl FnOnce() -> HashSet<String>,
) -> Vec<MentionMatch> {
    // Lowercase each candidate once and keep it paired with the original
    // display name; reused for the prefix filter, the online lookup, and the
    // alphabetical tie-breaker.
    let mut filtered: Vec<(String, String)> = all_usernames
        .iter()
        .filter_map(|name| {
            let lower = name.to_ascii_lowercase();
            lower
                .starts_with(query_lower)
                .then(|| (lower, name.clone()))
        })
        .collect();
    if filtered.is_empty() {
        return Vec::new();
    }

    let online = online_set();
    let mut matches: Vec<(String, MentionMatch)> = filtered
        .drain(..)
        .map(|(lower, name)| {
            let is_online = online.contains(&lower);
            (
                lower,
                MentionMatch {
                    name,
                    online: is_online,
                    prefix: "@",
                    description: None,
                },
            )
        })
        .collect();
    matches.sort_by(|(a_lower, a), (b_lower, b)| {
        b.online.cmp(&a.online).then_with(|| a_lower.cmp(b_lower))
    });
    matches.into_iter().map(|(_, m)| m).collect()
}

pub(crate) fn rank_room_name_matches<'a>(
    rooms: impl IntoIterator<Item = &'a ChatRoom>,
    query_lower: &str,
) -> Vec<MentionMatch> {
    let mut rooms: Vec<(String, String)> = rooms
        .into_iter()
        .filter_map(|room| {
            if room.kind == "dm" {
                return None;
            }
            let name = room.slug.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let lower = name.to_ascii_lowercase();
            lower
                .starts_with(query_lower)
                .then(|| (lower, name.to_string()))
        })
        .collect();
    rooms.sort_by(|(a, _), (b, _)| a.cmp(b));
    rooms.dedup_by(|(a, _), (b, _)| a == b);
    rooms
        .into_iter()
        .map(|(_, name)| MentionMatch {
            name,
            online: true,
            prefix: "#",
            description: None,
        })
        .collect()
}

fn format_active_user_lines(
    active_users: Option<&ActiveUsers>,
    friend_user_ids: &HashSet<Uuid>,
) -> Vec<String> {
    let Some(active_users) = active_users else {
        return vec!["Active user list unavailable".to_string()];
    };

    let guard = active_users.lock_recover();
    if guard.is_empty() {
        return vec!["No active users".to_string()];
    }

    let mut users: Vec<(&Uuid, &ActiveUser)> = guard.iter().collect();
    users.sort_by_key(|(_, user)| user.username.to_ascii_lowercase());
    users
        .into_iter()
        .map(|(user_id, user)| {
            let prefix = if friend_user_ids.contains(user_id) {
                "★ @"
            } else {
                "@"
            };
            if user.connection_count > 1 {
                format!(
                    "{prefix}{} ({} sessions)",
                    user.username, user.connection_count
                )
            } else {
                format!("{prefix}{}", user.username)
            }
        })
        .collect()
}

fn wrapped_index(current: isize, delta: isize, len: usize) -> usize {
    (current + delta).rem_euclid(len as isize) as usize
}

fn adjacent_composer_room(
    order: &[RoomSlot],
    current_room_id: Option<Uuid>,
    delta: isize,
) -> Option<Uuid> {
    let rooms: Vec<Uuid> = order
        .iter()
        .filter_map(|slot| match slot {
            RoomSlot::Room(room_id) => Some(*room_id),
            RoomSlot::Feeds
            | RoomSlot::News
            | RoomSlot::Cyberspace
            | RoomSlot::CyberspaceNotifications
            | RoomSlot::CyberspaceRoom(_)
            | RoomSlot::CyberspaceMail(_)
            | RoomSlot::Notifications
            | RoomSlot::Discover
            | RoomSlot::Showcase
            | RoomSlot::Work => None,
        })
        .collect();
    if rooms.is_empty() {
        return None;
    }

    let current = current_room_id
        .and_then(|room_id| rooms.iter().position(|candidate| *candidate == room_id))
        .unwrap_or(0) as isize;
    Some(rooms[wrapped_index(current, delta, rooms.len())])
}

fn news_modal_source_from_articles(
    articles: &[ArticleFeedItem],
    url: &str,
) -> Option<(NewsPayload, String, chrono::DateTime<chrono::Utc>, Uuid)> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let item = articles
        .iter()
        .find(|item| item.article.url.trim() == url)?;
    Some((
        NewsPayload {
            title: item.article.title.clone(),
            summary: item.article.summary.clone(),
            url: item.article.url.clone(),
            ascii_art: item.article.ascii_art.clone(),
        },
        modal_author_label(Some(&item.author_username), item.article.user_id),
        item.article.created,
        item.article.id,
    ))
}

fn modal_author_label(username: Option<&str>, user_id: Uuid) -> String {
    username
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| short_user_id(user_id))
}

/// Message-search state backing the Ctrl+/ modal's `?` mode. Owned by
/// `ChatState` because it owns the chat event receiver; the modal reads it.
#[derive(Default)]
pub(crate) struct MessageSearch {
    /// In-flight or last-completed request id. Results carrying any other id
    /// are stale and dropped (latest wins).
    request_id: Option<Uuid>,
    /// Query text the current request was fired with; snippets are built
    /// against it when results land.
    query: String,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) hits: Vec<MessageSearchHit>,
    /// Context windows (3 messages either side) keyed by hit message id,
    /// fetched lazily as hits are selected. Bounded in practice: filled only
    /// while the modal is open and reset with `clear()` when it closes.
    pub(crate) context: HashMap<Uuid, MessageContext>,
    /// The one in-flight context fetch: `(request_id, message_id)`. A single
    /// slot, so scrolling through hits fetches sequentially instead of
    /// fanning out one query per keypress.
    context_in_flight: Option<(Uuid, Uuid)>,
}

/// Messages immediately around a search hit, both sides chronological.
#[derive(Default)]
pub(crate) struct MessageContext {
    pub before: Vec<ChatMessage>,
    pub after: Vec<ChatMessage>,
}

impl MessageSearch {
    fn begin(&mut self, request_id: Uuid, query: String) {
        self.request_id = Some(request_id);
        self.query = query;
        self.loading = true;
        self.error = None;
    }

    fn is_current(&self, request_id: Uuid) -> bool {
        self.request_id == Some(request_id)
    }

    fn finish(&mut self, hits: Vec<MessageSearchHit>) {
        self.loading = false;
        self.error = None;
        self.hits = hits;
    }

    fn fail(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

pub(crate) struct MessageSearchHit {
    pub message: ChatMessage,
    /// Precomputed one-row snippet, split around the first case-insensitive
    /// query match so render can highlight it without rescanning the body.
    pub snippet_prefix: String,
    pub snippet_match: String,
    pub snippet_suffix: String,
}

/// Chars of context kept before the match in a snippet.
const SNIPPET_LEAD_CHARS: usize = 24;
/// Total snippet char budget; render truncates further by column width.
const SNIPPET_TOTAL_CHARS: usize = 160;

/// Split a message body into `(prefix, match, suffix)` around the first
/// case-insensitive occurrence of `query`, windowed so the match stays
/// visible in a one-row snippet. Newlines flatten to spaces and a leading
/// `---WORD---` card marker (news/report cards) is dropped so snippets read
/// as text. Falls back to a head-of-body snippet with an empty match part.
pub(crate) fn build_search_snippet(body: &str, query: &str) -> (String, String, String) {
    let flat = strip_card_marker(body).replace(['\n', '\r'], " ");
    let chars: Vec<char> = flat.chars().collect();
    let lower: Vec<char> = flat.to_lowercase().chars().collect();
    let needle: Vec<char> = query.to_lowercase().chars().collect();

    // `to_lowercase` can change char counts for some scripts; if the lowered
    // text no longer lines up with the original, skip highlighting rather
    // than slice at wrong offsets.
    let match_at = if needle.is_empty() || lower.len() != chars.len() {
        None
    } else {
        lower
            .windows(needle.len())
            .position(|window| window == needle.as_slice())
    };

    let Some(start) = match_at else {
        let mut head: String = chars.iter().take(SNIPPET_TOTAL_CHARS).collect();
        if chars.len() > SNIPPET_TOTAL_CHARS {
            head.push('…');
        }
        return (head, String::new(), String::new());
    };

    let window_start = start.saturating_sub(SNIPPET_LEAD_CHARS);
    let match_end = start + needle.len();
    let window_end = chars
        .len()
        .min(match_end + SNIPPET_TOTAL_CHARS.saturating_sub(match_end - window_start));

    let mut prefix: String = chars[window_start..start].iter().collect();
    if window_start > 0 {
        prefix.insert(0, '…');
    }
    let matched: String = chars[start..match_end].iter().collect();
    let mut suffix: String = chars[match_end..window_end].iter().collect();
    if window_end < chars.len() {
        suffix.push('…');
    }
    (prefix, matched, suffix)
}

/// Drop a leading `---WORD--- ` card marker (news/bug/suggestion cards) so
/// search snippets show the card's text instead of its wire marker.
fn strip_card_marker(body: &str) -> &str {
    let trimmed = body.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return body;
    };
    let Some((word, rest)) = rest.split_once("---") else {
        return body;
    };
    if word.is_empty() || !word.chars().all(|ch| ch.is_ascii_uppercase()) {
        return body;
    }
    rest.trim_start()
}

fn resolve_room_jump_target(targets: &[(u8, RoomSlot)], byte: u8) -> Option<RoomSlot> {
    targets
        .iter()
        .find_map(|(key, slot)| (*key == byte).then_some(*slot))
}

/// Set or clear a per-author context value, returning whether the map
/// actually changed (so callers bump the context epoch only on real updates).
/// Blank values clear, matching the service payload convention.
fn set_context_value(
    target: &mut HashMap<Uuid, String>,
    user_id: Uuid,
    value: Option<&str>,
) -> bool {
    match value.filter(|value| !value.trim().is_empty()) {
        Some(value) => match target.get(&user_id) {
            Some(existing) if existing == value => false,
            _ => {
                target.insert(user_id, value.to_string());
                true
            }
        },
        None => target.remove(&user_id).is_some(),
    }
}

/// Merge `incoming` into `target`, returning whether anything actually
/// changed.
fn extend_changed<K, V>(
    target: &mut HashMap<K, V>,
    incoming: impl IntoIterator<Item = (K, V)>,
) -> bool
where
    K: Eq + std::hash::Hash,
    V: PartialEq,
{
    let mut changed = false;
    for (key, value) in incoming {
        match target.get(&key) {
            Some(existing) if *existing == value => {}
            _ => {
                target.insert(key, value);
                changed = true;
            }
        }
    }
    changed
}

/// Parse `/<command>` or `/<command> [@]username`. Returns:
/// - `None` if `input` is not the given command,
/// - `Some(None)` for the bare command (caller treats as "list"),
/// - `Some(Some(username))` for the targeted form.
fn parse_user_command<'a>(input: &'a str, command: &str) -> Option<Option<&'a str>> {
    let rest = input.strip_prefix(command)?;
    let rest = match rest.chars().next() {
        None => return Some(None),
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        return Some(None);
    }
    let username = rest.strip_prefix('@').unwrap_or(rest).trim();
    Some((!username.is_empty()).then_some(username))
}

pub(crate) struct RoomBanRequest<'a> {
    pub username: &'a str,
    pub duration: Option<chrono::Duration>,
    pub reason: String,
}

/// `/ban @user [duration] [reason...]`, and `/unban @user [reason...]`. The
/// duration is only read from the slot right after the username and uses the
/// same `s/m/h/d` syntax the mod surface takes, so there is one place to look
/// for what a duration means. A word there that is not a duration starts the
/// reason instead. Returns `None` when the body is not this command at all,
/// and `Err(usage)` when it is but the arguments are unusable.
fn parse_room_ban_command<'a>(
    input: &'a str,
    command: &str,
) -> Option<Result<RoomBanRequest<'a>, &'static str>> {
    let usage = "Usage: /ban @user [duration] [reason]";
    let rest = input.strip_prefix(command)?;
    let rest = match rest.chars().next() {
        None => "",
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    let mut parts = rest.split_whitespace();
    let Some(username) = parts.next() else {
        return Some(Err(usage));
    };
    let username = username.strip_prefix('@').unwrap_or(username);
    if username.is_empty() {
        return Some(Err(usage));
    }
    let rest: Vec<&str> = parts.collect();
    let (duration, reason_from) = match parse_optional_duration(rest.first().copied(), 0) {
        Ok(parsed) => parsed,
        Err(_) => return Some(Err("Duration must be positive, like 30m or 7d")),
    };
    Some(Ok(RoomBanRequest {
        username,
        duration,
        reason: rest[reason_from..].join(" "),
    }))
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

/// The window a bare `/summary` opens: from when you last left the app on
/// this device, or the default when the device has no mark. The room's AFK
/// line is deliberately not an input; see [`ChatState::device_left_at`].
fn catch_up_window(device_left_at: Option<DateTime<Utc>>) -> SummaryWindow {
    match device_left_at {
        Some(left_at) => SummaryWindow::SinceLeftApp(left_at),
        None => SummaryWindow::Default,
    }
}

/// What the text after `/summary` asked for. Every outcome is named so the
/// command's match answers each one with its own banner: a typo must never
/// silently become the default window.
#[derive(Debug, PartialEq, Eq)]
enum SummaryArg {
    /// Bare `/summary`: catch up from when you last left the app here.
    CatchUp,
    Window(chrono::Duration),
    /// Not a `<count><unit>` duration at all.
    Unparseable,
    /// Parsed, but empty (`0h`): there is no window to read.
    TooShort,
    /// Parsed, but past [`SUMMARY_MAX_WINDOW_HOURS`]. Refused rather than
    /// clamped, so the answer is never narrower than the question.
    TooLong,
}

/// Parse the argument of `/summary [<count>h|<count>m]`.
///
/// The unit is required (`6h`, `90m`): a bare `6` could mean either, and
/// guessing for the user is how a catch-up quietly covers the wrong day.
fn parse_summary_arg(rest: &str) -> SummaryArg {
    let arg = rest.trim().to_ascii_lowercase();
    if arg.is_empty() {
        return SummaryArg::CatchUp;
    }
    let (count, minutes_per_unit) = match (arg.strip_suffix('h'), arg.strip_suffix('m')) {
        (Some(count), _) => (count, 60),
        (None, Some(count)) => (count, 1),
        (None, None) => return SummaryArg::Unparseable,
    };
    // `u32` keeps the multiply below inside i64 whatever is typed, and
    // refuses the sign and decimal point along with the rest of the junk.
    let Ok(count) = count.parse::<u32>() else {
        return SummaryArg::Unparseable;
    };
    match i64::from(count) * minutes_per_unit {
        0 => SummaryArg::TooShort,
        minutes if minutes > crate::app::ai::summary::SUMMARY_MAX_WINDOW_HOURS * 60 => {
            SummaryArg::TooLong
        }
        minutes => SummaryArg::Window(chrono::Duration::minutes(minutes)),
    }
}

fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Compact retry hint for the bot cooldown banner. Minutes round up so the
/// banner never promises an earlier retry than the ladder allows.
fn format_cooldown(remaining: Duration) -> String {
    let secs = remaining.as_secs().max(1);
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{} min", secs.div_ceil(60))
    }
}

/// Given a message list containing `current`, return the id of the message
/// that should take over the selection when `current` is deleted: prefer the
/// next index (older message, since the list is ordered newest-first), fall
/// back to the previous index if `current` was the last item, or `None` if
/// `current` is not in the list.
fn adjacent_message_id(msgs: &[ChatMessage], current: Uuid) -> Option<Uuid> {
    let idx = msgs.iter().position(|m| m.id == current)?;
    msgs.get(idx + 1)
        .map(|m| m.id)
        .or_else(|| idx.checked_sub(1).and_then(|i| msgs.get(i).map(|m| m.id)))
}

fn loaded_reply_target_id(msgs: &[ChatMessage], selected_id: Uuid) -> Option<Option<Uuid>> {
    let selected = msgs.iter().find(|m| m.id == selected_id)?;
    let reply_to_message_id = selected.reply_to_message_id?;
    Some(
        msgs.iter()
            .any(|m| m.id == reply_to_message_id)
            .then_some(reply_to_message_id),
    )
}

fn reply_preview_text(body: &str) -> String {
    if let Some(title) = news_reply_preview_text(body) {
        return title;
    }

    if let Some((kind, text)) = parse_report_payload(body) {
        let first_line = text.lines().find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        return truncate_reply_preview(&format!(
            "{} {}",
            kind.icon(),
            first_line.unwrap_or(kind.command())
        ));
    }

    let body_without_reply_quote = match body.split_once('\n') {
        Some((first_line, rest))
            if first_line.trim().starts_with("> ") && !rest.trim().is_empty() =>
        {
            rest
        }
        _ => body,
    };

    let first_content_line = body_without_reply_quote
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .unwrap_or("");
    let preview = strip_markdown_preview_markers(
        first_content_line
            .strip_prefix("> ")
            .unwrap_or(first_content_line)
            .trim(),
    );
    truncate_reply_preview(&preview)
}

pub(crate) fn new_chat_textarea() -> TextArea<'static> {
    composer::new_themed_textarea("Type a message...", WrapMode::Word, false)
}

/// Number of characters in `text` whose display cells all sit left of
/// `target_col`, i.e. the char index at the start of the glyph under a click.
/// Mirrors ratatui-textarea's own screen→char mapping so wide glyphs (CJK,
/// emoji) line up with the rendered cursor.
fn char_offset_for_display_col(text: &str, target_col: usize) -> usize {
    use unicode_width::UnicodeWidthChar;
    let mut col = 0usize;
    let mut chars = 0usize;
    for c in text.chars() {
        if col >= target_col {
            break;
        }
        let width = c.width().unwrap_or(0);
        if col + width > target_col {
            break;
        }
        col += width;
        chars += 1;
    }
    chars
}

/// Translate a global character offset — newlines counted as one char each,
/// matching `build_composer_rows` — into a logical `(line, column)` pair for
/// `CursorMove::Jump`.
fn global_char_to_line_col(text: &str, target: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for c in text.chars().take(target) {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn news_reply_preview_text(body: &str) -> Option<String> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with(NEWS_MARKER) {
        return None;
    }

    let raw = trimmed[NEWS_MARKER.len()..].trim_start();
    let title = raw
        .split(" || ")
        .next()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("news update");

    Some(truncate_reply_preview(title))
}

fn truncate_reply_preview(text: &str) -> String {
    let preview: String = text.chars().take(48).collect();
    if preview.chars().count() == 48 {
        format!("{}...", preview.trim_end())
    } else {
        preview
    }
}

fn strip_markdown_preview_markers(text: &str) -> String {
    let mut text = text.trim();

    if let Some(rest) = text.strip_prefix("> ") {
        text = rest.trim();
    }
    if let Some(rest) = text.strip_prefix("- ") {
        text = rest.trim();
    }

    let heading_level = text.chars().take_while(|ch| *ch == '#').count();
    if (1..=3).contains(&heading_level)
        && let Some(rest) = text[heading_level..].strip_prefix(' ')
    {
        text = rest.trim();
    }

    let digits = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0
        && let Some(rest) = text[digits..].strip_prefix(". ")
    {
        text = rest.trim();
    }

    let mut out = String::new();
    let mut idx = 0;
    while idx < text.len() {
        let rest = &text[idx..];

        if let Some(marker_len) = leading_backtick_run_len(rest) {
            let marker = &rest[..marker_len];
            let after_open = &rest[marker_len..];
            if let Some(end_rel) = after_open.find(marker)
                && end_rel > 0
            {
                out.push_str(&after_open[..end_rel]);
                idx += marker_len + end_rel + marker_len;
                continue;
            }
        }

        if rest.starts_with('[')
            && let Some(bracket_pos) = rest[1..].find(']')
            && bracket_pos > 0
            && let Some(paren_inner) = rest[1 + bracket_pos + 1..].strip_prefix('(')
            && let Some(close_paren) = paren_inner.find(')')
            && close_paren > 0
        {
            out.push_str(&rest[1..1 + bracket_pos]);
            idx += 1 + bracket_pos + 2 + close_paren + 1;
            continue;
        }

        let mut stripped_marker = false;
        for marker in ["***", "**", "~~", "*"] {
            if rest.starts_with(marker) {
                idx += marker.len();
                stripped_marker = true;
                break;
            }
        }
        if stripped_marker {
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        out.push(ch);
        idx += ch.len_utf8();
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn leading_backtick_run_len(text: &str) -> Option<usize> {
    let len = text.chars().take_while(|ch| *ch == '`').count();
    (len > 0).then_some(len)
}
#[cfg(test)]
#[path = "state_internal_test.rs"]
mod state_internal_test;
