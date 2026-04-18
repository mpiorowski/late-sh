use std::sync::Arc;

use anyhow::Context;
use late_core::MutexRecover;
use late_core::api_types::NowPlaying;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear},
};

use late_core::models::leaderboard::LeaderboardData;

use super::{
    chat,
    common::{
        primitives::{Banner, BannerKind, Screen, draw_banner},
        sidebar::{SidebarProps, draw_sidebar},
        theme,
    },
    dashboard, icon_picker, profile,
    state::App,
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

fn desktop_notification_bytes(title: &str, body: &str, bell: bool) -> Vec<u8> {
    // OSC 777 carries (title, body) separately — kitty, Ghostty, rxvt-unicode,
    // foot, wezterm, konsole. OSC 9 is iTerm2's single-string variant and acts
    // as a fallback for terminals that don't parse 777. Terminals that don't
    // recognize either sequence silently drop it.
    let title = sanitize_notification_field(title);
    let body = sanitize_notification_field(body);

    let bell = if bell { "\x07" } else { "" };
    format!("\x1b]777;notify;{title};{body}\x1b\\\x1b]9;{title}: {body}\x1b\\{bell}").into_bytes()
}

struct DrawContext<'a> {
    connect_url: &'a str,
    dashboard_view: dashboard::ui::DashboardRenderInput<'a>,
    chat_view: chat::ui::ChatRenderInput<'a>,
    profile_view: profile::ui::ProfileRenderInput<'a>,
    game_selection: usize,
    is_playing_game: bool,
    twenty_forty_eight_state: &'a crate::app::games::twenty_forty_eight::state::State,
    tetris_state: &'a crate::app::games::tetris::state::State,
    sudoku_state: &'a crate::app::games::sudoku::state::State,
    nonogram_state: &'a crate::app::games::nonogram::state::State,
    solitaire_state: &'a crate::app::games::solitaire::state::State,
    minesweeper_state: &'a crate::app::games::minesweeper::state::State,
    blackjack_state: &'a crate::app::games::blackjack::state::State,
    leaderboard: &'a Arc<LeaderboardData>,
    visualizer: &'a Visualizer,
    now_playing: Option<&'a NowPlaying>,
    paired_client: Option<&'a ClientAudioState>,
    online_count: usize,
    bonsai: &'a crate::app::bonsai::state::BonsaiState,
    activity: &'a std::collections::VecDeque<crate::state::ActivityEvent>,
    banner: Option<&'a Banner>,
    username: &'a str,
    is_admin: bool,
    show_welcome: bool,
    show_help: bool,
    help_scroll: u16,
    show_splash: bool,
    splash_ticks: usize,
    splash_hint: &'a str,
    show_web_chat_qr: bool,
    web_chat_qr_url: Option<&'a str>,
    is_draining: bool,
    icon_picker_open: bool,
    icon_picker_state: &'a icon_picker::IconPickerState,
    icon_catalog: Option<&'a icon_picker::catalog::IconCatalogData>,
}

impl App {
    pub fn render(&mut self) -> anyhow::Result<Vec<u8>> {
        // Init theme and layout sync
        theme::set_current_by_id(self.profile_state.theme_id());

        // Keep composer text width in sync for cursor up/down navigation.
        // outer border(2) + sidebar(24) + chat-block border(2) + composer padding(3) = 31
        self.chat
            .set_composer_text_width(self.size.0.saturating_sub(31).max(1) as usize);
        self.chat.sync_composer_layout();

        // Synchronize terminal background color with theme bg_canvas if enabled
        let enabled = self.profile_state.profile().enable_background_color;
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
        let screen = self.screen;
        let now_playing: Option<NowPlaying> = self
            .now_playing_rx
            .as_mut()
            .and_then(|rx| rx.borrow_and_update().clone());
        let banner = self.active_banner().cloned();
        let vote_snapshot = self.vote.snapshot();
        let vote_my_vote = self.vote.my_vote();
        let now_playing_text = now_playing.as_ref().map(|np| np.track.to_string());
        let vote_next_switch_in = vote_snapshot
            .next_switch_in
            .saturating_sub(vote_snapshot.updated_at.elapsed());
        let visualizer = &self.visualizer;
        let paired_client_state = self.paired_client_state();
        let chat_usernames = self.chat.usernames();
        let chat_badges = self.leaderboard.badges();
        let bonsai_glyphs = self.chat.bonsai_glyphs();
        let dashboard_view = dashboard::ui::DashboardRenderInput {
            now_playing: now_playing_text.as_deref(),
            vote_counts: &vote_snapshot.counts,
            current_genre: vote_snapshot.current_genre,
            next_switch_in: vote_next_switch_in,
            my_vote: vote_my_vote,
            chat_view: chat::ui::DashboardChatView {
                messages: self.chat.general_messages(),
                overlay: self.chat.overlay(),
                rows_cache: &mut self.dashboard_chat_rows_cache,
                usernames: chat_usernames,
                badges: &chat_badges,
                current_user_id: self.user_id,
                selected_message_id: self.chat.selected_message_id,
                composer: self.chat.composer.as_str(),
                composer_rows: self.chat.composer_rows(),
                composer_cursor: self.chat.composer_cursor,
                composing: self.chat.composing,
                cursor_visible: self.chat.cursor_visible(),
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
        };
        let notifications_view = chat::notifications::ui::NotificationListView {
            items: self.chat.notifications.all_items(),
            selected_index: self.chat.notifications.selected_index(),
        };
        let chat_view = chat::ui::ChatRenderInput {
            news_selected: self.chat.news_selected,
            news_unread_count: self.chat.news.unread_count(),
            news_view,
            rows_cache: &mut self.active_room_rows_cache,
            chat_rooms: self.chat.rooms.as_slice(),
            overlay: self.chat.overlay(),
            usernames: chat_usernames,
            badges: &chat_badges,
            unread_counts: &self.chat.unread_counts,
            selected_room_id: self.chat.selected_room_id,
            room_jump_active: self.chat.room_jump_active,
            selected_message_id: self.chat.selected_message_id,
            highlighted_message_id: self.chat.highlighted_message_id,
            composer: self.chat.composer.as_str(),
            composer_rows: self.chat.composer_rows(),
            composer_cursor: self.chat.composer_cursor,
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
        };
        // Update viewport height for profile scroll (content area = total - borders)
        let profile_viewport_h = area.height.saturating_sub(2);
        self.profile_state.set_viewport_height(profile_viewport_h);
        let user_streak = self
            .leaderboard
            .user_streaks
            .get(&self.user_id)
            .copied()
            .unwrap_or(0);
        let profile_view = profile::ui::ProfileRenderInput {
            profile: self.profile_state.profile(),
            editing_username: self.profile_state.editing_username(),
            username_composer: self.profile_state.username_composer(),
            ai_model: self.profile_state.ai_model(),
            theme_id: self.profile_state.theme_id(),
            scroll_offset: self.profile_state.scroll_offset(),
            current_streak: user_streak,
            chip_balance: self.chip_balance,
            tetris_best: self.tetris_state.best_score,
            twenty_forty_eight_best: self.twenty_forty_eight_state.best_score,
            cursor_visible: self.profile_state.cursor_visible(),
            notify_kinds: &self.profile_state.profile().notify_kinds,
            notify_bell: self.profile_state.profile().notify_bell,
            notify_cooldown_mins: self.profile_state.profile().notify_cooldown_mins,
            settings_row: self.profile_state.settings_row,
        };
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
                        profile_view,
                        game_selection: self.game_selection,
                        is_playing_game: self.is_playing_game,
                        twenty_forty_eight_state: &self.twenty_forty_eight_state,
                        tetris_state: &self.tetris_state,
                        sudoku_state: &self.sudoku_state,
                        nonogram_state: &self.nonogram_state,
                        solitaire_state: &self.solitaire_state,
                        minesweeper_state: &self.minesweeper_state,
                        blackjack_state: &self.blackjack_state,
                        leaderboard: &self.leaderboard,
                        visualizer,
                        now_playing: now_playing.as_ref(),
                        paired_client: paired_client_state.as_ref(),
                        online_count,
                        bonsai: &self.bonsai_state,
                        activity: &self.activity,
                        banner: banner.as_ref(),
                        username: &self.profile_state.profile().username,
                        is_admin: self.is_admin,
                        show_welcome: self.show_welcome,
                        show_help: self.show_help,
                        help_scroll: self.help_scroll,
                        show_splash: self.show_splash,
                        splash_ticks: self.splash_ticks,
                        splash_hint: &self.splash_hint,
                        show_web_chat_qr: self.show_web_chat_qr,
                        web_chat_qr_url: self.web_chat_qr_url.as_deref(),
                        is_draining: self.is_draining.load(std::sync::atomic::Ordering::Relaxed),
                        icon_picker_open: self.icon_picker_open,
                        icon_picker_state: &self.icon_picker_state,
                        icon_catalog: self.icon_catalog.as_ref(),
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
                let payload =
                    desktop_notification_bytes(&notif.title, &notif.body, profile.notify_bell);
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

        let block = Block::default()
            .title(" late.sh ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let main_layout =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(inner);
        let content_area = main_layout[0];
        let sidebar_area = main_layout[1];
        let connect_url = ctx.connect_url;

        match screen {
            Screen::Dashboard => {
                dashboard::ui::draw_dashboard(frame, content_area, ctx.dashboard_view)
            }
            Screen::Chat => chat::ui::draw_chat(frame, content_area, ctx.chat_view),
            Screen::Profile => profile::ui::draw_profile(frame, content_area, &ctx.profile_view),
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
                },
            ),
        }

        draw_sidebar(
            frame,
            sidebar_area,
            &SidebarProps {
                screen,
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
            },
        );

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

        if ctx.show_welcome {
            draw_welcome_overlay(frame, inner, ctx.username);
        }

        if ctx.show_help {
            draw_help_overlay(
                frame,
                inner,
                screen,
                ctx.game_selection,
                ctx.is_playing_game,
                ctx.help_scroll,
            );
        }

        if ctx.show_web_chat_qr
            && let Some(url) = ctx.web_chat_qr_url
        {
            let (title, subtitle) = if url.contains("/chat/") {
                ("Web Chat", "Scan to open web chat")
            } else {
                ("Pair", "Scan to pair audio")
            };
            super::qr::draw_qr_overlay(frame, inner, url, title, subtitle);
        }

        if ctx.icon_picker_open
            && let Some(catalog) = ctx.icon_catalog
        {
            icon_picker::picker::render(frame, area, ctx.icon_picker_state, catalog);
        }
    }
}

fn draw_help_overlay(
    frame: &mut Frame,
    area: Rect,
    _screen: Screen,
    _game_selection: usize,
    _is_playing_game: bool,
    help_scroll: u16,
) {
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Clear, Paragraph};

    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let muted = Style::default().fg(theme::TEXT_MUTED());
    let bold_amber = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let key_s = Style::default().fg(theme::AMBER_DIM());
    let desc_s = Style::default().fg(theme::TEXT());

    let col_w: u16 = 34;

    let key = |k: &str, d: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {:<10}", k), key_s),
            Span::styled(d.to_string(), desc_s),
        ])
    };

    let section = |label: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled("  ", faint),
            Span::styled(label.to_string(), bold_amber),
        ])
    };

    let divider = |w: u16| -> Line<'static> {
        Line::from(Span::styled(
            format!("  {}", "─".repeat((w as usize).saturating_sub(4))),
            faint,
        ))
    };

    let hint =
        |text: &str| -> Line<'static> { Line::from(Span::styled(format!("  {text}"), muted)) };

    // ── Left column ──
    let left = vec![
        Line::from(""),
        section("Global"),
        divider(col_w),
        key("Tab / S-Tab", "next / prev screen"),
        key("1-4", "jump to screen"),
        key("m", "mute paired"),
        key("+ / -", "volume"),
        key("p", "pair audio (QR)"),
        key("?", "this help"),
        key("q", "quit"),
        Line::from(""),
        section("Dashboard"),
        divider(col_w),
        key("i", "compose chat"),
        key("Enter", "copy CLI cmd"),
        Line::from(""),
        section("Chat"),
        divider(col_w),
        key("h / l", "switch room"),
        key("i", "compose"),
        key("/help", "commands"),
        key("/music", "music setup"),
        key("/active", "active users"),
        key("/list", "private room users"),
        Line::from(""),
        section("Profile"),
        divider(col_w),
        key("j / k", "navigate"),
        key("i", "edit username"),
        key("Space", "toggle"),
    ];

    // ── Right column ──
    let right = vec![
        Line::from(""),
        section("Bonsai"),
        divider(col_w),
        key("w", "water / replant"),
        key("x", "prune (reshape)"),
        key("s", "copy to clipboard"),
        Line::from(""),
        hint("Water daily to grow."),
        hint("Grows while connected."),
        hint("7 days dry = dies."),
        hint("Prune costs 20% growth"),
        hint("but changes tree shape."),
        Line::from(""),
        section("News"),
        divider(col_w),
        key("j / k", "navigate"),
        key("i", "paste / share URL"),
        key("Enter", "submit / copy URL"),
        key("Esc", "cancel URL entry"),
        key("d", "delete (own)"),
        Line::from(""),
        section("The Arcade"),
        divider(col_w),
        key("j / k", "browse"),
        key("Enter", "play"),
        key("Esc", "exit game"),
        hint("Game keys in info panel."),
    ];

    let row_count = left.len().max(right.len());
    let total_w = (col_w * 2 + 1).min(area.width.saturating_sub(4));
    let total_h = ((row_count + 4) as u16).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(total_w)) / 2;
    let y = area.y + (area.height.saturating_sub(total_h)) / 2;
    let popup_area = Rect::new(x, y, total_w, total_h);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " Keybindings ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Content area: leave 1 row at bottom for footer
    let content_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );

    // Two columns with a thin separator
    let cols = Layout::horizontal([
        Constraint::Length(col_w),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(content_area);

    // Vertical separator
    let sep_lines: Vec<Line<'static>> = (0..cols[1].height)
        .map(|_| Line::from(Span::styled("│", faint)))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), cols[1]);

    let content_h = left.len().max(right.len()) as u16;
    let visible_h = cols[0].height;
    let max_scroll = content_h.saturating_sub(visible_h);
    let scroll = help_scroll.min(max_scroll);

    frame.render_widget(Paragraph::new(left).scroll((scroll, 0)), cols[0]);
    frame.render_widget(Paragraph::new(right).scroll((scroll, 0)), cols[2]);

    // Footer
    let footer_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    let can_scroll = max_scroll > 0;
    let mut footer = vec![
        Span::styled("  press ", dim),
        Span::styled("? ", Style::default().fg(theme::TEXT_MUTED())),
        Span::styled("to close", dim),
    ];
    if can_scroll {
        footer.push(Span::styled(
            "  j/k ",
            Style::default().fg(theme::TEXT_MUTED()),
        ));
        footer.push(Span::styled("scroll", dim));
    }
    frame.render_widget(Paragraph::new(Line::from(footer)).centered(), footer_area);
}

fn draw_welcome_overlay(frame: &mut Frame, area: Rect, username: &str) {
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Clear, Paragraph, Wrap};

    let dim = Style::default().fg(theme::TEXT_DIM());
    let bold_cyan = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let white = Style::default().fg(theme::TEXT_BRIGHT());
    let green = Style::default().fg(theme::SUCCESS());

    let greeting = format!("Welcome back, @{username}.");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", dim),
            Span::styled(
                greeting,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "  A cozy terminal clubhouse for developers.",
            white,
        )),
        Line::from(""),
        // ── Music ──
        Line::from(vec![
            Span::styled("  ── ", dim),
            Span::styled("Music", bold_cyan),
            Span::styled(" ──", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "    Lofi, ambient, classical & jazz streams.",
            white,
        )),
        Line::from(vec![
            Span::styled("    Listen via ", dim),
            Span::styled("CLI", green),
            Span::styled(" (recommended) or the ", dim),
            Span::styled("web player", green),
            Span::styled(".", dim),
        ]),
        Line::from(""),
        // ── Chat ──
        Line::from(vec![
            Span::styled("  ── ", dim),
            Span::styled("Chat", bold_cyan),
            Span::styled(" ──", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "    Hang out in rooms with other devs.",
            white,
        )),
        Line::from(Span::styled(
            "    Tied to your SSH key, same chats anywhere.",
            dim,
        )),
        Line::from(""),
        // ── Arcade ──
        Line::from(vec![
            Span::styled("  ── ", dim),
            Span::styled("Arcade", bold_cyan),
            Span::styled(" ──", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "    2048, Tetris & daily puzzles with leaderboards.",
            white,
        )),
        Line::from(Span::styled("    Multiplayer coming soon.", dim)),
        Line::from(""),
        // ── News ──
        Line::from(vec![
            Span::styled("  ── ", dim),
            Span::styled("News", bold_cyan),
            Span::styled(" ──", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "    Share links and watch the community feed.",
            white,
        )),
        Line::from(Span::styled(
            "    Auto-generated summaries keep you in the loop.",
            dim,
        )),
        Line::from(""),
        // ── Your Identity ──
        Line::from(vec![
            Span::styled("  ── ", dim),
            Span::styled("Your Identity", bold_cyan),
            Span::styled(" ──", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "    No passwords. No OAuth. No accounts.",
            white,
        )),
        Line::from(Span::styled("    Your SSH key is your identity.", white)),
        Line::from(""),
        Line::from(Span::styled(
            "    Chats, scores, leaderboards, badges, your bonsai,",
            dim,
        )),
        Line::from(Span::styled(
            "    all tied to your public key fingerprint.",
            dim,
        )),
        Line::from(Span::styled("    Same key, same data anywhere.", dim)),
        Line::from(""),
        Line::from(vec![
            Span::styled("    Back up ", dim),
            Span::styled("~/.ssh/id_*", green),
            Span::styled(" to keep your account.", dim),
        ]),
        Line::from(Span::styled(
            "    Grab headphones, pick a vibe, build something.",
            white,
        )),
        Line::from(""),
        Line::from(Span::styled("    Want your music on late.sh?", white)),
        Line::from(vec![
            Span::styled("    Write to ", dim),
            Span::styled("admin@dwarfforge.io", green),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Press any key to start.", dim)),
        Line::from(""),
    ];

    let w = 64u16.min(area.width.saturating_sub(4));
    let content_h = lines.len() as u16;
    let h = (content_h + 2).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup_area = Rect::new(x, y, w, h);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" late.sh ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::desktop_notification_bytes;

    #[test]
    fn desktop_notification_bytes_emits_osc_777_and_osc_9_with_st_terminators() {
        let got = String::from_utf8(desktop_notification_bytes("DM title", "hello", true))
            .expect("valid utf8");
        assert_eq!(
            got,
            "\x1b]777;notify;DM title;hello\x1b\\\x1b]9;DM title: hello\x1b\\\x07"
        );
    }

    #[test]
    fn desktop_notification_bytes_sanitize_control_bytes_and_separators() {
        let got = String::from_utf8(desktop_notification_bytes("hey;\x07", "a\nb\x1bc", false))
            .expect("valid utf8");
        assert_eq!(
            got,
            "\x1b]777;notify;hey| ;a b c\x1b\\\x1b]9;hey| : a b c\x1b\\"
        );
    }
}
