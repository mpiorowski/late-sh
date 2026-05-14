use std::sync::Arc;

use anyhow::Context;
use late_core::MutexRecover;
use late_core::api_types::NowPlaying;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear},
};

use late_core::models::leaderboard::LeaderboardData;
use late_core::models::user::RightSidebarMode;

use super::{
    artboard, bonsai, chat,
    common::{
        primitives::{Banner, BannerKind, Screen, draw_banner},
        sidebar::{SidebarProps, draw_sidebar, sidebar_clock_text},
        theme,
    },
    dashboard, help_modal, icon_picker, mod_modal, profile_modal, quit_confirm, room_search_modal,
    settings_modal,
    state::{App, NotificationMode},
    terminal_help_modal,
    visualizer::Visualizer,
};

fn sanitize_notification_field(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '\x1b' | '\x07' | '\n' | '\r' => ' ',
            ';' => '|',
            _ => ch,
        })
        .collect()
}

fn desktop_notification_bytes(
    title: &str,
    body: &str,
    mode: NotificationMode,
    bell: bool,
) -> Vec<u8> {
    // OSC 777 carries (title, body) separately — kitty, Ghostty, rxvt-unicode,
    // foot, wezterm, konsole. OSC 9 is iTerm2's single-string variant.
    // `Both` is the startup default; an XTVERSION reply narrows it so we
    // don't send duplicate notifications on terminals (kitty) that accept
    // both sequences.
    let title = sanitize_notification_field(title);
    let body = sanitize_notification_field(body);
    let osc777 = format!("\x1b]777;notify;{title};{body}\x1b\\");
    let osc9 = format!("\x1b]9;{title}: {body}\x1b\\");
    let bell = if bell { "\x07" } else { "" };
    match mode {
        NotificationMode::Both => format!("{osc777}{osc9}{bell}").into_bytes(),
        NotificationMode::Osc777 => format!("{osc777}{bell}").into_bytes(),
        NotificationMode::Osc9 => format!("{osc9}{bell}").into_bytes(),
    }
}

fn sidebar_enabled(show_settings: bool, draft_enabled: bool, profile_enabled: bool) -> bool {
    if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

/// Map a top-level screen to its 1-based slot in `right_sidebar_screens`.
pub(crate) fn screen_number(screen: Screen) -> u8 {
    match screen {
        Screen::Dashboard => 1,
        Screen::Arcade => 2,
        Screen::Rooms => 3,
        Screen::Artboard => 4,
    }
}

/// Resolve whether the right sidebar should render on `screen` given a profile
/// (or draft) sidebar mode and per-screen visibility set.
pub(crate) fn resolve_right_sidebar_enabled(
    mode: RightSidebarMode,
    screens: &[u8],
    screen: Screen,
) -> bool {
    match mode {
        RightSidebarMode::On => true,
        RightSidebarMode::Off => false,
        RightSidebarMode::Custom => screens.contains(&screen_number(screen)),
    }
}

fn room_list_sidebar_enabled(
    show_settings: bool,
    draft_enabled: bool,
    profile_enabled: bool,
) -> bool {
    if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

fn lounge_info_enabled(show_settings: bool, draft_enabled: bool, profile_enabled: bool) -> bool {
    if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

fn dashboard_wire_enabled(show_settings: bool, draft_enabled: bool, profile_enabled: bool) -> bool {
    if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

fn dashboard_home_selected(
    general_room_id: Option<uuid::Uuid>,
    selected_room_id: Option<uuid::Uuid>,
    synthetic_selected: bool,
) -> bool {
    general_room_id.is_some_and(|general| selected_room_id == Some(general)) && !synthetic_selected
}

struct DrawContext<'a> {
    connect_url: &'a str,
    dashboard_view: dashboard::ui::DashboardRenderInput<'a>,
    chat_view: chat::ui::ChatRenderInput<'a>,
    game_selection: usize,
    is_playing_game: bool,
    rooms_create_flow: Option<&'a crate::app::rooms::backend::CreateRoomFlow>,
    rooms_snapshot: &'a crate::app::rooms::svc::RoomsSnapshot,
    rooms_selected_index: usize,
    rooms_active_room: Option<&'a crate::app::rooms::svc::RoomListItem>,
    rooms_filter: crate::app::rooms::filter::RoomsFilter,
    rooms_search_active: bool,
    rooms_search_query: &'a str,
    rooms_usernames: &'a std::collections::HashMap<uuid::Uuid, String>,
    room_game_registry: &'a crate::app::rooms::registry::RoomGameRegistry,
    active_room_game: Option<&'a dyn crate::app::rooms::backend::ActiveRoomBackend>,
    rooms_chat_view: Option<chat::ui::EmbeddedRoomChatView<'a>>,
    twenty_forty_eight_state: &'a crate::app::arcade::twenty_forty_eight::state::State,
    tetris_state: &'a crate::app::arcade::tetris::state::State,
    snake_state: &'a crate::app::arcade::snake::state::State,
    sudoku_state: &'a crate::app::arcade::sudoku::state::State,
    nonogram_state: &'a crate::app::arcade::nonogram::state::State,
    solitaire_state: &'a crate::app::arcade::solitaire::state::State,
    minesweeper_state: &'a crate::app::arcade::minesweeper::state::State,
    dartboard_state: Option<&'a crate::app::artboard::state::State>,
    artboard_interacting: bool,
    leaderboard: &'a Arc<LeaderboardData>,
    visualizer: &'a Visualizer,
    now_playing: Option<&'a NowPlaying>,
    paired_client: Option<&'a crate::session::ClientAudioState>,
    vote_view: crate::app::vote::ui::VoteCardView<'a>,
    sidebar_clock: &'a str,
    online_count: usize,
    bonsai: &'a crate::app::bonsai::state::BonsaiState,
    activity: &'a std::collections::VecDeque<crate::app::activity::event::ActivityEvent>,
    banner: Option<&'a Banner>,
    is_admin: bool,
    is_moderator: bool,
    show_right_sidebar: bool,
    show_room_list_sidebar: bool,
    show_settings: bool,
    settings_modal_state: &'a settings_modal::state::SettingsModalState,
    show_quit_confirm: bool,
    show_mod_modal: bool,
    show_hub_modal: bool,
    hub_state: &'a crate::app::hub::state::HubState,
    mod_modal_state: &'a mod_modal::state::ModModalState,
    show_profile_modal: bool,
    profile_modal_state: &'a profile_modal::state::ProfileModalState,
    show_bonsai_modal: bool,
    bonsai_care_state: &'a bonsai::care::BonsaiCareState,
    show_help: bool,
    help_modal_state: &'a help_modal::state::HelpModalState,
    show_terminal_help: bool,
    terminal_help_modal_state: &'a terminal_help_modal::state::TerminalHelpModalState,
    show_splash: bool,
    splash_ticks: usize,
    splash_hint: &'a str,
    show_web_chat_qr: bool,
    web_chat_qr_url: Option<&'a str>,
    show_pair_modal: bool,
    pair_url: &'a str,
    room_search_modal_open: bool,
    room_search_modal_state: &'a room_search_modal::state::RoomSearchModalState,
    chat_state: &'a chat::state::ChatState,
    user_id: uuid::Uuid,
    news_modal: Option<chat::news::ui::ArticleModalView<'a>>,
    is_draining: bool,
    icon_picker_open: bool,
    icon_picker_state: &'a icon_picker::IconPickerState,
    icon_catalog: Option<&'a icon_picker::catalog::IconCatalogData>,
    mentions_unread_count: i64,
    top_rooms: &'a [dashboard::ui::DashboardRoomCard],
    home_selected: bool,
}

impl App {
    pub fn render(&mut self) -> anyhow::Result<Vec<u8>> {
        // Init theme and layout sync — preview settings-modal draft live while open.
        let active_theme_id = if self.show_settings {
            self.settings_modal_state
                .draft()
                .theme_id
                .clone()
                .unwrap_or_else(|| self.profile_state.theme_id().to_string())
        } else {
            self.profile_state.theme_id().to_string()
        };
        theme::set_current_by_id(&active_theme_id);
        self.chat.refresh_composer_theme();

        // Synchronize terminal background color with theme bg_canvas if enabled
        let enabled = if self.show_settings {
            self.settings_modal_state.draft().enable_background_color
        } else {
            self.profile_state.profile().enable_background_color
        };
        let current_bg = if enabled {
            Some(theme::BG_CANVAS())
        } else {
            None
        };

        if current_bg != self.last_terminal_bg {
            let cmd = if let Some(color) = current_bg {
                let hex = theme::color_to_hex(color);
                format!("\x1b]11;{}\x1b\\", hex).into_bytes()
            } else {
                b"\x1b]111\x1b\\".to_vec()
            };
            self.pending_terminal_commands.push(cmd);
            self.last_terminal_bg = current_bg;
        }

        let area = Rect::new(0, 0, self.size.0, self.size.1);
        let show_right_sidebar = sidebar_enabled(
            self.show_settings,
            resolve_right_sidebar_enabled(
                self.settings_modal_state.draft().right_sidebar_mode,
                &self.settings_modal_state.draft().right_sidebar_screens,
                self.screen,
            ),
            resolve_right_sidebar_enabled(
                self.profile_state.profile().right_sidebar_mode,
                &self.profile_state.profile().right_sidebar_screens,
                self.screen,
            ),
        );
        let show_room_list_sidebar = room_list_sidebar_enabled(
            self.show_settings,
            self.settings_modal_state.draft().show_room_list_sidebar,
            self.profile_state.profile().show_room_list_sidebar,
        );
        let show_lounge_info = lounge_info_enabled(
            self.show_settings,
            self.settings_modal_state.draft().show_dashboard_header,
            self.profile_state.profile().show_dashboard_header,
        );
        let show_dashboard_wire = dashboard_wire_enabled(
            self.show_settings,
            self.settings_modal_state.draft().show_dashboard_wire,
            self.profile_state.profile().show_dashboard_wire,
        );
        let screen = self.screen;
        let now_playing: Option<NowPlaying> = self
            .now_playing_rx
            .as_mut()
            .and_then(|rx| rx.borrow_and_update().clone());
        let paired_client = self.paired_client_state();
        let vote_snapshot = self.vote.snapshot();
        let vote_my_vote = self.vote.my_vote();
        let vote_ends_in = vote_snapshot.remaining_until_switch();
        let banner = self.active_banner().cloned();
        let sidebar_clock = sidebar_clock_text(self.profile_state.profile().timezone.as_deref());
        let visualizer = &self.visualizer;
        let chat_usernames = self.chat.usernames();
        let chat_countries = self.chat.countries();
        let bonsai_glyphs = self.chat.bonsai_glyphs();
        let message_reactions = self.chat.message_reactions();
        let shell_active_room = self.chat.selected_room_id;
        let synthetic_selected = self.chat.feeds_selected
            || self.chat.news_selected
            || self.chat.notifications_selected
            || self.chat.discover_selected
            || self.chat.showcase_selected
            || self.chat.work_selected;
        let home_selected = dashboard_home_selected(
            self.chat.general_room_id(),
            shell_active_room,
            synthetic_selected,
        );
        let top_rooms =
            dashboard::ui::top_dashboard_rooms(&self.rooms_snapshot, &self.room_game_registry, 3);
        let online_count = self
            .active_users
            .as_ref()
            .map(|active_users| active_users.lock_recover().len())
            .unwrap_or(0);
        let dashboard_cycle_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let dashboard_wire_articles = self.chat.news.all_articles();
        let dashboard_messages = shell_active_room
            .map(|room_id| self.chat.messages_for_room(room_id))
            .unwrap_or(&[]);
        let dashboard_selected_news_message = shell_active_room
            .is_some_and(|room_id| self.chat.selected_message_is_news_in_room(room_id));
        let dashboard_view = dashboard::ui::DashboardRenderInput {
            activity: &self.activity,
            online_count,
            wire_news_articles: dashboard_wire_articles,
            dashboard_cycle_secs,
            show_lounge_info,
            show_dashboard_wire,
            pinned_messages: self.chat.pinned_messages(),
            chat_view: chat::ui::DashboardChatView {
                messages: dashboard_messages,
                overlay: self.chat.overlay(),
                rows_cache: &mut self.dashboard_chat_rows_cache,
                usernames: chat_usernames,
                countries: chat_countries,
                message_reactions,
                current_user_id: self.user_id,
                selected_message_id: self.chat.selected_message_id,
                selected_news_message: dashboard_selected_news_message,
                highlighted_message_id: self.chat.highlighted_message_id,
                reaction_picker_active: self.chat.is_reaction_leader_active(),
                composer: self.chat.composer(),
                composing: self.chat.composing,
                mention_matches: &self.chat.mention_ac.matches,
                mention_selected: self.chat.mention_ac.selected,
                mention_active: self.chat.mention_ac.active,
                reply_author: self.chat.reply_target().map(|reply| reply.author.as_str()),
                is_editing: self.chat.edited_message_id.is_some(),
                bonsai_glyphs,
                inline_images: &self.chat.inline_image_cache,
            },
        };
        let news_view = chat::news::ui::ArticleListView {
            articles: self.chat.news.displayed_articles(),
            selected_index: self.chat.news.selected_index(),
            marker_read_at: self.chat.news.marker_read_at(),
            mine_only: self.chat.news.mine_only(),
        };
        let feeds_view = chat::feeds::ui::FeedListView {
            entries: self.chat.feeds.all_entries(),
            selected_index: self.chat.feeds.selected_index(),
            has_feeds: self.chat.feeds.has_feeds(),
            marker_read_at: self.chat.feeds.marker_read_at(),
        };
        let discover_view = chat::discover::ui::DiscoverListView {
            items: self.chat.discover.all_items(),
            selected_index: self.chat.discover.selected_index(),
            loading: self.chat.discover.is_loading(),
        };
        let notifications_view = chat::notifications::ui::NotificationListView {
            items: self.chat.notifications.all_items(),
            selected_index: self.chat.notifications.selected_index(),
            marker_read_at: self.chat.notifications.marker_read_at(),
        };
        let showcase_view = chat::showcase::ui::ShowcaseListView {
            items: self.chat.showcase.all_items(),
            selected_index: self.chat.showcase.selected_index(),
            current_user_id: self.user_id,
            is_admin: self.chat.showcase.is_admin(),
            marker_read_at: self.chat.showcase.marker_read_at(),
            mine_only: self.chat.showcase.mine_only(),
        };
        let showcase_unread_count = self.chat.showcase.unread_count();
        let showcase_composing = self.chat.showcase.composing();
        let web_base_url = self
            .connect_url
            .rsplit_once('/')
            .map_or(&*self.connect_url, |p| p.0);
        let work_view = chat::work::ui::WorkListView {
            items: self.chat.work.all_items(),
            selected_index: self.chat.work.selected_index(),
            current_user_id: self.user_id,
            is_admin: self.chat.work.is_admin(),
            marker_read_at: self.chat.work.marker_read_at(),
            profile_base_url: web_base_url,
            mine_only: self.chat.work.mine_only(),
        };
        let work_unread_count = self.chat.work.unread_count();
        let work_composing = self.chat.work.composing();
        let news_modal = self
            .chat
            .news_modal()
            .map(|modal| chat::news::ui::ArticleModalView {
                payload: &modal.payload,
                meta: &modal.meta,
            });
        let selected_news_message = self
            .chat
            .selected_room_id
            .is_some_and(|room_id| self.chat.selected_message_is_news_in_room(room_id));
        let chat_view = chat::ui::ChatRenderInput {
            feeds_selected: self.chat.feeds_selected,
            feeds_processing: self.chat.feeds.processing(),
            feeds_unread_count: self.chat.feeds.unread_count(),
            feeds_view,
            news_selected: self.chat.news_selected,
            news_unread_count: self.chat.news.unread_count(),
            news_view,
            discover_selected: self.chat.discover_selected,
            discover_view,
            rows_cache: &mut self.active_room_rows_cache,
            chat_rooms: self.chat.rooms.as_slice(),
            overlay: self.chat.overlay(),
            usernames: chat_usernames,
            countries: chat_countries,
            message_reactions,
            inline_images: &self.chat.inline_image_cache,
            unread_counts: &self.chat.unread_counts,
            favorite_room_ids: &self.profile_state.profile().favorite_room_ids,
            selected_room_id: self.chat.selected_room_id,
            room_jump_active: self.chat.room_jump_active,
            selected_message_id: self.chat.selected_message_id,
            selected_news_message,
            reaction_picker_active: self.chat.is_reaction_leader_active(),
            highlighted_message_id: self.chat.highlighted_message_id,
            composer: self.chat.composer(),
            composing: self.chat.composing,
            current_user_id: self.user_id,
            cursor_visible: self.chat.cursor_visible(),
            mention_matches: &self.chat.mention_ac.matches,
            mention_selected: self.chat.mention_ac.selected,
            mention_active: self.chat.mention_ac.active,
            reply_author: self.chat.reply_target().map(|reply| reply.author.as_str()),
            is_editing: self.chat.edited_message_id.is_some(),
            bonsai_glyphs,
            news_composer: self.chat.news.composer(),
            news_composing: self.chat.news.composing(),
            news_processing: self.chat.news.processing(),
            notifications_selected: self.chat.notifications_selected,
            notifications_unread_count: self.chat.notifications.unread_count(),
            notifications_view,
            showcase_selected: self.chat.showcase_selected,
            showcase_unread_count,
            showcase_view,
            showcase_state: Some(&self.chat.showcase),
            showcase_composing,
            work_selected: self.chat.work_selected,
            work_unread_count,
            work_view,
            work_state: Some(&self.chat.work),
            work_composing,
        };
        self.settings_modal_state
            .set_modal_width(settings_modal::ui::MODAL_WIDTH);
        let rooms_chat_view =
            self.rooms_active_room
                .as_ref()
                .map(|room| chat::ui::EmbeddedRoomChatView {
                    title: "Chat",
                    messages: self.chat.messages_for_room(room.chat_room_id),
                    overlay: self.chat.overlay(),
                    rows_cache: &mut self.rooms_chat_rows_cache,
                    usernames: chat_usernames,
                    countries: chat_countries,
                    message_reactions,
                    inline_images: &self.chat.inline_image_cache,
                    current_user_id: self.user_id,
                    selected_message_id: self.chat.selected_message_id,
                    highlighted_message_id: self.chat.highlighted_message_id,
                    reaction_picker_active: self.chat.is_reaction_leader_active(),
                    composer: self.chat.composer(),
                    composing: self.chat.composing,
                    mention_matches: &self.chat.mention_ac.matches,
                    mention_selected: self.chat.mention_ac.selected,
                    mention_active: self.chat.mention_ac.active,
                    reply_author: self.chat.reply_target().map(|reply| reply.author.as_str()),
                    is_editing: self.chat.edited_message_id.is_some(),
                    bonsai_glyphs,
                });
        let terminal = &mut self.terminal;

        terminal
            .draw(|frame| {
                Self::draw(
                    frame,
                    area,
                    screen,
                    DrawContext {
                        connect_url: self.connect_url.as_str(),
                        dashboard_view,
                        chat_view,
                        game_selection: self.game_selection,
                        is_playing_game: self.is_playing_game,
                        rooms_create_flow: self.rooms_create_flow.as_ref(),
                        rooms_snapshot: &self.rooms_snapshot,
                        rooms_selected_index: self.rooms_selected_index,
                        rooms_active_room: self.rooms_active_room.as_ref(),
                        rooms_filter: self.rooms_filter,
                        rooms_search_active: self.rooms_search_active,
                        rooms_search_query: self.rooms_search_query.as_str(),
                        rooms_usernames: chat_usernames,
                        room_game_registry: &self.room_game_registry,
                        active_room_game: self.active_room_game.as_deref(),
                        rooms_chat_view,
                        twenty_forty_eight_state: &self.twenty_forty_eight_state,
                        tetris_state: &self.tetris_state,
                        snake_state: &self.snake_state,
                        sudoku_state: &self.sudoku_state,
                        nonogram_state: &self.nonogram_state,
                        solitaire_state: &self.solitaire_state,
                        minesweeper_state: &self.minesweeper_state,
                        dartboard_state: self.dartboard_state.as_ref(),
                        artboard_interacting: self.artboard_interacting,
                        leaderboard: &self.leaderboard,
                        visualizer,
                        now_playing: now_playing.as_ref(),
                        paired_client: paired_client.as_ref(),
                        vote_view: crate::app::vote::ui::VoteCardView {
                            vote_counts: &vote_snapshot.counts,
                            current_genre: vote_snapshot.current_genre,
                            my_vote: vote_my_vote,
                            ends_in: vote_ends_in,
                        },
                        sidebar_clock: &sidebar_clock,
                        online_count,
                        bonsai: &self.bonsai_state,
                        activity: &self.activity,
                        banner: banner.as_ref(),
                        is_admin: self.is_admin,
                        is_moderator: self.is_moderator,
                        show_right_sidebar,
                        show_room_list_sidebar,
                        show_settings: self.show_settings,
                        settings_modal_state: &self.settings_modal_state,
                        show_quit_confirm: self.show_quit_confirm,
                        show_mod_modal: self.show_mod_modal,
                        show_hub_modal: self.show_hub_modal,
                        hub_state: &self.hub_state,
                        mod_modal_state: &self.mod_modal_state,
                        show_profile_modal: self.show_profile_modal,
                        profile_modal_state: &self.profile_modal_state,
                        show_bonsai_modal: self.show_bonsai_modal,
                        bonsai_care_state: &self.bonsai_care_state,
                        show_help: self.show_help,
                        help_modal_state: &self.help_modal_state,
                        show_terminal_help: self.show_terminal_help,
                        terminal_help_modal_state: &self.terminal_help_modal_state,
                        show_splash: self.show_splash,
                        splash_ticks: self.splash_ticks,
                        splash_hint: &self.splash_hint,
                        show_web_chat_qr: self.show_web_chat_qr,
                        web_chat_qr_url: self.web_chat_qr_url.as_deref(),
                        show_pair_modal: self.show_pair_modal,
                        pair_url: &self.connect_url,
                        room_search_modal_open: self.room_search_modal_state.is_open(),
                        room_search_modal_state: &self.room_search_modal_state,
                        chat_state: &self.chat,
                        user_id: self.user_id,
                        news_modal,
                        is_draining: self.is_draining.load(std::sync::atomic::Ordering::Relaxed),
                        icon_picker_open: self.icon_picker_open,
                        icon_picker_state: &self.icon_picker_state,
                        icon_catalog: self.icon_catalog.as_ref(),
                        mentions_unread_count: self.chat.notifications.unread_count(),
                        top_rooms: &top_rooms,
                        home_selected,
                    },
                )
            })
            .context("failed to draw frame")?;

        // Emit OSC 52 clipboard sequence if a copy was requested.
        // Format: \x1b]52;c;<base64>\x07
        if let Some(text) = self.pending_clipboard.take() {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
            self.pending_terminal_commands
                .push(format!("\x1b]52;c;{}\x07", encoded).into_bytes());
        }

        // Emit OSC 777/OSC 9 desktop notifications for pending chat events.
        // Kind strings ("dms", "mentions", …) must match users.settings.notify_kinds.
        if !self.chat.pending_notifications.is_empty() {
            let profile = self.profile_state.profile();
            let enabled_kinds = profile.notify_kinds.clone();
            let cooldown_secs = profile.notify_cooldown_mins as u64 * 60;
            let cooldown_ok = self
                .last_notify_at
                .map(|t| t.elapsed() >= std::time::Duration::from_secs(cooldown_secs))
                .unwrap_or(true);

            if cooldown_ok
                && let Some(notif) = self
                    .chat
                    .pending_notifications
                    .iter()
                    .find(|n| enabled_kinds.iter().any(|k| k == n.kind))
            {
                tracing::info!(
                    kind = notif.kind,
                    title = notif.title,
                    body = notif.body,
                    "emitting desktop notification"
                );
                let payload = desktop_notification_bytes(
                    &notif.title,
                    &notif.body,
                    NotificationMode::from_format(profile.notify_format.as_deref()),
                    profile.notify_bell,
                );
                self.pending_terminal_commands.push(payload);
                self.last_notify_at = Some(std::time::Instant::now());
            } else {
                tracing::debug!(
                    ?cooldown_ok,
                    pending_count = self.chat.pending_notifications.len(),
                    "dropping pending desktop notifications"
                );
            }
            // Always drain — notifications during cooldown are dropped, not queued.
            self.chat.pending_notifications.clear();
        }

        Ok(self.shared.take())
    }

    fn active_banner(&self) -> Option<&Banner> {
        self.banner.as_ref().filter(|b| b.is_active())
    }

    fn draw(frame: &mut Frame, area: Rect, screen: Screen, ctx: DrawContext<'_>) {
        if ctx.show_splash {
            let msg = "take a break, grab a coffee";
            // Animate typing the message (1 char per tick instead of 1 char per 2 ticks)
            let len = msg.len();
            let visible_len = ctx.splash_ticks.max(1).min(len);
            let mut text = msg[..visible_len].to_string();

            if visible_len < len {
                if ctx.splash_ticks % 4 < 2 {
                    text.push('█');
                } else {
                    text.push(' ');
                }
            } else if ctx.splash_ticks % 16 < 8 {
                text.push('█');
            } else {
                text.push(' ');
            }

            let steam_frames = [
                ["   (  )   ", "    )(    "],
                ["    )(    ", "   (  )   "],
                ["   )  (   ", "    )(    "],
                ["    )(    ", "   (  )   "],
            ];
            let steam = &steam_frames[(ctx.splash_ticks / 6) % steam_frames.len()];
            let base = [" .------. ", "|      |`\\", "|      | /", " `----'   "];

            let mut lines = Vec::new();
            for s in steam {
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    *s,
                    Style::default().fg(theme::TEXT_FAINT()),
                )));
            }
            for b in &base {
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    *b,
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
            lines.push(ratatui::text::Line::from(""));
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                text,
                Style::default().fg(theme::TEXT_MUTED()),
            )));

            let p = ratatui::widgets::Paragraph::new(lines).centered();
            let layout = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Fill(1),
                ratatui::layout::Constraint::Length(8),
                ratatui::layout::Constraint::Fill(1),
            ])
            .split(area);

            frame.render_widget(p, layout[1]);
            let splash_bottom = layout[1].bottom();
            let gap = area.bottom().saturating_sub(splash_bottom);
            let hint_y = splash_bottom + (gap * 3 / 4);
            if hint_y < area.bottom() {
                let hint_area = Rect::new(area.x, hint_y, area.width, 1);
                let hint = ratatui::text::Line::from(ratatui::text::Span::styled(
                    ctx.splash_hint,
                    Style::default().fg(theme::TEXT_DIM()),
                ));
                let hint_paragraph = ratatui::widgets::Paragraph::new(hint).centered();
                frame.render_widget(hint_paragraph, hint_area);
            }
            return;
        }

        let mut block = Block::default()
            .title(app_frame_title(screen, &ctx))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
        if let Some(hud) = mentions_hud_title(ctx.mentions_unread_count) {
            block = block.title_top(hud);
        }
        block = block.title_bottom(app_frame_help_hint_title());
        block = block.title_bottom(app_frame_sponsor_title());

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Clear, inner);

        let (content_area, sidebar_area) = if ctx.show_right_sidebar {
            let main_layout =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(inner);
            (main_layout[0], Some(main_layout[1]))
        } else {
            (inner, None)
        };
        let connect_url = ctx.connect_url;

        match screen {
            Screen::Dashboard => {
                const HOME_RAIL_WIDTH: u16 = 24;
                let (rail_area, center_area) =
                    if ctx.show_room_list_sidebar && content_area.width > HOME_RAIL_WIDTH + 20 {
                        let split = Layout::horizontal([
                            Constraint::Length(HOME_RAIL_WIDTH),
                            Constraint::Fill(1),
                        ])
                        .split(content_area);
                        (Some(split[0]), split[1])
                    } else {
                        (None, content_area)
                    };

                if let Some(rail_area) = rail_area {
                    chat::ui::draw_room_list_rail(frame, rail_area, &ctx.chat_view);
                }

                if ctx.home_selected {
                    dashboard::ui::draw_dashboard(frame, center_area, ctx.dashboard_view);
                } else {
                    chat::ui::draw_chat_center(frame, center_area, ctx.chat_view);
                }
            }
            Screen::Artboard => {
                if let Some(state) = ctx.dartboard_state {
                    artboard::ui::draw_game(frame, content_area, state, ctx.artboard_interacting);
                }
            }
            Screen::Arcade => crate::app::arcade::ui::draw_arcade_hub(
                frame,
                content_area,
                &crate::app::arcade::ui::ArcadeHubView {
                    game_selection: ctx.game_selection,
                    is_playing_game: ctx.is_playing_game,
                    twenty_forty_eight_state: ctx.twenty_forty_eight_state,
                    tetris_state: ctx.tetris_state,
                    snake_state: ctx.snake_state,
                    sudoku_state: ctx.sudoku_state,
                    nonogram_state: ctx.nonogram_state,
                    solitaire_state: ctx.solitaire_state,
                    minesweeper_state: ctx.minesweeper_state,
                },
            ),
            Screen::Rooms => crate::app::rooms::ui::draw_rooms_page(
                frame,
                content_area,
                crate::app::rooms::ui::RoomsPageView {
                    create_flow: ctx.rooms_create_flow,
                    snapshot: ctx.rooms_snapshot,
                    selected_index: ctx.rooms_selected_index,
                    active_room: ctx.rooms_active_room,
                    active_room_game: ctx.active_room_game,
                    room_game_registry: ctx.room_game_registry,
                    is_admin: ctx.is_admin,
                    is_moderator: ctx.is_moderator,
                    filter: ctx.rooms_filter,
                    search_active: ctx.rooms_search_active,
                    search_query: ctx.rooms_search_query,
                    usernames: ctx.rooms_usernames,
                    active_room_chat: ctx.rooms_chat_view,
                },
            ),
        }

        if let Some(sidebar_area) = sidebar_area {
            draw_sidebar(
                frame,
                sidebar_area,
                &SidebarProps {
                    game_selection: ctx.game_selection,
                    is_playing_game: ctx.is_playing_game,
                    visualizer: ctx.visualizer,
                    now_playing: ctx.now_playing,
                    paired_client: ctx.paired_client,
                    vote: crate::app::vote::ui::VoteCardView {
                        vote_counts: ctx.vote_view.vote_counts,
                        current_genre: ctx.vote_view.current_genre,
                        my_vote: ctx.vote_view.my_vote,
                        ends_in: ctx.vote_view.ends_in,
                    },
                    online_count: ctx.online_count,
                    bonsai: ctx.bonsai,
                    audio_beat: ctx.visualizer.beat(),
                    connect_url,
                    activity: ctx.activity,
                    clock_text: ctx.sidebar_clock,
                    top_rooms: ctx.top_rooms,
                },
            );
        }

        // Toast banner overlay at top of content area
        let banner = if ctx.is_draining {
            Some(Banner {
                message:
                    "⚠️ Server updating! Press 'q' to quit, then reconnect to join the new pod."
                        .to_string(),
                kind: BannerKind::Error,
                created_at: std::time::Instant::now(),
            })
        } else {
            ctx.banner.cloned()
        };

        if let Some(banner) = banner {
            let color = match banner.kind {
                BannerKind::Success => theme::SUCCESS(),
                BannerKind::Error => theme::ERROR(),
            };
            // leading space (1) + icon (2) + message + border padding (4)
            let msg_w = (banner.message.len() as u16) + 7;
            let toast_w = msg_w.max(20).min(inner.width);
            let toast_x = inner.x + inner.width.saturating_sub(toast_w);
            let toast_area = Rect::new(toast_x, inner.y, toast_w, 3);
            frame.render_widget(Clear, toast_area);
            let notif_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));
            let notif_inner = notif_block.inner(toast_area);
            frame.render_widget(notif_block, toast_area);
            draw_banner(frame, notif_inner, &banner);
        }

        if ctx.show_settings {
            settings_modal::ui::draw(frame, inner, ctx.settings_modal_state);
        }

        if ctx.show_mod_modal {
            mod_modal::ui::draw(frame, inner, ctx.mod_modal_state);
        }

        if ctx.show_hub_modal {
            crate::app::hub::ui::draw(frame, inner, ctx.hub_state, ctx.leaderboard, ctx.user_id);
        }

        if ctx.show_profile_modal {
            profile_modal::ui::draw(frame, inner, ctx.profile_modal_state);
        }

        if ctx.show_bonsai_modal {
            bonsai::modal_ui::draw(
                frame,
                inner,
                ctx.bonsai,
                ctx.bonsai_care_state,
                ctx.visualizer.beat(),
            );
        }

        if ctx.show_help {
            help_modal::ui::draw(frame, inner, ctx.help_modal_state);
        }

        if ctx.show_terminal_help {
            terminal_help_modal::ui::draw(frame, inner, ctx.terminal_help_modal_state);
        }

        if ctx.show_quit_confirm {
            quit_confirm::ui::draw(frame, inner);
        }

        if let Some(news_modal) = ctx.news_modal {
            chat::news::ui::draw_article_modal(frame, inner, news_modal);
        }

        if ctx.show_web_chat_qr
            && let Some(url) = ctx.web_chat_qr_url
        {
            let (title, subtitle) = if url.contains("/chat/") {
                ("Web Chat", "Scan to open web chat")
            } else {
                ("Pair", "Scan to pair audio")
            };
            super::common::qr::draw_qr_overlay(frame, inner, url, title, subtitle);
        }

        if ctx.show_pair_modal {
            super::common::pair_modal::draw(frame, inner, ctx.pair_url);
        }

        if ctx.room_search_modal_open {
            room_search_modal::ui::draw(
                frame,
                inner,
                ctx.room_search_modal_state,
                ctx.chat_state,
                ctx.user_id,
            );
        }

        if ctx.icon_picker_open
            && let Some(catalog) = ctx.icon_catalog
        {
            icon_picker::picker::render(frame, area, ctx.icon_picker_state, catalog);
        }
    }
}

fn app_frame_title(screen: Screen, ctx: &DrawContext<'_>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        " late.sh ",
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )];

    spans.push(Span::styled("| ", Style::default().fg(theme::BORDER_DIM())));
    let tabs = [
        (Screen::Dashboard, "1"),
        (Screen::Arcade, "2"),
        (Screen::Rooms, "3"),
        (Screen::Artboard, "4"),
    ];
    for (idx, (tab_screen, key)) in tabs.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        let style = if *tab_screen == screen {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(*key, style));
    }

    let page_title = match screen {
        Screen::Dashboard => "Home",
        Screen::Arcade => "The Arcade",
        Screen::Artboard => "Artboard",
        Screen::Rooms => "Rooms",
    };
    spans.push(Span::styled(
        " | ",
        Style::default().fg(theme::BORDER_DIM()),
    ));
    spans.push(Span::styled(
        format!("{page_title} "),
        Style::default().fg(theme::TEXT_MUTED()),
    ));

    if screen == Screen::Rooms {
        append_rooms_title_extras(&mut spans, ctx);
    }

    if screen == Screen::Dashboard {
        append_home_title_extras(&mut spans, ctx);
    }

    if screen == Screen::Arcade && ctx.is_playing_game {
        append_arcade_title_extras(&mut spans, ctx);
    }

    if screen == Screen::Artboard {
        spans.push(Span::styled(
            "by github.com/mevanlc ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        let hints: &[(&str, &str)] = if ctx.artboard_interacting {
            &[
                ("active", "draw"),
                ("Space", "drop"),
                ("Esc", "view"),
                ("Ctrl+\\", "owners"),
                ("Ctrl+P", "help"),
            ]
        } else {
            &[
                ("view", "pan"),
                ("Alt+arrows/R-drag", "pan"),
                ("i", "edit"),
                ("g", "gallery"),
            ]
        };
        for (key, desc) in hints {
            spans.push(Span::styled("· ", Style::default().fg(theme::BORDER_DIM())));
            spans.push(Span::styled(
                *key,
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {desc} "),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    }

    Line::from(spans)
}

fn append_arcade_title_extras(spans: &mut Vec<Span<'static>>, ctx: &DrawContext<'_>) {
    spans.push(Span::styled("· ", Style::default().fg(theme::TEXT_DIM())));
    spans.push(Span::styled(
        format!(
            "{} ",
            crate::app::arcade::ui::game_title(ctx.game_selection)
        ),
        Style::default().fg(theme::TEXT_BRIGHT()),
    ));
}

fn append_home_title_extras(spans: &mut Vec<Span<'static>>, ctx: &DrawContext<'_>) {
    if let Some(label) = chat::ui::home_title_room_label(&ctx.chat_view) {
        spans.push(Span::styled("· ", Style::default().fg(theme::TEXT_DIM())));
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ));
    }
}

fn append_rooms_title_extras(spans: &mut Vec<Span<'static>>, ctx: &DrawContext<'_>) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());

    if let Some(room) = ctx.rooms_active_room {
        spans.push(Span::styled("· ", dim));
        spans.push(Span::styled(room.display_name.clone(), bright));
        if let Some(details) = ctx.active_room_game.and_then(|game| game.title_details()) {
            if let Some(seated) = details.seated {
                spans.push(Span::styled(" · ", dim));
                spans.push(Span::styled(seated, dim));
            }
            if let Some(role) = details.role {
                spans.push(Span::styled(" · ", dim));
                spans.push(Span::styled(role, dim));
            }
            if let Some(balance) = details.balance {
                spans.push(Span::styled(" · ", dim));
                spans.push(Span::styled("Bal ", dim));
                spans.push(Span::styled(format!("{} ", balance), amber));
            }
        }
    } else {
        let real_count = ctx.rooms_snapshot.rooms.len();
        let open = ctx
            .rooms_snapshot
            .rooms
            .iter()
            .filter(|r| r.status == "open")
            .count();
        spans.push(Span::styled("· ", dim));
        spans.push(Span::styled(format!("{real_count} live"), dim));
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(format!("{open} open "), dim));
    }
}

fn app_frame_sponsor_title() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " thanks for hanging out ",
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled("☕ ", Style::default().fg(theme::AMBER())),
        Span::styled(
            "https://ko-fi.com/mateuszpiorowski ",
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ])
    .right_aligned()
}

fn app_frame_help_hint_title() -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let sep = Style::default().fg(theme::TEXT_FAINT());
    Line::from(vec![
        Span::styled(" Settings ", dim),
        Span::styled("Ctrl+O", key),
        Span::styled(" · ", sep),
        Span::styled("Hub ", dim),
        Span::styled("Ctrl+G", key),
        Span::styled(" · ", sep),
        Span::styled("FAQ ", dim),
        Span::styled("Ctrl+L", key),
        Span::styled(" · ", sep),
        Span::styled("Guide ", dim),
        Span::styled("? ", key),
    ])
}

fn mentions_hud_title(unread: i64) -> Option<Line<'static>> {
    if unread <= 0 {
        return None;
    }
    let noun = if unread == 1 { "mention" } else { "mentions" };
    Some(
        Line::from(vec![
            Span::styled(
                format!(" {unread}"),
                Style::default()
                    .fg(theme::MENTION())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" unread {noun} "),
                Style::default().fg(theme::TEXT_MUTED()),
            ),
        ])
        .right_aligned(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        NotificationMode, dashboard_home_selected, desktop_notification_bytes, lounge_info_enabled,
        mentions_hud_title, room_list_sidebar_enabled, sidebar_enabled,
    };
    use uuid::Uuid;

    #[test]
    fn desktop_notification_bytes_both_mode_with_bell_emits_osc_777_and_osc_9() {
        let got = String::from_utf8(desktop_notification_bytes(
            "DM title",
            "hello",
            NotificationMode::Both,
            true,
        ))
        .expect("valid utf8");
        assert_eq!(
            got,
            "\x1b]777;notify;DM title;hello\x1b\\\x1b]9;DM title: hello\x1b\\\x07"
        );
    }

    #[test]
    fn desktop_notification_bytes_osc777_mode_emits_only_osc_777() {
        let got = String::from_utf8(desktop_notification_bytes(
            "DM title",
            "hello",
            NotificationMode::Osc777,
            false,
        ))
        .expect("valid utf8");
        assert_eq!(got, "\x1b]777;notify;DM title;hello\x1b\\");
    }

    #[test]
    fn desktop_notification_bytes_osc9_mode_emits_only_osc_9() {
        let got = String::from_utf8(desktop_notification_bytes(
            "DM title",
            "hello",
            NotificationMode::Osc9,
            false,
        ))
        .expect("valid utf8");
        assert_eq!(got, "\x1b]9;DM title: hello\x1b\\");
    }

    #[test]
    fn desktop_notification_bytes_sanitize_control_bytes_and_separators() {
        let got = String::from_utf8(desktop_notification_bytes(
            "hey;\x07",
            "a\nb\x1bc",
            NotificationMode::Both,
            false,
        ))
        .expect("valid utf8");
        assert_eq!(
            got,
            "\x1b]777;notify;hey| ;a b c\x1b\\\x1b]9;hey| : a b c\x1b\\"
        );
    }

    #[test]
    fn sidebar_enabled_prefers_settings_draft_while_modal_is_open() {
        assert!(!sidebar_enabled(true, false, true));
        assert!(sidebar_enabled(true, true, false));
    }

    #[test]
    fn sidebar_enabled_uses_saved_profile_when_modal_is_closed() {
        assert!(sidebar_enabled(false, false, true));
        assert!(!sidebar_enabled(false, true, false));
    }

    #[test]
    fn room_list_sidebar_enabled_prefers_settings_draft_while_modal_is_open() {
        assert!(!room_list_sidebar_enabled(true, false, true));
        assert!(room_list_sidebar_enabled(true, true, false));
    }

    #[test]
    fn room_list_sidebar_enabled_uses_saved_profile_when_modal_is_closed() {
        assert!(room_list_sidebar_enabled(false, false, true));
        assert!(!room_list_sidebar_enabled(false, true, false));
    }

    #[test]
    fn lounge_info_enabled_prefers_settings_draft_while_modal_is_open() {
        assert!(!lounge_info_enabled(true, false, true));
        assert!(lounge_info_enabled(true, true, false));
    }

    #[test]
    fn lounge_info_enabled_uses_saved_profile_when_modal_is_closed() {
        assert!(lounge_info_enabled(false, false, true));
        assert!(!lounge_info_enabled(false, true, false));
    }

    #[test]
    fn dashboard_home_selected_for_general_room_without_synthetic_entry() {
        let general = Uuid::from_u128(1);
        assert!(dashboard_home_selected(Some(general), Some(general), false));
    }

    #[test]
    fn dashboard_home_selected_rejects_synthetic_and_non_general_rooms() {
        let general = Uuid::from_u128(1);
        let topic = Uuid::from_u128(2);
        assert!(!dashboard_home_selected(Some(general), Some(general), true));
        assert!(!dashboard_home_selected(Some(general), Some(topic), false));
        assert!(!dashboard_home_selected(None, Some(topic), false));
    }

    #[test]
    fn mentions_hud_title_hidden_when_unread_is_zero_or_negative() {
        assert!(mentions_hud_title(0).is_none());
        assert!(mentions_hud_title(-3).is_none());
    }

    #[test]
    fn mentions_hud_title_renders_right_aligned_pluralized_text() {
        use ratatui::layout::Alignment;

        let one = mentions_hud_title(1).expect("one mention should render");
        assert_eq!(one.alignment, Some(Alignment::Right));
        let text: String = one.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, " 1 unread mention ");

        let many = mentions_hud_title(14).expect("many mentions should render");
        let text: String = many.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, " 14 unread mentions ");
    }
}
