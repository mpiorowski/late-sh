use anyhow::Context;
use chrono::{DateTime, Utc};
use crossterm::{
    cursor,
    terminal::{self, ClearType},
};
use late_core::{MutexRecover, api_types::NowPlaying, audio::VizFrame};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};
use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use late_core::models::leaderboard::LeaderboardData;
use late_core::models::profile::Profile;

use crate::{
    app::activity::{
        channel::ACTIVITY_HISTORY_MAX_EVENTS, event::ActivityEvent, filter::ActivityFilter,
    },
    app::audio::{client_state::ClientAudioState, viz::Visualizer},
    app::files::terminal_image::{
        TerminalImageProtocol, TerminalImageRenderState, iterm2_capabilities_probe,
        kitty_cleanup_commands, protocol_from_env_hint, protocol_from_term,
        protocol_from_terminal_features, protocol_from_xtversion, term_disables_terminal_images,
        terminal_image_cleanup_commands, terminal_string_terminator,
    },
    app::{
        chat,
        chat::news::svc::ArticleService,
        chat::notifications::svc::NotificationService,
        chat::svc::ChatService,
        common::primitives::{Banner, Screen},
        help_modal, hub, mod_modal, profile,
        profile::svc::ProfileService,
        profile_modal, settings_modal, vote,
        vote::svc::{Genre, VoteService},
    },
    authz::Permissions,
    paired_clients::{PairControlMessage, PairedClientRegistry},
    session::{SessionMessage, SessionRegistry},
    state::ActiveUsers,
    web::WebChatRegistry,
};

/// Which desktop-notification OSC sequence(s) to emit. Chosen by the user
/// in profile settings; stored as a string key and mapped here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotificationMode {
    Both,
    Osc777,
    Osc9,
}

pub(crate) const GAME_SELECTION_2048: usize = 0;
pub(crate) const GAME_SELECTION_TETRIS: usize = 1;
pub(crate) const GAME_SELECTION_SUDOKU: usize = 2;
pub(crate) const GAME_SELECTION_NONOGRAMS: usize = 3;
pub(crate) const GAME_SELECTION_MINESWEEPER: usize = 4;
pub(crate) const GAME_SELECTION_SOLITAIRE: usize = 5;
pub(crate) const GAME_SELECTION_SNAKE: usize = 6;
pub(crate) const GAME_SELECTION_NES_SQUIRREL_DOMINO: usize = 7;
pub(crate) const GAME_SELECTION_NES_THWAITE: usize = 8;
pub(crate) const GAME_SELECTION_NES_DABG: usize = 9;
pub(crate) const GAME_SELECTION_NES_FALLING: usize = 10;
pub(crate) const GAME_SELECTION_NES_BRICK_BREAKER: usize = 11;
pub(crate) const GAME_SELECTION_NES_ESCAPE_FROM_PONG: usize = 12;
pub(crate) const GAME_SELECTION_NES_RHDE: usize = 13;
pub(crate) const GAME_SELECTION_NES_CONCENTRATION_ROOM: usize = 14;
pub(crate) const GAME_SELECTION_NES_ZAP_RUDER: usize = 15;
pub(crate) const GAME_SELECTION_NES_2048: usize = 16;
pub(crate) const DEFAULT_GAME_SELECTION: usize = GAME_SELECTION_2048;

fn aquarium_area_for_terminal(cols: u16, rows: u16) -> Rect {
    let app_inner = Rect::new(1, 1, cols.saturating_sub(2), rows.saturating_sub(2));
    crate::app::hub::aquarium::ui::bottom_tray_area(app_inner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashboardGameToggleTarget {
    Arcade,
    Room,
}

impl NotificationMode {
    /// Map the `notify_format` profile field to a concrete mode. Unknown
    /// or missing values fall back to `Both`, matching the on-read
    /// default in `late_core::models::user::extract_notify_format`.
    pub(crate) fn from_format(format: Option<&str>) -> Self {
        match format.unwrap_or("both") {
            "osc777" => Self::Osc777,
            "osc9" => Self::Osc9,
            _ => Self::Both,
        }
    }
}

fn seed_activity_from_history(
    mut activity: VecDeque<ActivityEvent>,
    activity_feed_rx: Option<&mut broadcast::Receiver<ActivityEvent>>,
) -> VecDeque<ActivityEvent> {
    let Some(rx) = activity_feed_rx else {
        return activity;
    };
    let newest_seed_at = activity.back().map(|event| event.at);
    let activity_filter = ActivityFilter::dashboard();

    while let Ok(event) = rx.try_recv() {
        if newest_seed_at.is_some_and(|at| event.at <= at) {
            continue;
        }
        if !activity_filter.includes(&event) {
            continue;
        }
        activity.push_back(event);
        while activity.len() > ACTIVITY_HISTORY_MAX_EVENTS {
            activity.pop_front();
        }
    }

    activity
}

fn seed_room_joins_from_history(
    mut joins: VecDeque<crate::app::dashboard::state::DashboardRoomJoin>,
    room_join_rx: Option<&mut crate::app::dashboard::state::DashboardRoomJoinReceiver>,
) -> VecDeque<crate::app::dashboard::state::DashboardRoomJoin> {
    let Some(rx) = room_join_rx else {
        return joins;
    };

    while let Ok(join) = rx.try_recv() {
        crate::app::dashboard::state::push_recent_room_join(&mut joins, join);
    }

    joins
}

const CURSOR_SHAPE_STEADY_BLOCK: &[u8] = b"\x1b[2 q";
const CURSOR_SHAPE_STEADY_UNDERLINE: &[u8] = b"\x1b[4 q";

#[derive(Clone, Default)]
pub(super) struct SharedBuffer {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SharedBuffer {
    pub(super) fn take(&self) -> Vec<u8> {
        let mut guard = self.inner.lock_recover();
        std::mem::take(&mut *guard)
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self.inner.lock_recover();
        guard.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Passed to App::new() to configure the app on startup
pub struct SessionConfig {
    /// Terminal / layout
    pub cols: u16,
    pub rows: u16,
    pub term: String,

    /// Services / data sources
    pub audio_service: crate::app::audio::svc::AudioService,
    pub vote_service: VoteService,
    pub chat_service: ChatService,
    pub notification_service: NotificationService,
    pub article_service: ArticleService,
    pub feed_service: crate::app::chat::feeds::svc::FeedService,
    pub showcase_service: crate::app::chat::showcase::svc::ShowcaseService,
    pub work_service: crate::app::chat::work::svc::WorkService,
    pub profile_service: ProfileService,
    pub twenty_forty_eight_service:
        crate::app::arcade::twenty_forty_eight::svc::TwentyFortyEightService,
    pub initial_2048_game: Option<late_core::models::twenty_forty_eight::Game>,
    pub initial_2048_high_score: Option<late_core::models::twenty_forty_eight::HighScore>,
    pub tetris_service: crate::app::arcade::tetris::svc::TetrisService,
    pub snake_service: crate::app::arcade::snake::svc::SnakeService,
    pub initial_tetris_game: Option<late_core::models::tetris::Game>,
    pub initial_snake_game: Option<late_core::models::snake::Game>,
    pub initial_tetris_high_score: Option<late_core::models::tetris::HighScore>,
    pub initial_snake_high_score: Option<late_core::models::snake::HighScore>,
    pub sudoku_service: crate::app::arcade::sudoku::svc::SudokuService,
    pub initial_sudoku_games: Vec<late_core::models::sudoku::Game>,
    pub nonogram_service: crate::app::arcade::nonogram::svc::NonogramService,
    pub initial_nonogram_games: Vec<late_core::models::nonogram::Game>,
    pub solitaire_service: crate::app::arcade::solitaire::svc::SolitaireService,
    pub initial_solitaire_games: Vec<late_core::models::solitaire::Game>,
    pub minesweeper_service: crate::app::arcade::minesweeper::svc::MinesweeperService,
    pub initial_minesweeper_games: Vec<late_core::models::minesweeper::Game>,
    pub rooms_service: crate::app::rooms::svc::RoomsService,
    pub room_game_registry: crate::app::rooms::registry::RoomGameRegistry,
    /// Shared in-proc dartboard server handle. Each session only connects — consuming a
    /// color slot and showing up in `peer_count` — when the user actually
    /// enters the dartboard game from the arcade.
    pub dartboard_server: dartboard_local::ServerHandle,
    pub dartboard_provenance: crate::app::artboard::provenance::SharedArtboardProvenance,
    pub artboard_snapshot_service: crate::app::artboard::svc::ArtboardSnapshotService,
    pub pinstar_registry: crate::app::pinstar::svc::PinstarServerRegistry,
    pub username: String,
    pub bonsai_service: crate::app::bonsai::svc::BonsaiService,
    pub initial_bonsai_tree: Option<late_core::models::bonsai::Tree>,
    pub initial_bonsai_care: Option<late_core::models::bonsai::DailyCare>,
    pub pet_service: crate::app::pet::svc::PetService,
    pub initial_pet: Option<late_core::models::pet::PetCompanion>,
    pub quest_service: crate::app::hub::dailies::svc::QuestService,
    pub quest_snapshot_rx:
        tokio::sync::watch::Receiver<crate::app::hub::dailies::svc::QuestSnapshot>,
    pub shop_service: crate::app::hub::shop::svc::ShopService,
    pub shop_snapshot_rx: tokio::sync::watch::Receiver<crate::app::hub::shop::svc::ShopSnapshot>,
    pub ultimate_service: crate::app::ultimates::UltimateService,
    pub initial_ultimate_cooldowns: Vec<late_core::models::ultimate_cooldown::UltimateCooldown>,
    pub nonogram_library: crate::app::arcade::nonogram::state::Library,
    pub initial_chip_balance: i64,

    /// Session / connection
    pub web_url: String,
    pub session_token: String,
    pub session_registry: Option<SessionRegistry>,
    pub paired_client_registry: Option<PairedClientRegistry>,
    pub web_chat_registry: Option<WebChatRegistry>,
    pub session_rx: Option<tokio::sync::mpsc::Receiver<SessionMessage>>,
    pub now_playing_rx: Option<tokio::sync::watch::Receiver<Option<NowPlaying>>>,
    pub active_users: Option<ActiveUsers>,
    pub username_directory: Option<crate::usernames::UsernameDirectory>,
    pub activity_feed_rx: Option<broadcast::Receiver<ActivityEvent>>,
    pub initial_activity: VecDeque<ActivityEvent>,
    pub room_join_rx: Option<crate::app::dashboard::state::DashboardRoomJoinReceiver>,
    pub initial_room_joins: VecDeque<crate::app::dashboard::state::DashboardRoomJoin>,
    pub user_id: Uuid,
    pub permissions: Permissions,
    pub artboard_banned: bool,
    pub artboard_ban_expires_at: Option<DateTime<Utc>>,

    /// Voting
    pub my_vote: Option<Genre>,

    /// Leaderboard
    pub leaderboard_rx: Option<watch::Receiver<Arc<LeaderboardData>>>,

    /// UI flags
    pub is_new_user: bool,

    /// Display config
    pub initial_theme_id: String,
    /// Initial audio source for the paired browser, loaded from
    /// `users.settings.audio_source` (default `Icecast`). v+x mutates this and
    /// persists the new value.
    pub initial_audio_source: late_core::models::user::AudioSource,

    /// Server state
    pub is_draining: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Main application state
pub struct App {
    /// Lifecycle
    pub(crate) running: bool,

    /// UI state
    pub(super) size: (u16, u16),
    pub(crate) screen: Screen,
    pub(super) banner: Option<Banner>,
    pub(crate) show_settings: bool,
    pub(crate) show_splash: bool,
    pub(crate) splash_ticks: usize,
    pub(crate) splash_hint: String,
    pub(crate) show_quit_confirm: bool,
    pub(crate) show_help: bool,
    pub(crate) show_mod_modal: bool,
    pub(crate) show_hub_modal: bool,
    pub(crate) show_aquarium_tray: bool,
    pub(crate) show_profile_modal: bool,
    pub(crate) show_bonsai_modal: bool,
    pub(crate) show_terminal_help: bool,
    pub(crate) show_ultimate_modal: bool,
    pub(crate) help_modal_state: help_modal::state::HelpModalState,
    pub(crate) hub_state: hub::state::HubState,
    pub(crate) aquarium_state: hub::aquarium::state::AquariumState,
    pub(crate) terminal_help_modal_state:
        crate::app::terminal_help_modal::state::TerminalHelpModalState,
    pub(crate) mod_modal_state: mod_modal::state::ModModalState,
    pub(crate) pending_escape: bool,
    pub(crate) pending_escape_started_at: Option<Instant>,
    pub(crate) vt_input: crate::app::input::VtInputParser,

    /// Terminal / rendering
    pub(super) terminal: Terminal<CrosstermBackend<SharedBuffer>>,
    pub(super) shared: SharedBuffer,
    pub(super) visualizer: Visualizer,
    pub(super) viz_frame_buffer: VecDeque<VizFrame>,
    pub(super) last_viz_frame_at: Option<Instant>,

    /// Session / connection
    pub(super) connect_url: String,
    pub(super) session_registry: Option<SessionRegistry>,
    pub(super) paired_client_registry: Option<PairedClientRegistry>,
    pub(super) web_chat_registry: Option<WebChatRegistry>,
    pub(crate) show_web_chat_qr: bool,
    pub(crate) web_chat_qr_url: Option<String>,
    pub(crate) show_pair_modal: bool,
    pub(crate) pair_modal_scroll: u16,
    pub(super) session_token: String,
    pub(super) session_rx: Option<tokio::sync::mpsc::Receiver<SessionMessage>>,
    pub(super) now_playing_rx: Option<tokio::sync::watch::Receiver<Option<NowPlaying>>>,
    pub(super) active_users: Option<ActiveUsers>,
    pub(super) username_directory: Option<crate::usernames::UsernameDirectory>,
    pub(super) activity_feed_rx: Option<broadcast::Receiver<ActivityEvent>>,
    pub(super) room_join_rx: Option<crate::app::dashboard::state::DashboardRoomJoinReceiver>,
    pub(super) activity: VecDeque<ActivityEvent>,
    /// Mouse-wheel scroll offset for the Home top-strip Activity panel. `0`
    /// keeps the newest event at the top (default); larger values scroll
    /// back through older events. Capped at `activity.len()` each frame so
    /// trimming the buffer can't strand the user past the end.
    pub(crate) dashboard_activity_scroll: u16,
    /// Last-rendered rect for the Home top-strip Activity panel. Set by
    /// `dashboard::ui::draw_box_activity` during draw, consumed by mouse
    /// wheel hit-testing in `app::input`. Reset to `None` at the top of
    /// every frame so a layout change can't leave a stale target behind.
    pub(crate) last_dashboard_activity_rect: std::cell::Cell<Option<Rect>>,
    pub(crate) audio: crate::app::audio::state::AudioState,
    pub(crate) user_id: Uuid,
    pub(crate) permissions: Permissions,
    pub(crate) is_admin: bool,
    pub(crate) is_moderator: bool,
    pub(crate) artboard_banned: bool,
    pub(crate) artboard_ban_expires_at: Option<DateTime<Utc>>,

    /// Voting
    pub(crate) vote: vote::state::VoteState,

    /// Chat
    pub(crate) chat: chat::state::ChatState,
    pub(crate) dashboard_chat_rows_cache: chat::ui::ChatRowsCache,
    pub(crate) active_room_rows_cache: chat::ui::ChatRowsCache,
    pub(crate) rooms_chat_rows_cache: chat::ui::ChatRowsCache,
    pub(crate) room_search_modal_state: crate::app::room_search_modal::state::RoomSearchModalState,
    pub(crate) booth_modal_state: crate::app::audio::booth::state::BoothModalState,
    /// Server-authoritative audio source for the paired playback surface.
    /// Mirrors `users.settings.audio_source`. v+x flips this, persists it to
    /// the DB, and pushes `SetPlaybackSource` to browsers and YouTube-capable
    /// CLI control-plane clients. On browser pair-up the current value is
    /// replayed so a refresh lands in the right mode.
    pub(crate) paired_browser_source: late_core::models::user::AudioSource,

    pub(crate) vote_prefix_armed: bool,
    pub(crate) room_join_prefix_armed: bool,
    pub(crate) room_section_prefix_armed: bool,

    /// AFK state set by /brb command. None = active.
    pub(crate) afk: Option<String>,
    /// True if the paired client was muted by /brb (so we can unmute on return).
    pub(crate) afk_muted: bool,

    /// Profile
    pub(crate) profile_state: profile::state::ProfileState,
    pub(crate) profile_modal_state: profile_modal::state::ProfileModalState,
    pub(crate) settings_modal_state: settings_modal::state::SettingsModalState,

    /// Leaderboard
    pub(super) leaderboard_rx: Option<watch::Receiver<Arc<LeaderboardData>>>,
    pub(crate) leaderboard: Arc<LeaderboardData>,

    /// Bonsai
    pub(crate) bonsai_state: crate::app::bonsai::state::BonsaiState,
    pub(crate) bonsai_care_state: crate::app::bonsai::care::BonsaiCareState,

    /// Cat companion
    pub(crate) pet_state: crate::app::pet::state::PetState,
    pub(crate) show_cat_modal: bool,

    /// Hub Shop
    pub(crate) quest_state: crate::app::hub::dailies::state::QuestState,
    pub(crate) shop_state: crate::app::hub::shop::state::ShopState,
    pub(crate) ultimate_service: crate::app::ultimates::UltimateService,
    pub(crate) ultimate_state: crate::app::ultimates::UltimateState,

    /// Arcade Hub
    pub(crate) game_selection: usize,
    pub(crate) is_playing_game: bool,
    pub(crate) dashboard_game_toggle_target: Option<DashboardGameToggleTarget>,
    pub(crate) rooms_service: crate::app::rooms::svc::RoomsService,
    pub(crate) room_game_registry: crate::app::rooms::registry::RoomGameRegistry,
    pub(crate) rooms_selected_index: usize,
    pub(crate) rooms_active_room: Option<crate::app::rooms::svc::RoomListItem>,
    pub(crate) rooms_last_active_room_id: Option<Uuid>,
    pub(crate) rooms_create_flow: Option<crate::app::rooms::backend::CreateRoomFlow>,
    pub(crate) rooms_filter: crate::app::rooms::filter::RoomsFilter,
    pub(crate) rooms_search_active: bool,
    pub(crate) rooms_search_query: String,
    pub(super) rooms_snapshot_rx:
        tokio::sync::watch::Receiver<crate::app::rooms::svc::RoomsSnapshot>,
    pub(super) rooms_event_rx: tokio::sync::broadcast::Receiver<crate::app::rooms::svc::RoomsEvent>,
    pub(crate) rooms_snapshot: crate::app::rooms::svc::RoomsSnapshot,
    pub(crate) dashboard_room_joins: VecDeque<crate::app::dashboard::state::DashboardRoomJoin>,
    pub(crate) twenty_forty_eight_state: crate::app::arcade::twenty_forty_eight::state::State,
    pub(crate) tetris_state: crate::app::arcade::tetris::state::State,
    pub(crate) snake_state: crate::app::arcade::snake::state::State,
    pub(crate) sudoku_state: crate::app::arcade::sudoku::state::State,
    pub(crate) nonogram_state: crate::app::arcade::nonogram::state::State,
    pub(crate) solitaire_state: crate::app::arcade::solitaire::state::State,
    pub(crate) minesweeper_state: crate::app::arcade::minesweeper::state::State,
    pub(crate) nes_cabinet_state: crate::app::arcade::nes_cabinet::state::State,
    pub(crate) active_room_game: Option<Box<dyn crate::app::rooms::backend::ActiveRoomBackend>>,
    /// `Some` while the user is inside the dartboard game, `None` otherwise.
    /// Constructed on entry (connecting + consuming a color slot) and
    /// dropped on leave (firing `server.disconnect()` via `LocalClient`'s
    /// `Drop` impl). A full SSH-session drop cascades through `App` → this
    /// `Option` → the underlying client, so the seat is released on logout
    /// or connection loss.
    pub(crate) dartboard_state: Option<crate::app::artboard::state::State>,
    /// `true` while the dedicated Artboard screen is in editing mode.
    /// View mode stays connected to the shared board but reserves global
    /// screen hotkeys like `1-4` and `Tab`.
    pub(crate) artboard_interacting: bool,
    /// Pinstar diagram editor state. `Some` while the user is on the Pinstar screen
    /// and has opened a diagram file.
    pub(crate) pinstar_state: Option<crate::app::pinstar::state::PinstarState>,
    /// Diagram browser shown when Pinstar page has no active diagram.
    pub(crate) pinstar_browser: crate::app::pinstar::browser::DiagramBrowser,
    /// Registry for collaborative pinstar servers.
    pub(crate) pinstar_registry: crate::app::pinstar::svc::PinstarServerRegistry,
    pub(crate) pinstar_open_rx: Option<
        tokio::sync::oneshot::Receiver<
            anyhow::Result<crate::app::pinstar::browser::BrowserActionResult>,
        >,
    >,
    pub(crate) pinstar_session_rx: Option<
        tokio::sync::oneshot::Receiver<
            anyhow::Result<(crate::app::pinstar::svc::PinstarService, String)>,
        >,
    >,
    pub(crate) pinstar_list_rx: Option<
        tokio::sync::oneshot::Receiver<
            anyhow::Result<Vec<crate::app::pinstar::browser::DiagramEntry>>,
        >,
    >,
    pub(crate) dartboard_server: dartboard_local::ServerHandle,
    pub(crate) dartboard_provenance: crate::app::artboard::provenance::SharedArtboardProvenance,
    pub(crate) artboard_snapshot_service: crate::app::artboard::svc::ArtboardSnapshotService,
    pub(crate) username: String,

    /// Late Chips balance (loaded on login, updated via leaderboard refresh)
    pub(crate) chip_balance: i64,

    /// Pending OSC 52 clipboard payload (written once, cleared after render)
    pub(crate) pending_clipboard: Option<String>,

    /// Terminal control sequences that should be emitted after the frame diff.
    pub(crate) pending_terminal_commands: Vec<Vec<u8>>,

    pub(crate) terminal_image_protocol: Option<TerminalImageProtocol>,
    pub(crate) terminal_images_disabled: bool,
    pub(crate) terminal_image_render_state: TerminalImageRenderState,

    /// Last time a desktop notification was emitted (shared cooldown).
    pub(crate) last_notify_at: Option<Instant>,

    /// Last background color sent to the terminal via OSC 11 (if any).
    pub(crate) last_terminal_bg: Option<ratatui::style::Color>,

    /// Server state
    pub(crate) is_draining: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Emoji + Nerd Font picker
    pub(crate) icon_picker_open: bool,
    pub(crate) icon_picker_state: super::icon_picker::IconPickerState,
    pub(crate) icon_catalog: Option<super::icon_picker::catalog::IconCatalogData>,
}

impl App {
    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn skip_splash_for_tests(&mut self) {
        self.show_splash = false;
        self.show_settings = false;
        self.show_quit_confirm = false;
        self.show_hub_modal = false;
        self.show_bonsai_modal = false;
        self.show_cat_modal = false;
    }

    fn current_visible_chat_room_id(&self) -> Option<Uuid> {
        match self.screen {
            Screen::Dashboard => self.chat.selected_room_id,
            Screen::Rooms => self
                .rooms_active_room
                .as_ref()
                .map(|room| room.chat_room_id),
            _ => None,
        }
    }

    pub(crate) fn sync_visible_chat_room(&mut self) {
        let visible_room_id = self.current_visible_chat_room_id();
        let changed = self.chat.visible_room_id() != visible_room_id;
        self.chat.set_visible_room_id(visible_room_id);
        if changed && let Some(room_id) = visible_room_id {
            self.chat.mark_room_read(room_id);
            self.chat.request_room_tail(room_id);
        }
    }

    pub fn show_splash_for_tests(&mut self, hint: impl Into<String>) {
        self.show_splash = true;
        self.show_settings = false;
        self.show_quit_confirm = false;
        self.splash_ticks = 1;
        self.splash_hint = hint.into();
    }

    pub fn new(mut config: SessionConfig) -> anyhow::Result<Self> {
        let (cols, rows) = if config.cols == 0 || config.rows == 0 {
            tracing::warn!(
                config.cols,
                config.rows,
                "pty size missing, using 80x24 fallback"
            );
            (80, 24)
        } else {
            (config.cols, config.rows)
        };
        tracing::debug!(cols, rows, "initializing app");

        let activity =
            seed_activity_from_history(config.initial_activity, config.activity_feed_rx.as_mut());
        let dashboard_room_joins =
            seed_room_joins_from_history(config.initial_room_joins, config.room_join_rx.as_mut());

        let shared = SharedBuffer::default();
        let backend = CrosstermBackend::new(shared.clone());
        let viewport = Viewport::Fixed(Rect::new(0, 0, cols, rows));
        let terminal = Terminal::with_options(backend, TerminalOptions { viewport })
            .context("failed to create terminal backend")?;
        let terminal_images_disabled = term_disables_terminal_images(&config.term);
        let terminal_image_protocol = if terminal_images_disabled {
            None
        } else {
            protocol_from_term(&config.term)
        };
        let pending_terminal_commands = Vec::new();

        let twenty_forty_eight_state = if let Some(game) = config.initial_2048_game {
            crate::app::arcade::twenty_forty_eight::state::State::restore(
                config.user_id,
                config.twenty_forty_eight_service.clone(),
                game.score,
                config
                    .initial_2048_high_score
                    .as_ref()
                    .map(|score| score.score)
                    .unwrap_or(0),
                game.grid,
                game.is_game_over,
            )
        } else {
            crate::app::arcade::twenty_forty_eight::state::State::new(
                config.user_id,
                config.twenty_forty_eight_service.clone(),
                config
                    .initial_2048_high_score
                    .as_ref()
                    .map(|score| score.score)
                    .unwrap_or(0),
            )
        };

        let tetris_state = if let Some(game) = config.initial_tetris_game {
            crate::app::arcade::tetris::state::State::restore(
                config.user_id,
                config.tetris_service.clone(),
                config
                    .initial_tetris_high_score
                    .as_ref()
                    .map(|score| score.score)
                    .unwrap_or(0),
                game,
            )
        } else {
            crate::app::arcade::tetris::state::State::new(
                config.user_id,
                config.tetris_service.clone(),
                config
                    .initial_tetris_high_score
                    .as_ref()
                    .map(|score| score.score)
                    .unwrap_or(0),
            )
        };
        let snake_best_score = config
            .initial_snake_high_score
            .as_ref()
            .map(|score| score.score)
            .unwrap_or(0);
        let snake_state = if let Some(game) = config.initial_snake_game {
            crate::app::arcade::snake::state::State::restore(
                config.user_id,
                config.snake_service.clone(),
                snake_best_score,
                25,
                60,
                game,
            )
        } else {
            crate::app::arcade::snake::state::State::new(
                config.user_id,
                config.snake_service.clone(),
                snake_best_score,
                25,
                60,
            )
        };
        let sudoku_state = crate::app::arcade::sudoku::state::State::new(
            config.user_id,
            config.sudoku_service.clone(),
            config.initial_sudoku_games,
        );
        let nonogram_state = crate::app::arcade::nonogram::state::State::new(
            config.user_id,
            config.nonogram_service.clone(),
            config.nonogram_library,
            config.initial_nonogram_games,
        );
        let solitaire_state = crate::app::arcade::solitaire::state::State::new(
            config.user_id,
            config.solitaire_service.clone(),
            config.initial_solitaire_games,
        );
        let minesweeper_state = crate::app::arcade::minesweeper::state::State::new(
            config.user_id,
            config.minesweeper_service.clone(),
            config.initial_minesweeper_games,
        );
        let nes_cabinet_state = crate::app::arcade::nes_cabinet::state::State::new();
        let rooms_snapshot_rx = config.rooms_service.subscribe_snapshot();
        let rooms_snapshot = rooms_snapshot_rx.borrow().clone();
        let rooms_event_rx = config.rooms_service.subscribe_events();
        let dartboard_server = config.dartboard_server.clone();
        let dartboard_provenance = config.dartboard_provenance.clone();
        let artboard_snapshot_service = config.artboard_snapshot_service.clone();
        let username = config.username.clone();

        let bonsai_state = if let Some(tree) = config.initial_bonsai_tree {
            crate::app::bonsai::state::BonsaiState::new(
                config.user_id,
                config.bonsai_service.clone(),
                tree,
            )
        } else {
            // Fallback: create a default dead-ish state (should not happen in practice)
            crate::app::bonsai::state::BonsaiState::new(
                config.user_id,
                config.bonsai_service.clone(),
                late_core::models::bonsai::Tree {
                    id: uuid::Uuid::nil(),
                    created: chrono::Utc::now(),
                    updated: chrono::Utc::now(),
                    user_id: config.user_id,
                    growth_points: 0,
                    last_watered: None,
                    seed: config.user_id.as_u128() as i64,
                    is_alive: true,
                },
            )
        };
        let bonsai_care_state = config
            .initial_bonsai_care
            .map(|care| {
                crate::app::bonsai::care::BonsaiCareState::from_daily(
                    care,
                    bonsai_state.seed,
                    bonsai_state.stage(),
                )
            })
            .unwrap_or_else(|| {
                crate::app::bonsai::care::BonsaiCareState::fallback(
                    chrono::Utc::now().date_naive(),
                    bonsai_state.seed,
                    bonsai_state.stage(),
                )
            });

        let pet_state = if let Some(companion) = config.initial_pet {
            crate::app::pet::state::PetState::new(
                config.user_id,
                config.pet_service.clone(),
                companion,
            )
        } else {
            crate::app::pet::state::PetState::new(
                config.user_id,
                config.pet_service.clone(),
                late_core::models::pet::PetCompanion {
                    id: uuid::Uuid::nil(),
                    created: chrono::Utc::now(),
                    updated: chrono::Utc::now(),
                    user_id: config.user_id,
                    last_fed: None,
                    last_watered: None,
                    last_played: None,
                    last_groomed: None,
                    last_treated: None,
                    adopted_at: None,
                    name: None,
                    species: "cat".to_string(),
                    care_streak_days: 0,
                    care_streak_date: None,
                },
            )
        };
        let quest_state = crate::app::hub::dailies::state::QuestState::new(
            config.user_id,
            config.quest_service.clone(),
            config.quest_snapshot_rx,
        );
        let shop_state = crate::app::hub::shop::state::ShopState::new(
            config.user_id,
            config.shop_service.clone(),
            config.shop_snapshot_rx,
        );
        let aquarium_area = aquarium_area_for_terminal(cols, rows);
        let mut aquarium_state =
            crate::app::hub::aquarium::state::AquariumState::default_for_area(aquarium_area)?;
        aquarium_state.set_active_creatures(&shop_state.active_aquarium_fish());

        let active_users = config.active_users.clone();
        let splash_hint = super::common::splash_tips::choose_splash_hint(config.is_new_user);
        let initial_profile = Profile {
            theme_id: Some(config.initial_theme_id.clone()),
            ..Profile::default()
        };
        let mut settings_modal_state = settings_modal::state::SettingsModalState::new(
            config.profile_service.clone(),
            config.feed_service.clone(),
            config.user_id,
        );
        settings_modal_state.open_from_profile(&initial_profile);
        let mut app = Self {
            running: true,
            size: (cols, rows),
            screen: Screen::Dashboard,
            banner: None,
            show_settings: true,
            show_splash: true,
            splash_ticks: 0,
            splash_hint,
            show_quit_confirm: false,
            show_help: false,
            show_mod_modal: false,
            show_hub_modal: false,
            show_aquarium_tray: false,
            show_profile_modal: false,
            show_bonsai_modal: false,
            show_terminal_help: false,
            show_ultimate_modal: false,
            help_modal_state: help_modal::state::HelpModalState::new(),
            hub_state: hub::state::HubState::new(),
            aquarium_state,
            terminal_help_modal_state:
                crate::app::terminal_help_modal::state::TerminalHelpModalState::new(),
            mod_modal_state: mod_modal::state::ModModalState::new(),
            pending_escape: false,
            pending_escape_started_at: None,
            vt_input: crate::app::input::VtInputParser::default(),
            terminal,
            shared,
            visualizer: Visualizer::new(),
            viz_frame_buffer: VecDeque::new(),
            last_viz_frame_at: None,
            connect_url: format!("{}/{}", config.web_url, config.session_token),
            session_registry: config.session_registry,
            paired_client_registry: config.paired_client_registry,
            web_chat_registry: config.web_chat_registry,
            show_web_chat_qr: false,
            web_chat_qr_url: None,
            show_pair_modal: false,
            pair_modal_scroll: 0,
            session_token: config.session_token.clone(),
            session_rx: config.session_rx,
            now_playing_rx: config.now_playing_rx,
            active_users: active_users.clone(),
            username_directory: config.username_directory,
            activity_feed_rx: config.activity_feed_rx,
            room_join_rx: config.room_join_rx,
            activity,
            dashboard_activity_scroll: 0,
            last_dashboard_activity_rect: std::cell::Cell::new(None),
            audio: crate::app::audio::state::AudioState::new(config.audio_service, config.user_id),
            user_id: config.user_id,
            permissions: config.permissions,
            is_admin: config.permissions.is_admin(),
            is_moderator: config.permissions.is_moderator(),
            artboard_banned: config.artboard_banned,
            artboard_ban_expires_at: config.artboard_ban_expires_at,
            vote: vote::state::VoteState::new(config.vote_service, config.user_id, config.my_vote),
            chat: chat::state::ChatState::new(
                chat::state::ChatServices {
                    chat: config.chat_service,
                    notifications: config.notification_service,
                    articles: config.article_service.clone(),
                    feeds: config.feed_service.clone(),
                    showcases: config.showcase_service.clone(),
                    work: config.work_service.clone(),
                },
                config.user_id,
                config.permissions,
                active_users.clone(),
            ),
            dashboard_chat_rows_cache: chat::ui::ChatRowsCache::default(),
            active_room_rows_cache: chat::ui::ChatRowsCache::default(),
            rooms_chat_rows_cache: chat::ui::ChatRowsCache::default(),
            room_search_modal_state:
                crate::app::room_search_modal::state::RoomSearchModalState::default(),
            booth_modal_state: crate::app::audio::booth::state::BoothModalState::default(),
            paired_browser_source: config.initial_audio_source,
            vote_prefix_armed: false,
            room_join_prefix_armed: false,
            room_section_prefix_armed: false,
            afk: None,
            afk_muted: false,
            profile_state: profile::state::ProfileState::new(
                config.profile_service.clone(),
                config.user_id,
                config.initial_theme_id,
            ),
            profile_modal_state: profile_modal::state::ProfileModalState::new(
                config.profile_service.clone(),
                config.showcase_service.clone(),
            ),
            settings_modal_state,
            leaderboard_rx: config.leaderboard_rx,
            leaderboard: Arc::new(LeaderboardData::default()),
            bonsai_state,
            bonsai_care_state,
            pet_state,
            show_cat_modal: false,
            quest_state,
            shop_state,
            ultimate_service: config.ultimate_service,
            ultimate_state: crate::app::ultimates::UltimateState::with_cooldowns(
                config.initial_ultimate_cooldowns,
            ),
            game_selection: DEFAULT_GAME_SELECTION,
            is_playing_game: false,
            dashboard_game_toggle_target: None,
            rooms_service: config.rooms_service,
            room_game_registry: config.room_game_registry,
            rooms_selected_index: 0,
            rooms_active_room: None,
            rooms_last_active_room_id: None,
            rooms_create_flow: None,
            rooms_filter: crate::app::rooms::filter::RoomsFilter::default(),
            rooms_search_active: false,
            rooms_search_query: String::new(),
            rooms_snapshot_rx,
            rooms_event_rx,
            rooms_snapshot,
            dashboard_room_joins,
            twenty_forty_eight_state,
            tetris_state,
            snake_state,
            sudoku_state,
            nonogram_state,
            solitaire_state,
            minesweeper_state,
            nes_cabinet_state,
            active_room_game: None,
            dartboard_state: None,
            pinstar_state: None,
            pinstar_browser: crate::app::pinstar::browser::DiagramBrowser::default(),
            pinstar_registry: config.pinstar_registry,
            pinstar_open_rx: None,
            pinstar_session_rx: None,
            pinstar_list_rx: None,
            artboard_interacting: false,
            dartboard_server,
            dartboard_provenance,
            artboard_snapshot_service,
            username,
            chip_balance: config.initial_chip_balance,
            pending_clipboard: None,
            pending_terminal_commands,
            terminal_image_protocol,
            terminal_images_disabled,
            terminal_image_render_state: TerminalImageRenderState::default(),
            last_notify_at: None,
            is_draining: config.is_draining,
            icon_picker_open: false,
            icon_picker_state: super::icon_picker::IconPickerState::default(),
            icon_catalog: None,
            last_terminal_bg: None,
        };
        if app.screen == Screen::Artboard {
            app.enter_dartboard();
        }
        app.chat
            .set_favorite_room_ids(app.profile_state.profile().favorite_room_ids.clone());
        app.chat.sync_selection();
        app.sync_visible_chat_room();
        Ok(app)
    }

    /// Connect this session to the shared dartboard and install per-user
    /// state. No-op if already connected (e.g. re-entering the game without
    /// having left). Idempotent so input/render paths can call it without
    /// bookkeeping.
    pub(crate) fn enter_dartboard(&mut self) {
        if self.dartboard_state.is_some() {
            return;
        }
        let svc = crate::app::artboard::svc::DartboardService::new(
            self.dartboard_server.clone(),
            self.user_id,
            &self.username,
            self.dartboard_provenance.clone(),
        );
        self.dartboard_state = Some(crate::app::artboard::state::State::new(
            svc,
            self.artboard_snapshot_service.clone(),
            self.username.clone(),
            self.dartboard_provenance.clone(),
        ));
        self.set_cursor_shape(CURSOR_SHAPE_STEADY_UNDERLINE);
    }

    /// Drop this session's dartboard state. The underlying `LocalClient`'s
    /// `Drop` impl fires `server.disconnect()`, freeing the color slot.
    pub(crate) fn leave_dartboard(&mut self) {
        if self.dartboard_state.is_none() {
            return;
        }
        self.dartboard_state = None;
        self.set_cursor_shape(CURSOR_SHAPE_STEADY_BLOCK);
    }

    pub(crate) fn activate_artboard_interaction(&mut self) -> bool {
        self.expire_artboard_ban_if_needed();
        if self.artboard_banned {
            self.deactivate_artboard_interaction();
            self.banner = Some(Banner::error(
                "Artboard editing is disabled for this account.",
            ));
            return false;
        }
        self.enter_dartboard();
        self.artboard_interacting = true;
        true
    }

    pub(crate) fn deactivate_artboard_interaction(&mut self) {
        self.artboard_interacting = false;
        if let Some(state) = self.dartboard_state.as_mut() {
            state.clear_local_state();
            state.close_help();
            state.close_glyph_picker();
            state.close_snapshot_browser();
        }
    }

    pub(crate) fn enter_pinstar(&mut self) {
        // Pinstar state is lazily initialized when the user opens a file.
        // Refresh the diagram list when entering the screen.
        self.refresh_pinstar_browser();
    }

    pub(crate) fn leave_pinstar(&mut self) {
        if let Some(state) = &mut self.pinstar_state
            && matches!(
                state.mode,
                crate::app::pinstar::state::PinstarMode::Local { .. }
            )
        {
            let _ = state.save();
        }
    }

    pub(crate) fn refresh_pinstar_browser(&mut self) {
        if self.pinstar_list_rx.is_some() {
            return;
        }

        let db = self.pinstar_registry.db();
        let user_id = self.user_id;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pinstar_list_rx = Some(rx);
        self.pinstar_browser.loading = true;

        tokio::spawn(async move {
            if let Some(db) = db {
                let res = crate::app::pinstar::browser::load_diagram_list(&db, user_id).await;
                let _ = tx.send(res);
            } else {
                let _ = tx.send(Ok(Vec::new()));
            }
        });
    }

    pub(crate) fn start_pinstar_session(&mut self, diagram_id: Uuid, role: String) {
        let registry = self.pinstar_registry.clone();
        let user_id = self.user_id;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pinstar_open_rx = None; // clear any existing

        self.banner = Some(Banner::success("Connecting to diagram..."));

        let username = self.username.clone();
        let db = registry.db();

        tokio::spawn(async move {
            let result = async {
                let effective_role = if let Some(db) = db {
                    let client = db
                        .get()
                        .await
                        .context("db client for pinstar access check")?;
                    let Some((_, actual_role)) =
                        late_core::models::pinstar_diagram::PinstarDiagram::get_with_member_role(
                            &client, diagram_id, user_id,
                        )
                        .await?
                    else {
                        anyhow::bail!("you do not have access to this diagram");
                    };
                    actual_role
                } else {
                    role
                };

                let handle = registry.get_or_create(diagram_id).await?;
                let svc = crate::app::pinstar::svc::PinstarService::new(
                    &handle,
                    user_id,
                    &username,
                    effective_role.clone(),
                );
                Ok((svc, effective_role))
            }
            .await;

            match result {
                Ok(session) => {
                    let _ = tx.send(Ok(session));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });

        self.pinstar_session_rx = Some(rx);
    }

    pub(crate) fn set_artboard_banned(&mut self, banned: bool, expires_at: Option<DateTime<Utc>>) {
        let active_ban = banned
            && expires_at
                .map(|expires_at| expires_at > Utc::now())
                .unwrap_or(true);
        let active_expires_at = active_ban.then_some(expires_at).flatten();
        if self.artboard_banned == active_ban && self.artboard_ban_expires_at == active_expires_at {
            return;
        }
        self.artboard_banned = active_ban;
        self.artboard_ban_expires_at = active_expires_at;
        if active_ban {
            self.deactivate_artboard_interaction();
            self.banner = Some(Banner::error(
                "Artboard editing is disabled for this account.",
            ));
        } else {
            self.banner = Some(Banner::success("Artboard editing is enabled again."));
        }
    }

    pub(crate) fn set_permissions(&mut self, permissions: Permissions) {
        if self.permissions == permissions {
            return;
        }
        let was_admin = self.permissions.is_admin();
        let was_moderator = self.permissions.is_moderator();
        self.permissions = permissions;
        self.is_admin = permissions.is_admin();
        self.is_moderator = permissions.is_moderator();
        self.chat.set_permissions(permissions);
        self.banner = Some(Banner::success(&format!(
            "Permissions updated: admin={} moderator={}",
            permissions.is_admin(),
            permissions.is_moderator()
        )));
        if (was_admin || was_moderator) && !permissions.can_access_mod_surface() {
            self.show_mod_modal = false;
            self.pet_state.cancel_play();
            self.show_cat_modal = false;
        }
    }

    pub fn set_artboard_banned_for_tests(&mut self, banned: bool) {
        self.set_artboard_banned(banned, None);
    }

    pub(crate) fn expire_artboard_ban_if_needed(&mut self) {
        if !self.artboard_banned {
            return;
        }
        let Some(expires_at) = self.artboard_ban_expires_at else {
            return;
        };
        if expires_at > Utc::now() {
            return;
        }
        self.artboard_banned = false;
        self.artboard_ban_expires_at = None;
    }

    pub(crate) fn set_screen(&mut self, screen: Screen) {
        if self.screen == screen {
            if screen == Screen::Artboard {
                self.enter_dartboard();
            }
            if screen == Screen::Arcade
                && self.is_playing_game
                && crate::app::arcade::input::is_nes_selection(self.game_selection)
            {
                self.nes_cabinet_state.activate();
            }
            self.sync_visible_chat_room();
            return;
        }

        if self.screen == Screen::Arcade
            && self.is_playing_game
            && crate::app::arcade::input::is_nes_selection(self.game_selection)
        {
            self.nes_cabinet_state.deactivate();
        }

        if self.screen == Screen::Artboard {
            self.deactivate_artboard_interaction();
            self.leave_dartboard();
            self.force_full_repaint();
        }

        if self.screen == Screen::Pinstar {
            self.leave_pinstar();
            self.force_full_repaint();
        }

        self.screen = screen;

        if matches!(self.screen, Screen::Dashboard) {
            self.chat.request_list();
            self.chat.sync_selection();
        }

        if self.screen == Screen::Artboard {
            self.enter_dartboard();
        }
        if self.screen == Screen::Pinstar {
            self.enter_pinstar();
        }
        if self.screen == Screen::Arcade
            && self.is_playing_game
            && crate::app::arcade::input::is_nes_selection(self.game_selection)
        {
            self.nes_cabinet_state.activate();
        }
        self.sync_visible_chat_room();
    }

    fn set_cursor_shape(&mut self, sequence: &[u8]) {
        self.pending_terminal_commands.push(sequence.to_vec());
    }

    pub(crate) fn apply_terminal_env_hint(&mut self, name: &str, value: &str) {
        if self.terminal_images_disabled {
            return;
        }
        if let Some(protocol) = protocol_from_env_hint(name, value) {
            self.terminal_image_protocol = Some(protocol);
        }
    }

    pub(crate) fn apply_xtversion_reply(&mut self, value: &str) {
        if self.terminal_images_disabled {
            return;
        }
        if let Some(protocol) = protocol_from_xtversion(value) {
            self.terminal_image_protocol = Some(protocol);
        }
    }

    pub(crate) fn apply_terminal_capabilities(&mut self, value: &str) {
        if self.terminal_images_disabled {
            return;
        }
        if let Some(protocol) = protocol_from_terminal_features(value) {
            self.terminal_image_protocol = Some(protocol);
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), io::Error> {
        tracing::debug!(cols, rows, "window resized");
        self.size = (cols, rows);
        let aquarium_area = aquarium_area_for_terminal(cols, rows);
        self.aquarium_state
            .handle_resize(aquarium_area.width, aquarium_area.height);
        self.terminal.resize(Rect::new(0, 0, cols, rows))
    }

    pub fn handle_input(&mut self, data: &[u8]) {
        crate::app::input::handle(self, data)
    }

    pub fn toggle_paired_client_mute(&mut self) -> bool {
        let Some(registry) = &self.paired_client_registry else {
            return false;
        };
        registry.send_control(&self.session_token, PairControlMessage::ToggleMute)
    }

    /// Enter AFK mode: store the message, mute paired audio if not already muted.
    pub fn go_afk(&mut self, message: String) {
        let already_muted = self.paired_client_state().is_some_and(|s| s.muted);
        if !already_muted && self.toggle_paired_client_mute() {
            self.afk_muted = true;
        }
        self.afk = Some(message);
    }

    /// Return from AFK: clear AFK state, unmute if we were the one who muted.
    pub fn return_from_afk(&mut self) {
        self.afk = None;
        if self.afk_muted {
            let still_muted = self.paired_client_state().is_some_and(|state| state.muted);
            if still_muted {
                if self.toggle_paired_client_mute() {
                    self.afk_muted = false;
                }
            } else {
                self.afk_muted = false;
            }
        }
    }

    pub fn paired_client_volume_up(&mut self) -> bool {
        let Some(registry) = &self.paired_client_registry else {
            return false;
        };
        registry.send_control(&self.session_token, PairControlMessage::VolumeUp)
    }

    pub fn paired_client_volume_down(&mut self) -> bool {
        let Some(registry) = &self.paired_client_registry else {
            return false;
        };
        registry.send_control(&self.session_token, PairControlMessage::VolumeDown)
    }

    /// Push the currently-stored audio source to all paired entries. Called
    /// when a browser registers so every playback surface reflects the
    /// persisted choice plus the current surface policy: browser Icecast only
    /// when no CLI is paired, and embedded CLI webview only when no real
    /// browser is paired.
    pub fn replay_paired_browser_source(&self) {
        let Some(registry) = self.paired_client_registry.as_ref() else {
            return;
        };
        registry.broadcast_playback_source_for_token(&self.session_token);
    }

    /// Flip the per-user audio source preference. Persisted server-side; the
    /// `persist_audio_source` task then pushes `SetPlaybackSource` to every
    /// paired entry (CLI and browser) for this user. Works whether a browser
    /// is paired or not — the preference is meaningful even with only a CLI,
    /// because the CLI gates its Icecast decoder on the received source.
    pub fn toggle_paired_playback_source(&mut self) -> late_core::models::user::AudioSource {
        use late_core::models::user::AudioSource;
        let next = match self.paired_browser_source {
            AudioSource::Icecast => AudioSource::Youtube,
            AudioSource::Youtube => AudioSource::Icecast,
        };
        self.paired_browser_source = next;
        if let Some(active_users) = &self.active_users
            && let Some(active) = active_users.lock_recover().get_mut(&self.user_id)
        {
            active.audio_source = next;
        }
        self.audio.persist_audio_source(next);
        next
    }

    pub fn request_paired_clipboard_image_upload(&mut self, room_id: Option<Uuid>) -> bool {
        let Some(registry) = &self.paired_client_registry else {
            return false;
        };
        if registry.request_clipboard_image(&self.session_token) {
            self.chat.begin_pending_clipboard_image_upload(room_id);
            return true;
        }
        false
    }

    pub fn paired_client_state(&self) -> Option<ClientAudioState> {
        self.paired_client_registry
            .as_ref()
            .and_then(|registry| registry.snapshot(&self.session_token))
    }

    /// Reset the terminal diff state so the next `render()` emits a full frame.
    /// Used after dropped SSH frames and by integration test helpers.
    pub fn reset_render(&mut self) {
        self.force_full_repaint();
        self.shared.take();
    }

    pub(crate) fn force_full_repaint(&mut self) {
        let mut shared = self.shared.clone();
        let _ = shared.write_all(terminal_string_terminator());
        let _ = self.terminal.clear();
        if self.terminal_image_protocol == Some(TerminalImageProtocol::Kitty) {
            self.pending_terminal_commands
                .extend(kitty_cleanup_commands());
        }
        self.terminal_image_render_state = TerminalImageRenderState::default();
    }

    pub fn enter_alt_screen() -> Vec<u8> {
        let mut buf = Vec::new();
        // If a prior session was killed mid-OSC image payload, recover the
        // terminal parser before sending normal alt-screen setup.
        buf.extend_from_slice(terminal_string_terminator());
        crossterm::execute!(
            buf,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(ClearType::All)
        )
        .expect("failed to enter alt screen");
        for command in terminal_image_cleanup_commands() {
            buf.extend_from_slice(&command);
        }
        // 1000h = basic mouse tracking (button press/release + scroll wheel)
        // 1003h = any-event mouse tracking (motion reports with or without a
        // button held). Dartboard needs drag + hover parity with standalone.
        // 1006h = SGR extended encoding (ESC[< sequences instead of legacy X11)
        // 2004h = bracketed paste mode (ESC[200~ ... ESC[201~)
        buf.extend_from_slice(b"\x1b[?1000h\x1b[?1003h\x1b[?1006h\x1b[?2004h");
        buf.extend_from_slice(&crate::app::files::terminal_image::xtversion_probe());
        buf.extend_from_slice(&iterm2_capabilities_probe());
        buf
    }

    pub fn leave_alt_screen() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(terminal_string_terminator());
        // 2004l = disable bracketed paste
        // 1006l = disable SGR mouse tracking
        // 1003l = disable any-event mouse tracking
        // 1000l = disable basic mouse tracking
        // OSC 111 = reset terminal background color
        buf.extend_from_slice(b"\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?1000l\x1b]111\x1b\\");
        for command in terminal_image_cleanup_commands() {
            buf.extend_from_slice(&command);
        }
        crossterm::execute!(buf, terminal::Clear(ClearType::All))
            .expect("failed to clear terminal before leaving alt screen");
        buf.extend_from_slice(CURSOR_SHAPE_STEADY_BLOCK);
        crossterm::execute!(buf, cursor::Show, terminal::LeaveAlternateScreen)
            .expect("failed to leave alt screen");
        for command in terminal_image_cleanup_commands() {
            buf.extend_from_slice(&command);
        }
        buf
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let Some(registry) = self.session_registry.clone() else {
            return;
        };
        if self.session_token.is_empty() {
            return;
        }
        let token = self.session_token.clone();
        tokio::spawn(async move {
            registry.unregister(&token).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn shared_buffer_write_and_take() {
        let mut buf = SharedBuffer::default();
        buf.write_all(b"hello").unwrap();
        let taken = buf.take();
        assert_eq!(taken, b"hello");
    }

    #[test]
    fn shared_buffer_take_clears() {
        let mut buf = SharedBuffer::default();
        buf.write_all(b"data").unwrap();
        let _ = buf.take();
        assert!(buf.take().is_empty());
    }

    #[test]
    fn shared_buffer_multiple_writes() {
        let mut buf = SharedBuffer::default();
        buf.write_all(b"hello").unwrap();
        buf.write_all(b" world").unwrap();
        assert_eq!(buf.take(), b"hello world");
    }

    #[test]
    fn shared_buffer_flush_succeeds() {
        let mut buf = SharedBuffer::default();
        assert!(buf.flush().is_ok());
    }

    #[test]
    fn shared_buffer_write_returns_correct_len() {
        let mut buf = SharedBuffer::default();
        let written = buf.write(b"test").unwrap();
        assert_eq!(written, 4);
    }

    #[test]
    fn shared_buffer_default_is_empty() {
        let buf = SharedBuffer::default();
        assert!(buf.take().is_empty());
    }

    #[test]
    fn notification_mode_from_format_maps_known_values() {
        assert_eq!(
            NotificationMode::from_format(Some("both")),
            NotificationMode::Both
        );
        assert_eq!(
            NotificationMode::from_format(Some("osc777")),
            NotificationMode::Osc777
        );
        assert_eq!(
            NotificationMode::from_format(Some("osc9")),
            NotificationMode::Osc9
        );
    }

    #[test]
    fn notification_mode_from_format_defaults_to_both() {
        assert_eq!(NotificationMode::from_format(None), NotificationMode::Both);
        assert_eq!(
            NotificationMode::from_format(Some("")),
            NotificationMode::Both
        );
        assert_eq!(
            NotificationMode::from_format(Some("garbage")),
            NotificationMode::Both
        );
    }

    #[test]
    fn seed_activity_from_history_drops_events_already_in_history() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let event = ActivityEvent::joined(uuid::Uuid::nil(), "alice");
        tx.send(event.clone()).expect("send activity");
        let mut history = VecDeque::new();
        history.push_back(event);

        let activity = seed_activity_from_history(history, Some(&mut rx));

        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].username, "alice");
    }

    #[test]
    fn seed_activity_from_history_keeps_events_newer_than_history() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let old = ActivityEvent::joined(uuid::Uuid::nil(), "alice");
        let mut history = VecDeque::new();
        history.push_back(old);
        let mut fresh = ActivityEvent::joined(uuid::Uuid::from_u128(1), "bob");
        fresh.at = history.back().map_or(fresh.at, |event| {
            event.at + std::time::Duration::from_secs(1)
        });
        tx.send(fresh).expect("send activity");

        let activity = seed_activity_from_history(history, Some(&mut rx));

        assert_eq!(activity.len(), 2);
        assert_eq!(activity[0].username, "alice");
        assert_eq!(activity[1].username, "bob");
    }

    #[test]
    fn leave_alt_screen_resets_cursor_shape() {
        let bytes = App::leave_alt_screen();
        assert!(
            bytes
                .windows(CURSOR_SHAPE_STEADY_BLOCK.len())
                .any(|w| w == CURSOR_SHAPE_STEADY_BLOCK),
            "expected steady block cursor reset in shutdown bytes, got: {bytes:?}"
        );
    }

    #[test]
    fn alt_screen_boundaries_recover_terminal_string_state() {
        assert!(App::enter_alt_screen().starts_with(terminal_string_terminator()));
        assert!(App::leave_alt_screen().starts_with(terminal_string_terminator()));
    }

    #[test]
    fn cursor_shape_sequences_match_expected_descusr_codes() {
        assert_eq!(CURSOR_SHAPE_STEADY_BLOCK, b"\x1b[2 q");
        assert_eq!(CURSOR_SHAPE_STEADY_UNDERLINE, b"\x1b[4 q");
    }
}
