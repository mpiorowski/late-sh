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
use unicode_width::UnicodeWidthStr;

use late_core::models::leaderboard::LeaderboardData;
use late_core::models::user::RightSidebarMode;

use super::{
    announcements, artboard,
    audio::{client_state::ClientAudioState, viz::Visualizer},
    bonsai, chat,
    common::{
        primitives::{Banner, BannerKind, Screen, draw_banner},
        sidebar::{SidebarProps, draw_sidebar, sidebar_clock_text},
        theme,
    },
    dashboard, help_modal, icon_picker, mod_modal, profile_modal, quit_confirm, room_search_modal,
    settings_modal, sheet_modal,
    state::App,
};
use crate::app::door::game::DoorGame;
use crate::app::files::terminal_image::TerminalImageFrame;

fn sidebar_enabled(show_settings: bool, draft_enabled: bool, profile_enabled: bool) -> bool {
    if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

/// Map a top-level screen to its 1-based page number.
pub(crate) fn screen_number(screen: Screen) -> u8 {
    match screen {
        Screen::Dashboard => 1,
        Screen::Arcade => 2,
        Screen::Rooms => 3,
        Screen::Artboard => 4,
        Screen::Lateania => 5,
        Screen::Rebels => 6,
        Screen::Pinstar => 7,
    }
}

fn right_sidebar_allowed_on_screen(screen: Screen) -> bool {
    matches!(screen, Screen::Dashboard | Screen::Arcade | Screen::Rooms)
}

/// Resolve whether the right sidebar should render on `screen` given a profile
/// (or draft) sidebar mode and per-screen visibility set.
pub(crate) fn resolve_right_sidebar_enabled(
    mode: RightSidebarMode,
    screens: &[u8],
    screen: Screen,
) -> bool {
    if !right_sidebar_allowed_on_screen(screen) {
        return false;
    }

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

fn room_top_boxes_enabled(
    show_settings: bool,
    draft_enabled: bool,
    profile_enabled: bool,
    home_selected: bool,
    room_selected: bool,
) -> bool {
    if home_selected {
        true
    } else if !room_selected {
        false
    } else if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

fn dashboard_home_selected(
    lounge_room_id: Option<uuid::Uuid>,
    selected_room_id: Option<uuid::Uuid>,
    synthetic_selected: bool,
) -> bool {
    lounge_room_id.is_some_and(|lounge| selected_room_id == Some(lounge)) && !synthetic_selected
}

/// Push the quit-confirm sayonara pixel scene into the current frame's
/// terminal-image frame. No-op when the session has no detected image
/// protocol or when the modal is too small to hold the scene — the
/// non-image render still draws the existing prompt + footer.
fn push_quit_confirm_sayonara_placement(
    modal_area: Rect,
    protocol: Option<crate::app::files::terminal_image::TerminalImageProtocol>,
    terminal_images: &mut crate::app::files::terminal_image::TerminalImageFrame,
) {
    use crate::app::files::terminal_image::TerminalImagePlacement;
    use crate::app::quit_confirm::sayonara_sixel::sayonara_terminal_image;
    use crate::app::quit_confirm::ui::sayonara_scene_area;

    let Some(protocol) = protocol else {
        return;
    };
    let Some(area) = sayonara_scene_area(modal_area) else {
        return;
    };
    let data = match sayonara_terminal_image(protocol) {
        Ok(data) => data,
        Err(err) => {
            tracing::trace!("sayonara image unavailable: {err:?}");
            return;
        }
    };
    if !data.supports_protocol(protocol) {
        return;
    }
    // Stable UUID so consecutive frames within the modal's open lifetime
    // hash to the same placement key and skip redundant byte emission.
    let message_id = uuid::Uuid::from_u128(0x4C40_6A41_BAB7_0000_0000_0000_0000_0001);
    terminal_images.push(TerminalImagePlacement {
        message_id,
        area,
        data: (*data).clone(),
    });
}

struct DrawContext<'a> {
    dashboard_view: dashboard::ui::DashboardRenderInput<'a>,
    chat_view: chat::ui::ChatRenderInput<'a>,
    game_selection: usize,
    is_playing_game: bool,
    door_delete_confirm: bool,
    rooms_create_flow: Option<&'a crate::app::rooms::backend::CreateRoomFlow>,
    rooms_snapshot: &'a crate::app::rooms::svc::RoomsSnapshot,
    rooms_selected_index: usize,
    rooms_active_room: Option<&'a crate::app::rooms::svc::RoomListItem>,
    rooms_filter: crate::app::rooms::filter::RoomsFilter,
    rooms_search_active: bool,
    rooms_search_query: &'a str,
    rooms_usernames: &'a crate::usernames::UsernameLookup<'a>,
    room_game_registry: &'a crate::app::rooms::registry::RoomGameRegistry,
    active_room_game: Option<&'a dyn crate::app::rooms::backend::ActiveRoomBackend>,
    rooms_chat_view: Option<chat::ui::EmbeddedRoomChatView<'a>>,
    lateania_state: Option<&'a crate::app::door::lateania::state::State>,
    rebels_state: Option<&'a mut crate::app::door::rebels::state::State>,
    /// Detected terminal-image protocol for the current session.
    /// `None` -> no native images supported; capable terminals get
    /// pixel polish on top of the existing text rendering.
    terminal_image_protocol: Option<crate::app::files::terminal_image::TerminalImageProtocol>,
    twenty_forty_eight_state: &'a crate::app::arcade::twenty_forty_eight::state::State,
    tetris_state: &'a crate::app::arcade::tetris::state::State,
    snake_state: &'a crate::app::arcade::snake::state::State,
    sudoku_state: &'a crate::app::arcade::sudoku::state::State,
    nonogram_state: &'a crate::app::arcade::nonogram::state::State,
    solitaire_state: &'a crate::app::arcade::solitaire::state::State,
    minesweeper_state: &'a crate::app::arcade::minesweeper::state::State,
    nes_cabinet_state: &'a crate::app::arcade::nes_cabinet::state::State,
    dartboard_state: Option<&'a crate::app::artboard::state::State>,
    directory_tab: crate::app::directory::state::DirectoryTab,
    pinstar_state: Option<&'a mut crate::app::pinstar::state::PinstarState>,
    pinstar_browser: Option<&'a crate::app::pinstar::browser::DiagramBrowser>,
    artboard_interacting: bool,
    leaderboard: &'a Arc<LeaderboardData>,
    visualizer: &'a Visualizer,
    now_playing: Option<&'a NowPlaying>,
    paired_client: Option<&'a ClientAudioState>,
    sidebar_clock: &'a str,
    bonsai: &'a crate::app::bonsai::state::BonsaiState,
    bonsai_v2: &'a crate::app::bonsai_v2::state::BonsaiV2State,
    cat: &'a crate::app::pet::state::PetState,
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
    show_aquarium_tray: bool,
    aquarium_state: &'a crate::app::hub::aquarium::state::AquariumState,
    hub_state: &'a crate::app::hub::state::HubState,
    quest_state: &'a crate::app::hub::dailies::state::QuestState,
    shop_state: &'a crate::app::hub::shop::state::ShopState,
    hub_admin_state: &'a crate::app::hub::admin::state::AdminState,
    mod_modal_state: &'a mod_modal::state::ModModalState,
    show_profile_modal: bool,
    profile_modal_state: &'a profile_modal::state::ProfileModalState,
    show_sheet_modal: bool,
    sheet_modal_state: &'a sheet_modal::state::SheetModalState,
    show_poll_modal: bool,
    poll_modal_state: &'a chat::polls::state::PollModalState,
    show_bonsai_modal: bool,
    show_bonsai_v2_modal: bool,
    bonsai_care_state: &'a bonsai::care::BonsaiCareState,
    show_cat_modal: bool,
    login_announcements: Option<&'a announcements::LoginAnnouncements>,
    show_help: bool,
    help_modal_state: &'a help_modal::state::HelpModalState,
    show_ultimate_modal: bool,
    ultimate_state: &'a crate::app::ultimates::UltimateState,
    show_splash: bool,
    splash_ticks: usize,
    splash_hint: &'a str,
    pair_url: &'a str,
    room_search_modal_open: bool,
    room_search_modal_state: &'a room_search_modal::state::RoomSearchModalState,
    booth_modal_open: bool,
    booth_modal_state: &'a crate::app::audio::booth::state::BoothModalState,
    booth_snapshot: crate::app::audio::svc::QueueSnapshot,
    booth_submit_enabled: bool,
    youtube_source_count: usize,
    icecast_source_count: usize,
    radio_source_count: usize,
    paired_browser_source: late_core::models::user::AudioSource,
    selected_icecast_stream: late_core::models::user::IcecastStream,
    selected_radio_station: late_core::models::user::RadioStation,
    radio_now_playing: Option<&'a str>,
    afk: Option<&'a str>,
    chat_state: &'a chat::state::ChatState,
    user_id: uuid::Uuid,
    pet_species: &'a str,
    news_modal: Option<chat::news::ui::ArticleModalView<'a>>,
    is_draining: bool,
    icon_picker_open: bool,
    icon_picker_state: &'a icon_picker::IconPickerState,
    icon_catalog: Option<&'a icon_picker::catalog::IconCatalogData>,
    mentions_unread_count: i64,
    home_selected: bool,
}

impl App {
    pub fn render(&mut self) -> anyhow::Result<Vec<u8>> {
        // Clear last-frame mouse hit-test rects so screens that don't draw
        // them this frame can't leave a stale target behind.
        self.last_dashboard_activity_rect.set(None);
        self.chat.last_composer_rect.set(None);
        // `last_composer_viewport_top` is intentionally NOT reset here: it
        // replays ratatui-textarea's minimal-scroll rule, which needs the
        // previous frame's top to know when the viewport stays put. Clearing
        // it every frame would bottom-anchor the reconstruction at the cursor
        // and desync it from the widget's real (persistent) viewport whenever
        // the cursor moves up inside the visible window.
        self.chat.last_chat_hit_layout.set(None);

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
        let ultimate_effects = self.ultimate_state.active_theme_effects();
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
        let login_announcements_visible = self.login_announcements_visible();
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
        let shell_active_room = self.chat.selected_room_id;
        let synthetic_selected = self.chat.feeds_selected
            || self.chat.news_selected
            || self.chat.notifications_selected
            || self.chat.discover_selected
            || self.chat.showcase_selected
            || self.chat.work_selected;
        let home_selected = dashboard_home_selected(
            self.chat.lounge_room_id(),
            shell_active_room,
            synthetic_selected,
        );
        let room_selected = shell_active_room.is_some() && !synthetic_selected;
        let show_room_top_boxes = room_top_boxes_enabled(
            self.show_settings,
            self.settings_modal_state.draft().show_dashboard_header,
            self.profile_state.profile().show_dashboard_header,
            home_selected,
            room_selected,
        );
        let screen = self.screen;
        // The icecast rows render the USER'S SELECTED stream's track, not a
        // global single mount.
        let selected_icecast_stream = self.selected_icecast_stream;
        let now_playing: Option<NowPlaying> = self.now_playing_rx.as_mut().and_then(|rx| {
            rx.borrow_and_update()
                .get(selected_icecast_stream.as_str())
                .cloned()
        });
        let selected_radio_station = self.selected_radio_station;
        let radio_now_playing: Option<String> = self.radio_meta_rx.as_mut().and_then(|rx| {
            rx.borrow_and_update()
                .get(selected_radio_station.as_str())
                .map(|meta| format!("{} - {}", meta.artist, meta.title))
        });
        let paired_client = self.paired_client_state();
        let paired_cli_supports_voice = self.paired_cli_supports_voice();
        let banner = self.active_banner().cloned();
        let sidebar_clock = sidebar_clock_text(self.profile_state.profile().timezone.as_deref());
        let visualizer = &self.visualizer;
        self.chat
            .request_image_modal_terminal_image(self.terminal_image_protocol);
        let username_directory_snapshot = self
            .username_directory
            .as_ref()
            .map(crate::usernames::snapshot);
        let render_usernames = crate::usernames::UsernameLookup::new(
            self.chat.usernames(),
            username_directory_snapshot.as_deref(),
        );
        let chat_usernames = &render_usernames;
        let chat_countries = self.chat.countries();
        let bonsai_glyphs = self.chat.bonsai_glyphs();
        let chat_badges = self.chat.chat_badges();
        let profile_award_badges = self.chat.profile_award_badges();
        let message_reactions = self.chat.message_reactions();
        let voice_snapshot = self.voice.snapshot();
        let online_count = self
            .active_users
            .as_ref()
            .map(|active_users| active_users.lock_recover().len())
            .unwrap_or(0);
        self.afk_user_ids = crate::state::afk_users_snapshot(&self.afk_users);
        let image_modal = self
            .chat
            .image_modal()
            .map(|modal| chat::ui::ImageModalView {
                message_id: modal.message_id,
                url: modal.url.as_str(),
                preview: self.chat.inline_image_cache.get(&modal.message_id),
                terminal_image: self.terminal_image_protocol.and_then(|protocol| {
                    self.chat
                        .terminal_image_for_message(modal.message_id)
                        .filter(|image| image.supports_protocol(protocol))
                }),
                terminal_image_protocol: self.terminal_image_protocol,
            });
        let multiplayer_rooms = dashboard::ui::recent_dashboard_rooms(
            &self.rooms_snapshot,
            &self.room_game_registry,
            &self.dashboard_room_joins,
            4,
        );
        let dashboard_cycle_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let dashboard_messages = shell_active_room
            .map(|room_id| self.chat.messages_for_room(room_id))
            .unwrap_or(&[]);
        let active_friend_names = self.chat.active_friend_names();
        let dashboard_selected_news_message = shell_active_room
            .is_some_and(|room_id| self.chat.selected_message_is_news_in_room(room_id));
        let dashboard_selected_image_message = shell_active_room
            .is_some_and(|room_id| self.chat.selected_message_has_inline_image_in_room(room_id));
        let dashboard_room_effects = shell_active_room
            .and_then(|room_id| self.shop_state.active_room_effects().get(&room_id))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let dashboard_active_poll =
            shell_active_room.and_then(|room_id| self.chat.active_poll_for_room(room_id));
        let dashboard_voice_channel_id = shell_active_room
            .and_then(|room_id| self.chat.voice_channels_by_room_id.get(&room_id))
            .map(|channel| channel.id);
        let dashboard_view = dashboard::ui::DashboardRenderInput {
            activity: &self.activity,
            online_count,
            active_friend_names: &active_friend_names,
            multiplayer_rooms: &multiplayer_rooms,
            quest_snapshot: self.quest_state.snapshot(),
            dashboard_cycle_secs,
            show_room_top_boxes,
            pinned_messages: self.chat.pinned_messages(),
            chat_view: chat::ui::DashboardChatView {
                messages: dashboard_messages,
                overlay: self.chat.overlay(),
                image_modal,
                rows_cache: &mut self.dashboard_chat_rows_cache,
                usernames: chat_usernames,
                countries: chat_countries,
                friend_user_ids: self.chat.friend_user_ids(),
                afk_user_ids: self.afk_user_ids.as_ref(),
                message_reactions,
                current_user_id: self.user_id,
                voice_channel_id: dashboard_voice_channel_id,
                voice_snapshot,
                voice_paired_cli_supports_voice: paired_cli_supports_voice,
                show_flag_fallback: self.profile_state.profile().show_flag_fallback,
                selected_message_id: self.chat.selected_message_id,
                selected_image_message: dashboard_selected_image_message,
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
                chat_badges,
                profile_award_badges,
                bot_username_color_active: self.shop_state.bot_username_color_active(),
                active_room_effects: dashboard_room_effects,
                active_poll: dashboard_active_poll,
                inline_images: &self.chat.inline_image_cache,
                keep_composer_focused: self.profile_state.profile().keep_composer_focused,
                composer_rect_slot: Some(&self.chat.last_composer_rect),
                composer_viewport_top_slot: Some(&self.chat.last_composer_viewport_top),
                chat_hit_slot: Some(&self.chat.last_chat_hit_layout),
            },
            activity_scroll: self.dashboard_activity_scroll,
            activity_rect_slot: Some(&self.last_dashboard_activity_rect),
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
        let selected_image_message = self
            .chat
            .selected_room_id
            .is_some_and(|room_id| self.chat.selected_message_has_inline_image_in_room(room_id));
        let selected_room_active_poll = if self.chat.selected_bumped_join_room_id().is_none()
            && !self.chat.feeds_selected
            && !self.chat.news_selected
            && !self.chat.discover_selected
            && !self.chat.notifications_selected
            && !self.chat.showcase_selected
            && !self.chat.work_selected
        {
            self.chat
                .selected_room_id
                .and_then(|room_id| self.chat.active_poll_for_room(room_id))
        } else {
            None
        };
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
            image_modal,
            usernames: chat_usernames,
            countries: chat_countries,
            friend_user_ids: self.chat.friend_user_ids(),
            afk_user_ids: self.afk_user_ids.as_ref(),
            message_reactions,
            inline_images: &self.chat.inline_image_cache,
            unread_counts: &self.chat.unread_counts,
            room_last_message_at: &self.chat.room_last_message_at,
            favorite_room_ids: &self.profile_state.profile().favorite_room_ids,
            active_room_effects: self.shop_state.active_room_effects(),
            active_poll: selected_room_active_poll,
            collapsed_sections: &self.chat.collapsed_sections,
            selected_room_id: self.chat.selected_room_id,
            selected_bumped_join_room_id: self.chat.selected_bumped_join_room_id(),
            room_jump_active: self.chat.room_jump_active,
            room_section_prefix_armed: self.room_section_prefix_armed,
            selected_message_id: self.chat.selected_message_id,
            selected_image_message,
            selected_news_message,
            reaction_picker_active: self.chat.is_reaction_leader_active(),
            highlighted_message_id: self.chat.highlighted_message_id,
            composer: self.chat.composer(),
            composing: self.chat.composing,
            current_user_id: self.user_id,
            show_flag_fallback: self.profile_state.profile().show_flag_fallback,
            cursor_visible: self.chat.cursor_visible(),
            mention_matches: &self.chat.mention_ac.matches,
            mention_selected: self.chat.mention_ac.selected,
            mention_active: self.chat.mention_ac.active,
            reply_author: self.chat.reply_target().map(|reply| reply.author.as_str()),
            is_editing: self.chat.edited_message_id.is_some(),
            bonsai_glyphs,
            chat_badges,
            profile_award_badges,
            bot_username_color_active: self.shop_state.bot_username_color_active(),
            news_composer: self.chat.news.composer(),
            news_composing: self.chat.news.composing(),
            news_processing: self.chat.news.processing(),
            notifications_selected: self.chat.notifications_selected,
            notifications_unread_count: self.chat.notifications.unread_count(),
            notifications_view,
            voice_channels_by_room_id: &self.chat.voice_channels_by_room_id,
            voice_snapshot,
            voice_paired_cli_supports_voice: paired_cli_supports_voice,
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
            keep_composer_focused: self.profile_state.profile().keep_composer_focused,
            composer_rect_slot: Some(&self.chat.last_composer_rect),
            composer_viewport_top_slot: Some(&self.chat.last_composer_viewport_top),
            chat_hit_slot: Some(&self.chat.last_chat_hit_layout),
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
                    image_modal,
                    rows_cache: &mut self.rooms_chat_rows_cache,
                    usernames: chat_usernames,
                    countries: chat_countries,
                    friend_user_ids: self.chat.friend_user_ids(),
                    afk_user_ids: self.afk_user_ids.as_ref(),
                    message_reactions,
                    inline_images: &self.chat.inline_image_cache,
                    current_user_id: self.user_id,
                    voice_channel_id: room.voice_channel_id,
                    voice_snapshot,
                    voice_paired_cli_supports_voice: paired_cli_supports_voice,
                    show_flag_fallback: self.profile_state.profile().show_flag_fallback,
                    selected_message_id: self.chat.selected_message_id,
                    selected_image_message: self
                        .chat
                        .selected_message_has_inline_image_in_room(room.chat_room_id),
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
                    chat_badges,
                    profile_award_badges,
                    keep_composer_focused: self.profile_state.profile().keep_composer_focused,
                    composer_rect_slot: Some(&self.chat.last_composer_rect),
                    composer_viewport_top_slot: Some(&self.chat.last_composer_viewport_top),
                    chat_hit_slot: Some(&self.chat.last_chat_hit_layout),
                });
        let mut terminal_image_frame = TerminalImageFrame::default();

        // Sixel cleanup, pre-frame phase. Sixel — unlike Kitty — has no
        // delete-by-id protocol, so prior pixels persist on the terminal
        // raster layer until the cells underneath are written to. Compute
        // the wipe HERE so the bytes land in `shared` BEFORE ratatui's frame
        // diff. ratatui's normal cell writes then overwrite the wiped area
        // with the correct new content. See `pre_frame_sixel_wipe_bytes`.
        //
        // Read each modal flag individually instead of passing `self` to a
        // helper — `dashboard_view` already holds `&mut self.dashboard_chat_rows_cache`
        // so the borrow checker rejects an `&self` reborrow here.
        let image_modal_msg_id = self.chat.image_modal().map(|m| m.message_id);
        let overlay_blocks_sixel = self.show_settings
            || self.show_quit_confirm
            || self.show_mod_modal
            || self.show_hub_modal
            || self.show_aquarium_tray
            || self.show_profile_modal
            || self.show_sheet_modal
            || self.show_poll_modal
            || self.show_bonsai_modal
            || self.show_bonsai_v2_modal
            || self.show_cat_modal
            || login_announcements_visible
            || self.show_help
            || self.show_ultimate_modal
            || self.show_splash
            || news_modal.is_some()
            || self.icon_picker_open
            || self.room_search_modal_state.is_open()
            || self.booth_modal_state.is_open();
        let suppress_new_sixel = self.show_settings
            || self.show_mod_modal
            || self.show_hub_modal
            || self.show_aquarium_tray
            || self.show_profile_modal
            || self.show_sheet_modal
            || self.show_poll_modal
            || self.show_bonsai_modal
            || self.show_bonsai_v2_modal
            || self.show_cat_modal
            || login_announcements_visible
            || self.show_help
            || self.show_ultimate_modal
            || self.show_splash
            || news_modal.is_some()
            || self.icon_picker_open
            || self.room_search_modal_state.is_open()
            || self.booth_modal_state.is_open();
        let pre_wipe = self
            .terminal_image_render_state
            .pre_frame_sixel_wipe_bytes(image_modal_msg_id, overlay_blocks_sixel);
        if !pre_wipe.is_empty() {
            use std::io::Write;
            let _ = self.shared.write_all(&pre_wipe);
        }

        let terminal = &mut self.terminal;
        let mut pinstar_state_taken = self.pinstar_state.take();
        // Taken out (like pinstar_state) so the draw dispatch can hold &mut and
        // call set_viewport with the exact content_area before blitting.
        let mut rebels_state_taken = self.rebels_state.take();

        let pinstar_browser = if screen == Screen::Pinstar {
            Some(&self.pinstar_browser)
        } else {
            None
        };
        let draw_result = terminal
            .draw(|frame| {
                Self::draw(
                    frame,
                    area,
                    screen,
                    DrawContext {
                        dashboard_view,
                        chat_view,
                        game_selection: self.game_selection,
                        is_playing_game: self.is_playing_game,
                        door_delete_confirm: self.door_delete_confirm,
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
                        lateania_state: self.lateania_state.as_ref(),
                        rebels_state: rebels_state_taken.as_mut(),
                        terminal_image_protocol: self.terminal_image_protocol,
                        twenty_forty_eight_state: &self.twenty_forty_eight_state,
                        tetris_state: &self.tetris_state,
                        snake_state: &self.snake_state,
                        sudoku_state: &self.sudoku_state,
                        nonogram_state: &self.nonogram_state,
                        solitaire_state: &self.solitaire_state,
                        minesweeper_state: &self.minesweeper_state,
                        nes_cabinet_state: &self.nes_cabinet_state,
                        dartboard_state: self.dartboard_state.as_ref(),
                        directory_tab: self.directory_state.tab,
                        pinstar_state: pinstar_state_taken.as_mut(),
                        pinstar_browser,
                        artboard_interacting: self.artboard_interacting,
                        leaderboard: &self.leaderboard,
                        visualizer,
                        now_playing: now_playing.as_ref(),
                        paired_client: paired_client.as_ref(),
                        sidebar_clock: &sidebar_clock,
                        bonsai: &self.bonsai_state,
                        bonsai_v2: &self.bonsai_v2_state,
                        cat: &self.pet_state,
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
                        show_aquarium_tray: self.show_aquarium_tray,
                        aquarium_state: &self.aquarium_state,
                        hub_state: &self.hub_state,
                        quest_state: &self.quest_state,
                        shop_state: &self.shop_state,
                        hub_admin_state: &self.hub_admin_state,
                        mod_modal_state: &self.mod_modal_state,
                        show_profile_modal: self.show_profile_modal,
                        profile_modal_state: &self.profile_modal_state,
                        show_sheet_modal: self.show_sheet_modal,
                        sheet_modal_state: &self.sheet_modal_state,
                        show_poll_modal: self.show_poll_modal,
                        poll_modal_state: &self.poll_modal_state,
                        show_bonsai_modal: self.show_bonsai_modal,
                        show_bonsai_v2_modal: self.show_bonsai_v2_modal,
                        bonsai_care_state: &self.bonsai_care_state,
                        show_cat_modal: self.show_cat_modal,
                        login_announcements: if login_announcements_visible {
                            self.login_announcements.as_ref()
                        } else {
                            None
                        },
                        show_help: self.show_help,
                        help_modal_state: &self.help_modal_state,
                        show_ultimate_modal: self.show_ultimate_modal,
                        ultimate_state: &self.ultimate_state,
                        show_splash: self.show_splash,
                        splash_ticks: self.splash_ticks,
                        splash_hint: &self.splash_hint,
                        pair_url: &self.connect_url,
                        room_search_modal_open: self.room_search_modal_state.is_open(),
                        room_search_modal_state: &self.room_search_modal_state,
                        booth_modal_open: self.booth_modal_state.is_open(),
                        booth_modal_state: &self.booth_modal_state,
                        booth_snapshot: self.audio.queue_snapshot(),
                        booth_submit_enabled: self.audio.booth_submit_enabled(),
                        youtube_source_count: self.audio.youtube_source_count(),
                        icecast_source_count: self.audio.icecast_source_count(),
                        radio_source_count: self.audio.radio_source_count(),
                        paired_browser_source: self.paired_browser_source,
                        selected_icecast_stream,
                        selected_radio_station,
                        radio_now_playing: radio_now_playing.as_deref(),
                        afk: self.afk.as_deref(),
                        chat_state: &self.chat,
                        user_id: self.user_id,
                        pet_species: &self.pet_state.species,
                        news_modal,
                        is_draining: self.is_draining.load(std::sync::atomic::Ordering::Relaxed),
                        icon_picker_open: self.icon_picker_open,
                        icon_picker_state: &self.icon_picker_state,
                        icon_catalog: self.icon_catalog.as_ref(),
                        mentions_unread_count: self.chat.notifications.unread_count(),
                        home_selected,
                    },
                    &mut terminal_image_frame,
                );
                for effect in ultimate_effects {
                    crate::app::ultimates::apply_ultimate_postprocess(frame.buffer_mut(), effect);
                }
            })
            .context("failed to draw frame");

        self.pinstar_state = pinstar_state_taken;
        self.rebels_state = rebels_state_taken;
        draw_result?;

        // Feed the modal's image capacity (recorded during draw) back into
        // chat state so the next frame's Sixel fetch encodes to fit.
        self.chat
            .set_image_modal_capacity(terminal_image_frame.modal_capacity());

        let image_commands = self.terminal_image_render_state.build_commands(
            self.terminal_image_protocol,
            &terminal_image_frame,
            suppress_new_sixel,
        );
        self.pending_terminal_commands.extend(image_commands);

        // Emit OSC 52 clipboard sequence if a copy was requested.
        // Format: \x1b]52;c;<base64>\x07
        if let Some(text) = self.pending_clipboard.take() {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
            self.pending_terminal_commands
                .push(format!("\x1b]52;c;{}\x07", encoded).into_bytes());
        }

        // Emit OSC 777/OSC 9 desktop notifications queued by producers this
        // tick; the outbox applies notify_kinds, cooldown, format, and bell.
        if let Some(payload) = self.notify_outbox.drain(self.profile_state.profile()) {
            self.pending_terminal_commands.push(payload);
        }

        Ok(self.shared.take())
    }

    fn active_banner(&self) -> Option<&Banner> {
        self.banner.as_ref().filter(|b| b.is_active())
    }

    fn draw(
        frame: &mut Frame,
        area: Rect,
        screen: Screen,
        ctx: DrawContext<'_>,
        terminal_images: &mut TerminalImageFrame,
    ) {
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

        let title = app_frame_title(screen, &ctx);
        let mut block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
        if let Some(hud) = mentions_hud_title(ctx.mentions_unread_count) {
            block = block.title_top(hud);
        }
        let (help_hint_title, sponsor_title) = app_frame_bottom_titles(area.width);
        block = block.title_bottom(help_hint_title);
        if let Some(sponsor_title) = sponsor_title {
            block = block.title_bottom(sponsor_title);
        }

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Clear, inner);

        let (app_inner, aquarium_tray_area) =
            if ctx.show_aquarium_tray && ctx.shop_state.entitlements().has_aquarium() {
                let tray = crate::app::hub::aquarium::ui::bottom_tray_area(inner);
                (
                    Rect::new(
                        inner.x,
                        inner.y,
                        inner.width,
                        inner.height.saturating_sub(tray.height),
                    ),
                    Some(tray),
                )
            } else {
                (inner, None)
            };

        let (content_area, sidebar_area) = if ctx.show_right_sidebar {
            let main_layout =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(app_inner);
            (main_layout[0], Some(main_layout[1]))
        } else {
            (app_inner, None)
        };
        let foreground_overlay_open = foreground_terminal_overlay_open(&ctx);
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
                    dashboard::ui::draw_dashboard(
                        frame,
                        center_area,
                        ctx.dashboard_view,
                        terminal_images,
                    );
                } else if ctx.dashboard_view.show_room_top_boxes {
                    dashboard::ui::draw_chat_with_top_strip(
                        frame,
                        center_area,
                        ctx.dashboard_view,
                        terminal_images,
                    );
                } else {
                    chat::ui::draw_chat_center(frame, center_area, ctx.chat_view, terminal_images);
                }
            }
            Screen::Artboard => {
                if let Some(state) = ctx.dartboard_state {
                    artboard::ui::draw_game(frame, content_area, state, ctx.artboard_interacting);
                }
            }
            Screen::Lateania => {
                crate::app::door::lateania::screen::GAME.draw(
                    frame,
                    content_area,
                    &crate::app::door::lateania::screen::LateaniaScreenView {
                        delete_confirm: ctx.door_delete_confirm,
                        state: ctx.lateania_state,
                        usernames: ctx.rooms_usernames,
                        terminal_image_protocol: ctx.terminal_image_protocol,
                    },
                    terminal_images,
                );
            }
            Screen::Rebels => {
                if let Some(state) = ctx.rebels_state {
                    // Size the proxy PTY to the exact widget area before blitting
                    // so the vt100 grid matches what we draw.
                    state.set_viewport(content_area);
                    crate::app::door::rebels::render::draw_page(frame, content_area, state);
                }
            }
            Screen::Pinstar => {
                crate::app::directory::ui::draw_directory_page(
                    frame,
                    content_area,
                    crate::app::directory::ui::DirectoryPageView {
                        tab: ctx.directory_tab,
                        profiles: ctx.chat_view.work_view,
                        work_state: ctx
                            .chat_view
                            .work_state
                            .expect("directory work state is always present"),
                        projects: ctx.chat_view.showcase_view,
                        showcase_state: ctx
                            .chat_view
                            .showcase_state
                            .expect("directory showcase state is always present"),
                        pinstar_state: ctx.pinstar_state,
                        pinstar_browser: ctx.pinstar_browser,
                    },
                );
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
                    nes_cabinet_state: ctx.nes_cabinet_state,
                    daily_completion: ctx.leaderboard.user_daily_statuses.get(&ctx.user_id),
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
                terminal_images,
                ctx.terminal_image_protocol,
            ),
        }

        if let Some(sidebar_area) = sidebar_area {
            draw_sidebar(
                frame,
                sidebar_area,
                &SidebarProps {
                    visualizer: ctx.visualizer,
                    now_playing: ctx.now_playing,
                    paired_client: ctx.paired_client,
                    bonsai: ctx.bonsai,
                    bonsai_v2: ctx.bonsai_v2,
                    use_bonsai_v2: ctx.shop_state.dynamic_bonsai_enabled(),
                    cat: ctx.cat,
                    pet_available: ctx.shop_state.entitlements().has_pet_companion(),
                    audio_beat: ctx.visualizer.beat(),
                    clock_text: ctx.sidebar_clock,
                    queue_snapshot: &ctx.booth_snapshot,
                    youtube_source_count: ctx.youtube_source_count,
                    icecast_source_count: ctx.icecast_source_count,
                    radio_source_count: ctx.radio_source_count,
                    paired_browser_source: ctx.paired_browser_source,
                    selected_icecast_stream: ctx.selected_icecast_stream,
                    selected_radio_station: ctx.selected_radio_station,
                    radio_now_playing: ctx.radio_now_playing,
                    afk: ctx.afk,
                },
            );
        }

        if let Some(aquarium_area) = aquarium_tray_area {
            crate::app::hub::aquarium::ui::draw_bottom_tray(
                frame,
                aquarium_area,
                ctx.aquarium_state,
            );
        }

        if foreground_overlay_open {
            terminal_images.clear();
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
            let toast_w = msg_w.max(20).min(app_inner.width);
            let toast_x = app_inner.x + app_inner.width.saturating_sub(toast_w);
            let toast_area = Rect::new(toast_x, app_inner.y, toast_w, 3);
            frame.render_widget(Clear, toast_area);
            let notif_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));
            let notif_inner = notif_block.inner(toast_area);
            frame.render_widget(notif_block, toast_area);
            draw_banner(frame, notif_inner, &banner);
        }

        if !ctx.show_cat_modal {
            crate::app::pet::ui::draw_roaming_pet(frame, app_inner, ctx.cat);
        }

        if ctx.show_settings {
            settings_modal::ui::draw(frame, inner, ctx.settings_modal_state);
        }

        if ctx.show_mod_modal {
            mod_modal::ui::draw(frame, inner, ctx.mod_modal_state);
        }

        if ctx.show_hub_modal {
            crate::app::hub::ui::draw(
                frame,
                inner,
                crate::app::hub::ui::HubDrawProps {
                    state: ctx.hub_state,
                    quest_state: ctx.quest_state,
                    shop_state: ctx.shop_state,
                    admin_state: ctx.hub_admin_state,
                    leaderboard: ctx.leaderboard,
                    user_id: ctx.user_id,
                    pet_species: ctx.pet_species,
                    is_admin: ctx.is_admin,
                },
            );
        }

        if ctx.show_profile_modal {
            profile_modal::ui::draw(frame, inner, ctx.profile_modal_state);
        }

        if ctx.show_sheet_modal {
            sheet_modal::ui::draw(frame, inner, ctx.sheet_modal_state);
        }

        if ctx.show_poll_modal {
            chat::polls::ui::draw_modal(frame, inner, ctx.poll_modal_state);
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

        if ctx.show_bonsai_v2_modal {
            crate::app::bonsai_v2::modal_ui::draw(
                frame,
                inner,
                ctx.bonsai_v2,
                ctx.visualizer.beat(),
            );
        }

        if ctx.show_cat_modal {
            crate::app::pet::modal_ui::draw(frame, ctx.cat);
        }

        if let Some(modal) = ctx.login_announcements {
            announcements::draw(frame, inner, modal);
        }

        if ctx.show_help {
            help_modal::ui::draw(frame, inner, ctx.help_modal_state, ctx.pair_url);
        }

        if ctx.show_ultimate_modal {
            crate::app::ultimates::draw(frame, inner, ctx.ultimate_state, ctx.shop_state);
        }

        if ctx.show_quit_confirm {
            quit_confirm::ui::draw(frame, inner);
            push_quit_confirm_sayonara_placement(
                inner,
                ctx.terminal_image_protocol,
                terminal_images,
            );
        }

        if let Some(news_modal) = ctx.news_modal {
            chat::news::ui::draw_article_modal(frame, inner, news_modal);
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

        if ctx.booth_modal_open {
            crate::app::audio::booth::ui::draw(
                frame,
                inner,
                ctx.booth_modal_state,
                &ctx.booth_snapshot,
                ctx.booth_submit_enabled,
                ctx.is_admin || ctx.is_moderator,
            );
        }

        if ctx.icon_picker_open
            && let Some(catalog) = ctx.icon_catalog
        {
            icon_picker::picker::render(frame, area, ctx.icon_picker_state, catalog);
        }
    }
}

fn foreground_terminal_overlay_open(ctx: &DrawContext<'_>) -> bool {
    ctx.show_settings
        || ctx.show_quit_confirm
        || ctx.show_mod_modal
        || ctx.show_hub_modal
        || ctx.show_aquarium_tray
        || ctx.show_profile_modal
        || ctx.show_poll_modal
        || ctx.show_bonsai_modal
        || ctx.show_bonsai_v2_modal
        || ctx.show_cat_modal
        || ctx.login_announcements.is_some()
        || ctx.show_help
        || ctx.show_ultimate_modal
        || ctx.news_modal.is_some()
        || ctx.room_search_modal_open
        || ctx.booth_modal_open
        || ctx.icon_picker_open
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
        (Screen::Lateania, "5"),
        (Screen::Rebels, "6"),
        (Screen::Pinstar, "7"),
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
        Screen::Lateania => "Lateania",
        Screen::Rebels => "Rebels",
        Screen::Arcade => "The Arcade",
        Screen::Artboard => "Artboard",
        Screen::Rooms => "Tables",
        Screen::Pinstar => "Directory",
    };
    spans.push(Span::styled(
        " | ",
        Style::default().fg(theme::BORDER_DIM()),
    ));
    spans.push(Span::styled(
        format!("{page_title} "),
        Style::default().fg(theme::TEXT_MUTED()),
    ));

    if screen == Screen::Lateania {
        spans.push(Span::styled(
            "by hardlygospel.github.io ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }

    if screen == Screen::Rebels {
        spans.push(Span::styled(
            "by github.com/ricott1 ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }

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

    if screen == Screen::Pinstar {
        let hints: &[(&str, &str)] = match ctx.directory_tab {
            crate::app::directory::state::DirectoryTab::Profiles => &[
                ("i", "edit mine"),
                ("e", "edit selected"),
                ("Enter", "copy link"),
            ],
            crate::app::directory::state::DirectoryTab::Projects => {
                &[("i", "new"), ("e", "edit"), ("Enter", "copy link")]
            }
            crate::app::directory::state::DirectoryTab::Pinstar if ctx.pinstar_state.is_some() => {
                &[
                    ("R-click/a", "menu"),
                    ("L-drag", "pan"),
                    ("R-drag", "select"),
                    ("i", "edit"),
                    ("Ctrl+P", "help"),
                ]
            }
            crate::app::directory::state::DirectoryTab::Pinstar => &[
                ("Enter", "open"),
                ("n", "new"),
                ("a", "join"),
                ("Ctrl+P", "help"),
            ],
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
    let dim = Style::default().fg(theme::TEXT_DIM());

    spans.push(Span::styled("· ", Style::default().fg(theme::TEXT_DIM())));
    spans.push(Span::styled(
        format!(
            "{} ",
            crate::app::arcade::ui::game_title(ctx.game_selection)
        ),
        Style::default().fg(theme::TEXT_BRIGHT()),
    ));
    if ctx.game_selection == crate::app::state::GAME_SELECTION_SNAKE {
        spans.push(Span::styled("by github.com/AndreLobato ", dim));
    }
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
        if room.game_kind == crate::app::rooms::svc::GameKind::Asterion {
            spans.push(Span::styled(" by github.com/ricott1/asterion", dim));
        } else if room.game_kind == crate::app::rooms::svc::GameKind::Sshattrick {
            spans.push(Span::styled(" by github.com/ricott1/sshattrick", dim));
        }
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
        spans.push(Span::raw(" "));
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

fn line_width(line: &Line<'_>) -> usize {
    line.iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

fn app_frame_bottom_titles(area_width: u16) -> (Line<'static>, Option<Line<'static>>) {
    let title_width = usize::from(area_width.saturating_sub(2));
    for hint_style in [
        HelpHintStyle::DottedCtrl,
        HelpHintStyle::SpacedCtrl,
        HelpHintStyle::SpacedCaret,
    ] {
        let help_hint_title = app_frame_help_hint_title(hint_style);
        let help_hint_width = line_width(&help_hint_title);
        if help_hint_width <= title_width {
            let sponsor_title = app_frame_sponsor_title(title_width - help_hint_width);
            return (help_hint_title, sponsor_title);
        }
    }

    (app_frame_help_hint_title(HelpHintStyle::SpacedCaret), None)
}

fn app_frame_sponsor_title(sponsor_width: usize) -> Option<Line<'static>> {
    [
        sponsor_line(true, true),
        sponsor_line(false, true),
        sponsor_line(false, false),
    ]
    .into_iter()
    .find(|line| line_width(line) <= sponsor_width)
}

#[derive(Clone, Copy)]
enum HelpHintStyle {
    DottedCtrl,
    SpacedCtrl,
    SpacedCaret,
}

fn app_frame_help_hint_title(hint_style: HelpHintStyle) -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let sep_style = Style::default().fg(theme::TEXT_FAINT());
    let separator = match hint_style {
        HelpHintStyle::DottedCtrl => " · ",
        HelpHintStyle::SpacedCtrl | HelpHintStyle::SpacedCaret => "  ",
    };
    let use_caret = matches!(hint_style, HelpHintStyle::SpacedCaret);
    let hints = [
        ("Settings", ctrl_hint("O", use_caret)),
        ("Hub", ctrl_hint("G", use_caret)),
        ("Aqua", ctrl_hint("Q", use_caret)),
        ("Guide", "?"),
    ];

    let mut spans = Vec::new();
    for (idx, (label, key_text)) in hints.into_iter().enumerate() {
        if idx == 0 {
            spans.push(Span::styled(" ", dim));
        } else {
            spans.push(Span::styled(separator, sep_style));
        }
        spans.push(Span::styled(format!("{label} "), dim));
        spans.push(Span::styled(key_text, key));
    }
    spans.push(Span::styled(" ", dim));
    Line::from(spans)
}

fn ctrl_hint(key: &'static str, use_caret: bool) -> &'static str {
    match (use_caret, key) {
        (true, "O") => "^O",
        (true, "G") => "^G",
        (true, "Q") => "^Q",
        (false, "O") => "Ctrl+O",
        (false, "G") => "Ctrl+G",
        (false, "Q") => "Ctrl+Q",
        _ => key,
    }
}

fn sponsor_line(include_thanks: bool, include_protocol: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if include_thanks {
        spans.push(Span::styled(
            " thanks for hanging out ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled("☕ ", Style::default().fg(theme::AMBER())));
    }
    let url = if include_protocol {
        "https://ko-fi.com/mateuszpiorowski "
    } else {
        "ko-fi.com/mateuszpiorowski "
    };
    spans.push(Span::styled(url, Style::default().fg(theme::AMBER_DIM())));
    Line::from(spans).right_aligned()
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
        HelpHintStyle, app_frame_bottom_titles, app_frame_help_hint_title, app_frame_sponsor_title,
        dashboard_home_selected, line_width, mentions_hud_title, resolve_right_sidebar_enabled,
        room_list_sidebar_enabled, room_top_boxes_enabled, screen_number, sidebar_enabled,
        sponsor_line,
    };
    use crate::app::common::primitives::Screen;
    use late_core::models::user::RightSidebarMode;
    use uuid::Uuid;

    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.iter().map(|s| s.content.as_ref()).collect()
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
    fn right_sidebar_is_only_available_on_first_three_pages() {
        assert!(resolve_right_sidebar_enabled(
            RightSidebarMode::On,
            &[],
            Screen::Dashboard,
        ));
        assert!(resolve_right_sidebar_enabled(
            RightSidebarMode::On,
            &[],
            Screen::Arcade,
        ));
        assert!(resolve_right_sidebar_enabled(
            RightSidebarMode::On,
            &[],
            Screen::Rooms,
        ));
        assert!(!resolve_right_sidebar_enabled(
            RightSidebarMode::On,
            &[],
            Screen::Lateania,
        ));
        assert!(!resolve_right_sidebar_enabled(
            RightSidebarMode::On,
            &[],
            Screen::Artboard,
        ));
        assert!(!resolve_right_sidebar_enabled(
            RightSidebarMode::On,
            &[],
            Screen::Pinstar,
        ));
    }

    #[test]
    fn right_sidebar_custom_slots_follow_available_page_order() {
        assert_eq!(screen_number(Screen::Dashboard), 1);
        assert_eq!(screen_number(Screen::Arcade), 2);
        assert_eq!(screen_number(Screen::Rooms), 3);
        assert_eq!(screen_number(Screen::Artboard), 4);
        assert_eq!(screen_number(Screen::Lateania), 5);
        assert_eq!(screen_number(Screen::Rebels), 6);
        assert_eq!(screen_number(Screen::Pinstar), 7);

        assert!(resolve_right_sidebar_enabled(
            RightSidebarMode::Custom,
            &[1, 3],
            Screen::Dashboard,
        ));
        assert!(!resolve_right_sidebar_enabled(
            RightSidebarMode::Custom,
            &[1, 3],
            Screen::Arcade,
        ));
        assert!(resolve_right_sidebar_enabled(
            RightSidebarMode::Custom,
            &[1, 3],
            Screen::Rooms,
        ));
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
    fn room_top_boxes_enabled_is_always_on_for_home() {
        assert!(room_top_boxes_enabled(true, false, false, true, true));
        assert!(room_top_boxes_enabled(false, false, false, true, true));
        assert!(room_top_boxes_enabled(true, false, false, true, false));
    }

    #[test]
    fn room_top_boxes_enabled_prefers_settings_draft_for_non_home_while_modal_is_open() {
        assert!(!room_top_boxes_enabled(true, false, true, false, true));
        assert!(room_top_boxes_enabled(true, true, false, false, true));
    }

    #[test]
    fn room_top_boxes_enabled_uses_saved_profile_for_non_home_when_modal_is_closed() {
        assert!(room_top_boxes_enabled(false, false, true, false, true));
        assert!(!room_top_boxes_enabled(false, true, false, false, true));
    }

    #[test]
    fn room_top_boxes_enabled_is_off_for_synthetic_home_entries() {
        assert!(!room_top_boxes_enabled(true, true, true, false, false));
        assert!(!room_top_boxes_enabled(false, true, true, false, false));
    }

    #[test]
    fn dashboard_home_selected_for_lounge_room_without_synthetic_entry() {
        let lounge = Uuid::from_u128(1);
        assert!(dashboard_home_selected(Some(lounge), Some(lounge), false));
    }

    #[test]
    fn dashboard_home_selected_rejects_synthetic_and_non_lounge_rooms() {
        let lounge = Uuid::from_u128(1);
        let topic = Uuid::from_u128(2);
        assert!(!dashboard_home_selected(Some(lounge), Some(lounge), true));
        assert!(!dashboard_home_selected(Some(lounge), Some(topic), false));
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

    #[test]
    fn sponsor_title_drops_optional_segments_before_overlapping_help_hints() {
        let full_width = line_width(&sponsor_line(true, true));
        let url_width = line_width(&sponsor_line(false, true));
        let short_url_width = line_width(&sponsor_line(false, false));

        let full = app_frame_sponsor_title(full_width).expect("full sponsor should fit");
        assert_eq!(
            line_text(&full),
            " thanks for hanging out ☕ https://ko-fi.com/mateuszpiorowski "
        );

        let url_only =
            app_frame_sponsor_title(full_width - 1).expect("url-only sponsor should fit");
        assert_eq!(line_text(&url_only), "https://ko-fi.com/mateuszpiorowski ");

        let short_url =
            app_frame_sponsor_title(url_width - 1).expect("protocol-stripped sponsor should fit");
        assert_eq!(line_text(&short_url), "ko-fi.com/mateuszpiorowski ");

        let hidden = app_frame_sponsor_title(short_url_width - 1);
        assert!(hidden.is_none());
    }

    #[test]
    fn help_hint_title_lists_guide_last() {
        let help = app_frame_help_hint_title(HelpHintStyle::DottedCtrl);
        assert_eq!(
            line_text(&help),
            " Settings Ctrl+O · Hub Ctrl+G · Aqua Ctrl+Q · Guide ? "
        );
    }

    #[test]
    fn help_hint_title_compacts_separators_then_ctrl_notation() {
        let dotted = app_frame_help_hint_title(HelpHintStyle::DottedCtrl);
        let spaced = app_frame_help_hint_title(HelpHintStyle::SpacedCtrl);
        let caret = app_frame_help_hint_title(HelpHintStyle::SpacedCaret);
        assert_eq!(
            line_text(&spaced),
            " Settings Ctrl+O  Hub Ctrl+G  Aqua Ctrl+Q  Guide ? "
        );
        assert_eq!(line_text(&caret), " Settings ^O  Hub ^G  Aqua ^Q  Guide ? ");

        let (help, sponsor) = app_frame_bottom_titles((line_width(&dotted) + 2) as u16);
        assert_eq!(line_text(&help), line_text(&dotted));
        assert!(sponsor.is_none());

        let (help, sponsor) = app_frame_bottom_titles((line_width(&spaced) + 2) as u16);
        assert_eq!(line_text(&help), line_text(&spaced));
        assert!(sponsor.is_none());

        let (help, sponsor) = app_frame_bottom_titles((line_width(&caret) + 2) as u16);
        assert_eq!(line_text(&help), line_text(&caret));
        assert!(sponsor.is_none());
    }
}
