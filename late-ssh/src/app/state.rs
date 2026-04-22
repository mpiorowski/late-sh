use anyhow::Context;
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
    app::{
        chat,
        chat::news::svc::ArticleService,
        chat::notifications::svc::NotificationService,
        chat::svc::ChatService,
        common::primitives::{Banner, Screen},
        help_modal, profile,
        profile::svc::ProfileService,
        profile_modal, settings_modal,
        visualizer::Visualizer,
        vote,
        vote::svc::{Genre, VoteService},
    },
    session::{
        ClientAudioState, PairControlMessage, PairedClientRegistry, SessionMessage, SessionRegistry,
    },
    state::{ActiveUsers, ActivityEvent},
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

    /// Services / data sources
    pub vote_service: VoteService,
    pub chat_service: ChatService,
    pub notification_service: NotificationService,
    pub article_service: ArticleService,
    pub profile_service: ProfileService,
    pub twenty_forty_eight_service:
        crate::app::games::twenty_forty_eight::svc::TwentyFortyEightService,
    pub initial_2048_game: Option<late_core::models::twenty_forty_eight::Game>,
    pub initial_2048_high_score: Option<late_core::models::twenty_forty_eight::HighScore>,
    pub tetris_service: crate::app::games::tetris::svc::TetrisService,
    pub initial_tetris_game: Option<late_core::models::tetris::Game>,
    pub initial_tetris_high_score: Option<late_core::models::tetris::HighScore>,
    pub sudoku_service: crate::app::games::sudoku::svc::SudokuService,
    pub initial_sudoku_games: Vec<late_core::models::sudoku::Game>,
    pub nonogram_service: crate::app::games::nonogram::svc::NonogramService,
    pub initial_nonogram_games: Vec<late_core::models::nonogram::Game>,
    pub solitaire_service: crate::app::games::solitaire::svc::SolitaireService,
    pub initial_solitaire_games: Vec<late_core::models::solitaire::Game>,
    pub minesweeper_service: crate::app::games::minesweeper::svc::MinesweeperService,
    pub initial_minesweeper_games: Vec<late_core::models::minesweeper::Game>,
    pub blackjack_service: crate::app::games::blackjack::svc::BlackjackService,
    pub bonsai_service: crate::app::bonsai::svc::BonsaiService,
    pub initial_bonsai_tree: Option<late_core::models::bonsai::Tree>,
    pub nonogram_library: crate::app::games::nonogram::state::Library,
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
    pub activity_feed_rx: Option<broadcast::Receiver<ActivityEvent>>,
    pub user_id: Uuid,
    pub is_admin: bool,

    /// Voting
    pub my_vote: Option<Genre>,

    /// Leaderboard
    pub leaderboard_rx: Option<watch::Receiver<Arc<LeaderboardData>>>,

    /// UI flags
    pub is_new_user: bool,

    /// Display config
    pub initial_theme_id: String,

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
    pub(crate) show_profile_modal: bool,
    pub(crate) help_modal_state: help_modal::state::HelpModalState,
    pub(crate) pending_escape: bool,
    pub(crate) pending_escape_started_at: Option<Instant>,
    pub(crate) vt_input: crate::app::input::VtInputParser,

    /// Terminal / rendering
    pub(super) terminal: Terminal<CrosstermBackend<SharedBuffer>>,
    pub(super) shared: SharedBuffer,
    pub(super) visualizer: Visualizer,
    pub(super) browser_viz_buffer: VecDeque<VizFrame>,
    pub(super) last_browser_viz_at: Option<Instant>,

    /// Session / connection
    pub(super) connect_url: String,
    pub(super) session_registry: Option<SessionRegistry>,
    pub(super) paired_client_registry: Option<PairedClientRegistry>,
    pub(super) web_chat_registry: Option<WebChatRegistry>,
    pub(crate) show_web_chat_qr: bool,
    pub(crate) web_chat_qr_url: Option<String>,
    pub(super) session_token: String,
    pub(super) session_rx: Option<tokio::sync::mpsc::Receiver<SessionMessage>>,
    pub(super) now_playing_rx: Option<tokio::sync::watch::Receiver<Option<NowPlaying>>>,
    pub(super) active_users: Option<ActiveUsers>,
    pub(super) activity_feed_rx: Option<broadcast::Receiver<ActivityEvent>>,
    pub(super) activity: VecDeque<ActivityEvent>,
    pub(crate) user_id: Uuid,
    pub(crate) is_admin: bool,

    /// Voting
    pub(crate) vote: vote::state::VoteState,

    /// Chat
    pub(crate) chat: chat::state::ChatState,
    pub(crate) dashboard_chat_rows_cache: chat::ui::ChatRowsCache,
    pub(crate) active_room_rows_cache: chat::ui::ChatRowsCache,

    /// Which favorite room the dashboard's chat card is currently showing,
    /// when the user has 2+ favorites pinned. Clamped on read against the
    /// current profile list so it stays valid after adds/removes. Session-
    /// local — not persisted.
    pub(crate) dashboard_favorite_index: usize,
    /// Previously-active favorite index, so `,` can jump back to the last
    /// pin Vim-alternate-buffer style. Session-local.
    pub(crate) dashboard_previous_favorite_index: Option<usize>,
    /// `true` while the user has pressed `g` on the dashboard and we're
    /// waiting for a digit to complete a jump (Vim-style two-key prefix).
    /// Any non-digit keystroke disarms and falls through to its normal
    /// handling.
    pub(crate) dashboard_g_prefix_armed: bool,

    /// Profile
    pub(crate) profile_state: profile::state::ProfileState,
    pub(crate) profile_modal_state: profile_modal::state::ProfileModalState,
    pub(crate) settings_modal_state: settings_modal::state::SettingsModalState,

    /// Leaderboard
    pub(super) leaderboard_rx: Option<watch::Receiver<Arc<LeaderboardData>>>,
    pub(crate) leaderboard: Arc<LeaderboardData>,

    /// Bonsai
    pub(crate) bonsai_state: crate::app::bonsai::state::BonsaiState,

    /// Games Hub
    pub(crate) game_selection: usize,
    pub(crate) is_playing_game: bool,
    pub(crate) twenty_forty_eight_state: crate::app::games::twenty_forty_eight::state::State,
    pub(crate) tetris_state: crate::app::games::tetris::state::State,
    pub(crate) sudoku_state: crate::app::games::sudoku::state::State,
    pub(crate) nonogram_state: crate::app::games::nonogram::state::State,
    pub(crate) solitaire_state: crate::app::games::solitaire::state::State,
    pub(crate) minesweeper_state: crate::app::games::minesweeper::state::State,
    pub(crate) blackjack_state: crate::app::games::blackjack::state::State,

    /// Late Chips balance (loaded on login, updated via leaderboard refresh)
    pub(crate) chip_balance: i64,

    /// Pending OSC 52 clipboard payload (written once, cleared after render)
    pub(crate) pending_clipboard: Option<String>,

    /// Terminal control sequences that should be emitted after the frame diff.
    pub(crate) pending_terminal_commands: Vec<Vec<u8>>,

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
    pub fn skip_splash_for_tests(&mut self) {
        self.show_splash = false;
        self.show_settings = false;
        self.show_quit_confirm = false;
    }

    /// Resolves which room the dashboard's chat card should display, given
    /// the user's pinned favorites:
    /// - 0 pins → `#general`
    /// - 1 pin → that pin (or `#general` if it was left)
    /// - 2+ pins → favorites[index], clamped against the current list
    ///
    /// The strip only renders in the 2+ case; see [`Self::dashboard_strip_pins`].
    pub(crate) fn dashboard_active_room_id(&self) -> Option<uuid::Uuid> {
        let pins = &self.profile_state.profile().favorite_room_ids;
        let general = self.chat.general_room_id();
        match pins.len() {
            0 => general,
            1 => self.resolve_joined_room(pins[0]).or(general),
            len => {
                let idx = self.dashboard_favorite_index.min(len - 1);
                self.resolve_joined_room(pins[idx]).or(general)
            }
        }
    }

    /// Pins to render in the dashboard quick-switch strip. `None` when fewer
    /// than two favorites are pinned — there's nothing to switch between, so
    /// the strip is hidden entirely.
    pub(crate) fn dashboard_strip_pins(&self) -> Option<Vec<(uuid::Uuid, String, bool, i64)>> {
        let pins = &self.profile_state.profile().favorite_room_ids;
        if pins.len() < 2 {
            return None;
        }
        let catalog = self.chat.favorite_room_options();
        let active = self.dashboard_active_room_id();
        let pills: Vec<(uuid::Uuid, String, bool, i64)> = pins
            .iter()
            .filter_map(|id| {
                catalog
                    .iter()
                    .find(|option| option.id == *id)
                    .map(|option| {
                        let unread = self
                            .chat
                            .unread_counts
                            .get(&option.id)
                            .copied()
                            .unwrap_or(0);
                        (
                            option.id,
                            option.label.clone(),
                            Some(option.id) == active,
                            unread,
                        )
                    })
            })
            .collect();
        // If membership churn leaves <2 resolvable pins, hide the strip
        // rather than show a lonely pill.
        if pills.len() < 2 { None } else { Some(pills) }
    }

    /// Cycle the dashboard's active favorite. Wraps both directions. No-op
    /// when fewer than two pins are present.
    pub(crate) fn cycle_dashboard_favorite(&mut self, delta: isize) {
        let len = self.profile_state.profile().favorite_room_ids.len();
        if len < 2 {
            return;
        }
        let len_isize = len as isize;
        let current = self.dashboard_favorite_index.min(len - 1) as isize;
        let next = ((current + delta).rem_euclid(len_isize)) as usize;
        if next != current as usize {
            self.dashboard_previous_favorite_index = Some(current as usize);
        }
        self.dashboard_favorite_index = next;
    }

    /// Jump directly to `slot` (0-indexed) in the favorites list. Used by
    /// the `g<digit>` prefix. No-op when <2 pins or the slot is out of
    /// range. Records the current pin as the "last" target so `,` bounces
    /// back afterward.
    pub(crate) fn jump_dashboard_favorite(&mut self, slot: usize) {
        let len = self.profile_state.profile().favorite_room_ids.len();
        if len < 2 || slot >= len {
            return;
        }
        let current = self.dashboard_favorite_index.min(len - 1);
        if slot == current {
            return;
        }
        self.dashboard_previous_favorite_index = Some(current);
        self.dashboard_favorite_index = slot;
    }

    /// Vim-alternate-buffer style jump: swap the current and previous
    /// active pin. No-op when fewer than two pins are present or there's
    /// no prior pin to jump back to (first tap of this session).
    pub(crate) fn toggle_dashboard_last_favorite(&mut self) {
        let len = self.profile_state.profile().favorite_room_ids.len();
        if len < 2 {
            return;
        }
        let Some(prev) = self.dashboard_previous_favorite_index else {
            return;
        };
        let prev = prev.min(len - 1);
        let current = self.dashboard_favorite_index.min(len - 1);
        if prev == current {
            return;
        }
        self.dashboard_previous_favorite_index = Some(current);
        self.dashboard_favorite_index = prev;
    }

    /// Returns `room_id` if the user is currently a member of it; `None`
    /// otherwise. Used to guard against a pin that survived in the profile
    /// but vanished from the joined-rooms snapshot (left via `/leave`, etc).
    fn resolve_joined_room(&self, room_id: uuid::Uuid) -> Option<uuid::Uuid> {
        self.chat
            .favorite_room_options()
            .iter()
            .any(|option| option.id == room_id)
            .then_some(room_id)
    }

    pub fn show_splash_for_tests(&mut self, hint: impl Into<String>) {
        self.show_splash = true;
        self.show_settings = false;
        self.show_quit_confirm = false;
        self.splash_ticks = 1;
        self.splash_hint = hint.into();
    }

    pub fn new(config: SessionConfig) -> anyhow::Result<Self> {
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

        let shared = SharedBuffer::default();
        let backend = CrosstermBackend::new(shared.clone());
        let viewport = Viewport::Fixed(Rect::new(0, 0, cols, rows));
        let terminal = Terminal::with_options(backend, TerminalOptions { viewport })
            .context("failed to create terminal backend")?;

        let twenty_forty_eight_state = if let Some(game) = config.initial_2048_game {
            crate::app::games::twenty_forty_eight::state::State::restore(
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
            crate::app::games::twenty_forty_eight::state::State::new(
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
            crate::app::games::tetris::state::State::restore(
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
            crate::app::games::tetris::state::State::new(
                config.user_id,
                config.tetris_service.clone(),
                config
                    .initial_tetris_high_score
                    .as_ref()
                    .map(|score| score.score)
                    .unwrap_or(0),
            )
        };

        let sudoku_state = crate::app::games::sudoku::state::State::new(
            config.user_id,
            config.sudoku_service.clone(),
            config.initial_sudoku_games,
        );
        let nonogram_state = crate::app::games::nonogram::state::State::new(
            config.user_id,
            config.nonogram_service.clone(),
            config.nonogram_library,
            config.initial_nonogram_games,
        );
        let solitaire_state = crate::app::games::solitaire::state::State::new(
            config.user_id,
            config.solitaire_service.clone(),
            config.initial_solitaire_games,
        );
        let minesweeper_state = crate::app::games::minesweeper::state::State::new(
            config.user_id,
            config.minesweeper_service.clone(),
            config.initial_minesweeper_games,
        );
        let blackjack_state = crate::app::games::blackjack::state::State::new(
            config.blackjack_service.clone(),
            config.user_id,
            config.initial_chip_balance,
        );

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

        let active_users = config.active_users.clone();
        let splash_hint = super::common::splash_tips::choose_splash_hint(config.is_new_user);
        let initial_profile = Profile {
            theme_id: Some(config.initial_theme_id.clone()),
            ..Profile::default()
        };
        let mut settings_modal_state = settings_modal::state::SettingsModalState::new(
            config.profile_service.clone(),
            config.user_id,
        );
        settings_modal_state.open_from_profile(
            &initial_profile,
            Vec::new(),
            settings_modal::ui::MODAL_WIDTH,
        );

        Ok(Self {
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
            show_profile_modal: false,
            help_modal_state: help_modal::state::HelpModalState::new(),
            pending_escape: false,
            pending_escape_started_at: None,
            vt_input: crate::app::input::VtInputParser::default(),
            terminal,
            shared,
            visualizer: Visualizer::new(),
            browser_viz_buffer: VecDeque::new(),
            last_browser_viz_at: None,
            connect_url: format!("{}/{}", config.web_url, config.session_token),
            session_registry: config.session_registry,
            paired_client_registry: config.paired_client_registry,
            web_chat_registry: config.web_chat_registry,
            show_web_chat_qr: false,
            web_chat_qr_url: None,
            session_token: config.session_token,
            session_rx: config.session_rx,
            now_playing_rx: config.now_playing_rx,
            active_users: active_users.clone(),
            activity_feed_rx: config.activity_feed_rx,
            activity: VecDeque::new(),
            user_id: config.user_id,
            is_admin: config.is_admin,
            vote: vote::state::VoteState::new(config.vote_service, config.user_id, config.my_vote),
            chat: chat::state::ChatState::new(
                config.chat_service,
                config.notification_service,
                config.user_id,
                config.is_admin,
                active_users.clone(),
                config.article_service.clone(),
            ),
            dashboard_chat_rows_cache: chat::ui::ChatRowsCache::default(),
            active_room_rows_cache: chat::ui::ChatRowsCache::default(),
            dashboard_favorite_index: 0,
            dashboard_previous_favorite_index: None,
            dashboard_g_prefix_armed: false,
            profile_state: profile::state::ProfileState::new(
                config.profile_service.clone(),
                config.user_id,
                config.initial_theme_id,
            ),
            profile_modal_state: profile_modal::state::ProfileModalState::new(
                config.profile_service.clone(),
            ),
            settings_modal_state,
            leaderboard_rx: config.leaderboard_rx,
            leaderboard: Arc::new(LeaderboardData::default()),
            bonsai_state,
            game_selection: 0,
            is_playing_game: false,
            twenty_forty_eight_state,
            tetris_state,
            sudoku_state,
            nonogram_state,
            solitaire_state,
            minesweeper_state,
            blackjack_state,
            chip_balance: config.initial_chip_balance,
            pending_clipboard: None,
            pending_terminal_commands: Vec::new(),
            last_notify_at: None,
            is_draining: config.is_draining,
            icon_picker_open: false,
            icon_picker_state: super::icon_picker::IconPickerState::default(),
            icon_catalog: None,
            last_terminal_bg: None,
        })
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), io::Error> {
        tracing::debug!(cols, rows, "window resized");
        self.size = (cols, rows);
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

    pub fn paired_client_state(&self) -> Option<ClientAudioState> {
        self.paired_client_registry
            .as_ref()
            .and_then(|registry| registry.snapshot(&self.session_token))
    }

    /// Reset the terminal diff state so the next `render()` emits a full frame.
    /// Used by integration test helpers.
    #[allow(dead_code)]
    pub fn reset_render(&mut self) {
        let _ = self.terminal.clear();
        self.shared.take();
    }

    pub fn enter_alt_screen() -> Vec<u8> {
        let mut buf = Vec::new();
        crossterm::execute!(
            buf,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(ClearType::All)
        )
        .expect("failed to enter alt screen");
        // 1000h = basic mouse tracking (button press/release + scroll wheel)
        // 1006h = SGR extended encoding (ESC[< sequences instead of legacy X11)
        // 2004h = bracketed paste mode (ESC[200~ ... ESC[201~)
        buf.extend_from_slice(b"\x1b[?1000h\x1b[?1006h\x1b[?2004h");
        buf
    }

    pub fn leave_alt_screen() -> Vec<u8> {
        let mut buf = Vec::new();
        // 2004l = disable bracketed paste
        // 1006l = disable SGR mouse tracking
        // 1000l = disable basic mouse tracking
        // OSC 111 = reset terminal background color
        buf.extend_from_slice(b"\x1b[?2004l\x1b[?1006l\x1b[?1000l\x1b]111\x1b\\");
        crossterm::execute!(buf, cursor::Show, terminal::LeaveAlternateScreen)
            .expect("failed to leave alt screen");
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
}
