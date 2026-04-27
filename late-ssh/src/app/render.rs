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

use super::{
    artboard, bonsai, chat,
    common::{
        primitives::{Banner, BannerKind, Screen, draw_banner},
        sidebar::{SidebarProps, draw_sidebar, sidebar_clock_text},
        theme,
    },
    dashboard, help_modal, icon_picker, profile_modal, quit_confirm, settings_modal,
    state::{App, NotificationMode},
    visualizer::Visualizer,
};
use crate::session::ClientAudioState;

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

fn games_sidebar_enabled(show_settings: bool, draft_enabled: bool, profile_enabled: bool) -> bool {
    if show_settings {
        draft_enabled
    } else {
        profile_enabled
    }
}

fn dashboard_header_enabled(
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

struct DrawContext<'a> {
    connect_url: &'a str,
    dashboard_view: dashboard::ui::DashboardRenderInput<'a>,
    chat_view: chat::ui::ChatRenderInput<'a>,
    game_selection: usize,
    is_playing_game: bool,
    rooms_add_form_open: bool,
    rooms_display_name_input: &'a str,
    rooms_snapshot: &'a crate::app::rooms::svc::RoomsSnapshot,
    rooms_selected_index: usize,
    rooms_active_room: Option<&'a crate::app::rooms::svc::RoomListItem>,
    twenty_forty_eight_state: &'a crate::app::games::twenty_forty_eight::state::State,
    tetris_state: &'a crate::app::games::tetris::state::State,
    sudoku_state: &'a crate::app::games::sudoku::state::State,
    nonogram_state: &'a crate::app::games::nonogram::state::State,
    solitaire_state: &'a crate::app::games::solitaire::state::State,
    minesweeper_state: &'a crate::app::games::minesweeper::state::State,
    blackjack_state: &'a crate::app::rooms::blackjack::state::State,
    dartboard_state: Option<&'a crate::app::artboard::state::State>,
    artboard_interacting: bool,
    leaderboard: &'a Arc<LeaderboardData>,
    visualizer: &'a Visualizer,
    now_playing: Option<&'a NowPlaying>,
    paired_client: Option<&'a ClientAudioState>,
    sidebar_clock: &'a str,
    online_count: usize,
    bonsai: &'a crate::app::bonsai::state::BonsaiState,
    activity: &'a std::collections::VecDeque<crate::state::ActivityEvent>,
    banner: Option<&'a Banner>,
    is_admin: bool,
    show_right_sidebar: bool,
    show_games_sidebar: bool,
    show_settings: bool,
    settings_modal_state: &'a settings_modal::state::SettingsModalState,
    show_quit_confirm: bool,
    show_profile_modal: bool,
    profile_modal_state: &'a profile_modal::state::ProfileModalState,
    show_bonsai_modal: bool,
    bonsai_care_state: &'a bonsai::care::BonsaiCareState,
    show_help: bool,
    help_modal_state: &'a help_modal::state::HelpModalState,
    show_splash: bool,
    splash_ticks: usize,
    splash_hint: &'a str,
    show_web_chat_qr: bool,
    web_chat_qr_url: Option<&'a str>,
    is_draining: bool,
    icon_picker_open: bool,
    icon_picker_state: &'a icon_picker::IconPickerState,
    icon_catalog: Option<&'a icon_picker::catalog::IconCatalogData>,
    mentions_unread_count: i64,
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
            self.settings_modal_state.draft().show_right_sidebar,
            self.profile_state.profile().show_right_sidebar,
        );
        let show_dashboard_header = dashboard_header_enabled(
            self.show_settings,
            self.settings_modal_state.draft().show_dashboard_header,
            self.profile_state.profile().show_dashboard_header,
        );
        let show_games_sidebar = games_sidebar_enabled(
            self.show_settings,
            self.settings_modal_state.draft().show_games_sidebar,
            self.profile_state.profile().show_games_sidebar,
        );
        let screen = self.screen;
        let now_playing: Option<NowPlaying> = self
            .now_playing_rx
            .as_mut()
            .and_then(|rx| rx.borrow_and_update().clone());
        let banner = self.active_banner().cloned();
        let vote_snapshot = self.vote.snapshot();
        let vote_my_vote = self.vote.my_vote();
        let sidebar_clock = sidebar_clock_text(self.profile_state.profile().timezone.as_deref());
        let now_playing_text = now_playing.as_ref().map(|np| np.track.to_string());
        let vote_next_switch_in = vote_snapshot
            .next_switch_in
            .saturating_sub(vote_snapshot.updated_at.elapsed());
        let visualizer = &self.visualizer;
        let paired_client_state = self.paired_client_state();
        let chat_usernames = self.chat.usernames();
        let chat_countries = self.chat.countries();
        let chat_badges = self.leaderboard.badges();
        let bonsai_glyphs = self.chat.bonsai_glyphs();
        let message_reactions = self.chat.message_reactions();
        let dashboard_active_room = self.dashboard_active_room_id();
        let dashboard_strip_pins = self.dashboard_strip_pins();
        let dashboard_messages = dashboard_active_room
            .map(|room_id| self.chat.messages_for_room(room_id))
            .unwrap_or(&[]);
        let dashboard_view = dashboard::ui::DashboardRenderInput {
            now_playing: now_playing_text.as_deref(),
            vote_counts: &vote_snapshot.counts,
            current_genre: vote_snapshot.current_genre,
            next_switch_in: vote_next_switch_in,
            my_vote: vote_my_vote,
            show_header: show_dashboard_header,
            favorites_strip: dashboard_strip_pins.as_deref(),
            chat_view: chat::ui::DashboardChatView {
                messages: dashboard_messages,
                overlay: self.chat.overlay(),
                rows_cache: &mut self.dashboard_chat_rows_cache,
                usernames: chat_usernames,
                countries: chat_countries,
                badges: &chat_badges,
                message_reactions,
                current_user_id: self.user_id,
                selected_message_id: self.chat.selected_message_id,
                reaction_picker_active: self.chat.is_reaction_leader_active(),
                composer: self.chat.composer(),
                composing: self.chat.composing,
                mention_matches: &self.chat.mention_ac.matches,
                mention_selected: self.chat.mention_ac.selected,
                mention_active: self.chat.mention_ac.active,
                reply_author: self.chat.reply_target().map(|reply| reply.author.as_str()),
                is_editing: self.chat.edited_message_id.is_some(),
                bonsai_glyphs,
            },
        };
        let news_view = chat::news::ui::ArticleListView {
            articles: self.chat.news.all_articles(),
            selected_index: self.chat.news.selected_index(),
            marker_read_at: self.chat.news.marker_read_at(),
        };
        let discover_view = chat::discover::ui::DiscoverListView {
            items: self.chat.discover.all_items(),
            selected_index: self.chat.discover.selected_index(),
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
        };
        let showcase_unread_count = self.chat.showcase.unread_count();
        let showcase_composing = self.chat.showcase.composing();
        let chat_view = chat::ui::ChatRenderInput {
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
            badges: &chat_badges,
            message_reactions,
            unread_counts: &self.chat.unread_counts,
            selected_room_id: self.chat.selected_room_id,
            room_jump_active: self.chat.room_jump_active,
            selected_message_id: self.chat.selected_message_id,
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
        };
        self.settings_modal_state
            .set_modal_width(settings_modal::ui::MODAL_WIDTH);
        let online_count = self
            .active_users
            .as_ref()
            .map(|active_users| active_users.lock_recover().len())
            .unwrap_or(0);
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
                        rooms_add_form_open: self.rooms_add_form_open,
                        rooms_display_name_input: self.rooms_display_name_input.as_str(),
                        rooms_snapshot: &self.rooms_snapshot,
                        rooms_selected_index: self.rooms_selected_index,
                        rooms_active_room: self.rooms_active_room.as_ref(),
                        twenty_forty_eight_state: &self.twenty_forty_eight_state,
                        tetris_state: &self.tetris_state,
                        sudoku_state: &self.sudoku_state,
                        nonogram_state: &self.nonogram_state,
                        solitaire_state: &self.solitaire_state,
                        minesweeper_state: &self.minesweeper_state,
                        blackjack_state: &self.blackjack_state,
                        dartboard_state: self.dartboard_state.as_ref(),
                        artboard_interacting: self.artboard_interacting,
                        leaderboard: &self.leaderboard,
                        visualizer,
                        now_playing: now_playing.as_ref(),
                        paired_client: paired_client_state.as_ref(),
                        sidebar_clock: &sidebar_clock,
                        online_count,
                        bonsai: &self.bonsai_state,
                        activity: &self.activity,
                        banner: banner.as_ref(),
                        is_admin: self.is_admin,
                        show_right_sidebar,
                        show_games_sidebar,
                        show_settings: self.show_settings,
                        settings_modal_state: &self.settings_modal_state,
                        show_quit_confirm: self.show_quit_confirm,
                        show_profile_modal: self.show_profile_modal,
                        profile_modal_state: &self.profile_modal_state,
                        show_bonsai_modal: self.show_bonsai_modal,
                        bonsai_care_state: &self.bonsai_care_state,
                        show_help: self.show_help,
                        help_modal_state: &self.help_modal_state,
                        show_splash: self.show_splash,
                        splash_ticks: self.splash_ticks,
                        splash_hint: &self.splash_hint,
                        show_web_chat_qr: self.show_web_chat_qr,
                        web_chat_qr_url: self.web_chat_qr_url.as_deref(),
                        is_draining: self.is_draining.load(std::sync::atomic::Ordering::Relaxed),
                        icon_picker_open: self.icon_picker_open,
                        icon_picker_state: &self.icon_picker_state,
                        icon_catalog: self.icon_catalog.as_ref(),
                        mentions_unread_count: self.chat.notifications.unread_count(),
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
                dashboard::ui::draw_dashboard(frame, content_area, ctx.dashboard_view)
            }
            Screen::Chat => chat::ui::draw_chat(frame, content_area, ctx.chat_view),
            Screen::Artboard => {
                if let Some(state) = ctx.dartboard_state {
                    artboard::ui::draw_game(frame, content_area, state, ctx.artboard_interacting);
                }
            }
            Screen::Games => crate::app::games::ui::draw_games_hub(
                frame,
                content_area,
                &crate::app::games::ui::GamesHubView {
                    game_selection: ctx.game_selection,
                    is_playing_game: ctx.is_playing_game,
                    twenty_forty_eight_state: ctx.twenty_forty_eight_state,
                    tetris_state: ctx.tetris_state,
                    sudoku_state: ctx.sudoku_state,
                    nonogram_state: ctx.nonogram_state,
                    solitaire_state: ctx.solitaire_state,
                    minesweeper_state: ctx.minesweeper_state,
                    blackjack_state: ctx.blackjack_state,
                    is_admin: ctx.is_admin,
                    leaderboard: ctx.leaderboard,
                    show_sidebar: ctx.show_games_sidebar,
                },
            ),
            Screen::Rooms => crate::app::rooms::ui::draw_rooms_page(
                frame,
                content_area,
                &crate::app::rooms::ui::RoomsPageView {
                    add_form_open: ctx.rooms_add_form_open,
                    display_name: ctx.rooms_display_name_input,
                    snapshot: ctx.rooms_snapshot,
                    selected_index: ctx.rooms_selected_index,
                    active_room: ctx.rooms_active_room,
                    blackjack_state: ctx.blackjack_state,
                    is_admin: ctx.is_admin,
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
                    online_count: ctx.online_count,
                    bonsai: ctx.bonsai,
                    audio_beat: ctx.visualizer.beat(),
                    connect_url,
                    activity: ctx.activity,
                    clock_text: ctx.sidebar_clock,
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

        if ctx.show_quit_confirm {
            quit_confirm::ui::draw(frame, inner);
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
        (Screen::Chat, "2"),
        (Screen::Games, "3"),
        (Screen::Rooms, "4"),
        (Screen::Artboard, "5"),
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
        Screen::Dashboard => "Dashboard",
        Screen::Chat => "Chat",
        Screen::Games => "The Arcade",
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
        NotificationMode, desktop_notification_bytes, games_sidebar_enabled, mentions_hud_title,
        sidebar_enabled,
    };

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
    fn games_sidebar_enabled_prefers_settings_draft_while_modal_is_open() {
        assert!(!games_sidebar_enabled(true, false, true));
        assert!(games_sidebar_enabled(true, true, false));
    }

    #[test]
    fn games_sidebar_enabled_uses_saved_profile_when_modal_is_closed() {
        assert!(games_sidebar_enabled(false, false, true));
        assert!(!games_sidebar_enabled(false, true, false));
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
