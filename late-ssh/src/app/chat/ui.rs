use late_core::models::chat_message::ChatMessage;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};
use uuid::Uuid;

use crate::app::common::{
    overlay::{Overlay, draw_overlay},
    theme,
};
use late_core::models::leaderboard::BadgeTier;

// Re-export types that external modules reference via `chat::ui::`.
pub use super::ui_text::ComposerRow;
pub(super) use super::ui_text::build_composer_rows;
pub(crate) use super::ui_text::{composer_cursor_scroll_for_rows, composer_line_count_for_rows};

use super::ui_text::{
    build_composer_lines, build_composer_lines_from_rows, composer_line_count,
    wrap_chat_entry_to_lines,
};

// ── Dashboard chat card ─────────────────────────────────────

pub struct DashboardChatView<'a> {
    pub messages: &'a [ChatMessage],
    pub overlay: Option<&'a Overlay>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub usernames: &'a HashMap<Uuid, String>,
    pub badges: &'a HashMap<Uuid, BadgeTier>,
    pub current_user_id: Uuid,
    pub selected_message_id: Option<Uuid>,
    pub composer: &'a str,
    pub composer_rows: &'a [ComposerRow],
    pub composer_cursor: usize,
    pub composing: bool,
    pub cursor_visible: bool,
    pub mention_matches: &'a [String],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
}

pub fn draw_dashboard_chat_card(frame: &mut Frame, area: Rect, view: DashboardChatView<'_>) {
    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let total_composer_lines = composer_line_count_for_rows(view.composer, view.composer_rows);
    let visible_composer_lines = total_composer_lines.min(5);
    let dash_composer_scroll =
        composer_cursor_scroll_for_rows(view.composer_rows, view.composer_cursor, 5);
    let composer_height = visible_composer_lines as u16 + 2;
    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(composer_height),
    ])
    .split(inner);
    let messages_area = layout[0];
    let composer_area = Some(layout[2]);

    let mut lines = Vec::new();
    if view.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "No messages yet.",
            Style::default().fg(theme::TEXT_DIM),
        )));
    } else {
        let height = messages_area.height.max(1) as usize;
        let width = messages_area.width.max(1) as usize;
        ensure_chat_rows_cache(
            view.rows_cache,
            view.messages.iter().collect(),
            width,
            ChatRowsContext {
                current_user_id: view.current_user_id,
                usernames: view.usernames,
                badges: view.badges,
                bonsai_glyphs: view.bonsai_glyphs,
            },
        );
        lines = visible_chat_rows(view.rows_cache, view.selected_message_id, None, height);
    }

    frame.render_widget(Paragraph::new(lines), messages_area);
    if let Some(overlay) = view.overlay {
        draw_overlay(frame, messages_area, overlay);
    }

    if let Some(area) = composer_area {
        let composer_title = if view.composing {
            if let Some(author) = view.reply_author {
                format!(" Reply to @{author} (Enter send, Alt+Enter newline, Esc cancel) ")
            } else {
                " Message (Enter send, Alt+Enter newline, Esc cancel) ".to_string()
            }
        } else {
            " Message (i compose) ".to_string()
        };
        let composer_style = if view.composing {
            Style::default().fg(theme::BORDER_ACTIVE)
        } else {
            Style::default().fg(theme::BORDER)
        };
        let composer_block = Block::default()
            .title(composer_title.as_str())
            .borders(Borders::ALL)
            .border_style(composer_style);
        let composer_lines = build_composer_lines_from_rows(
            view.composer,
            view.composer_rows,
            view.composer_cursor,
            view.composing,
            view.cursor_visible,
        );
        frame.render_widget(
            Paragraph::new(composer_lines)
                .block(composer_block)
                .scroll((dash_composer_scroll, 0)),
            area,
        );

        if view.mention_active {
            draw_mention_autocomplete(frame, area, view.mention_matches, view.mention_selected);
        }
    }
}

// ── Chat rows cache & scroll ────────────────────────────────

struct ChatRowsContext<'a> {
    current_user_id: Uuid,
    usernames: &'a HashMap<Uuid, String>,
    badges: &'a HashMap<Uuid, BadgeTier>,
    bonsai_glyphs: &'a HashMap<Uuid, String>,
}

#[derive(Default)]
pub struct ChatRowsCache {
    width: usize,
    fingerprint: u64,
    all_rows: Vec<Line<'static>>,
    selected_ranges: HashMap<Uuid, (usize, usize)>,
    highlighted_ranges: HashMap<Uuid, (usize, usize)>,
}

fn chat_rows_fingerprint(
    messages: &[&ChatMessage],
    ctx: &ChatRowsContext<'_>,
    width: usize,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    ctx.current_user_id.hash(&mut hasher);
    // Include current minute so relative timestamps ("5 mins ago") stay fresh.
    (chrono::Utc::now().timestamp() / 60).hash(&mut hasher);

    for msg in messages {
        msg.id.hash(&mut hasher);
        msg.user_id.hash(&mut hasher);
        msg.created.hash(&mut hasher);
        msg.body.hash(&mut hasher);
        ctx.usernames.get(&msg.user_id).hash(&mut hasher);
        ctx.badges
            .get(&msg.user_id)
            .map(|badge| badge.label())
            .hash(&mut hasher);
        ctx.bonsai_glyphs.get(&msg.user_id).hash(&mut hasher);
    }

    hasher.finish()
}

fn ensure_chat_rows_cache(
    cache: &mut ChatRowsCache,
    messages: Vec<&ChatMessage>,
    width: usize,
    ctx: ChatRowsContext<'_>,
) {
    let fingerprint = chat_rows_fingerprint(&messages, &ctx, width);
    if cache.width == width && cache.fingerprint == fingerprint {
        return;
    }

    let our_mention = ctx
        .usernames
        .get(&ctx.current_user_id)
        .map(|name| format!("@{name}"));
    let mut all_rows: Vec<Line> = Vec::new();
    let mut selected_ranges = HashMap::new();
    let mut highlighted_ranges = HashMap::new();
    let mut first = true;
    let mut prev_user_id: Option<Uuid> = None;
    let mut prev_created: Option<chrono::DateTime<chrono::Utc>> = None;

    for msg in messages.into_iter().rev() {
        let is_own = msg.user_id == ctx.current_user_id;
        let is_continuation = prev_user_id == Some(msg.user_id)
            && prev_created.is_some_and(|prev| (msg.created - prev).num_seconds().abs() < 120);
        let stamp = format!(
            "[{}]",
            crate::app::common::primitives::format_relative_time(msg.created)
        );
        let author = ctx
            .usernames
            .get(&msg.user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(msg.user_id));
        let is_bot = author == "bot" || author == "graybeard";
        let badge = if !is_bot {
            ctx.badges.get(&msg.user_id).copied()
        } else {
            None
        };
        let author_style = if is_own {
            Style::default()
                .fg(theme::AMBER)
                .add_modifier(Modifier::BOLD)
        } else if is_bot {
            Style::default().fg(theme::BOT)
        } else {
            Style::default().fg(theme::CHAT_AUTHOR)
        };
        let body_style = Style::default().fg(theme::CHAT_BODY);
        let streak_badge = badge.map(|b| format!(" {}", b.label())).unwrap_or_default();
        let bonsai_badge = ctx
            .bonsai_glyphs
            .get(&msg.user_id)
            .map(|g| format!(" {}", g))
            .unwrap_or_default();
        let prefix = format!("{author}{streak_badge}{bonsai_badge}");

        let mentions_us = our_mention
            .as_ref()
            .is_some_and(|m| msg.body.contains(m.as_str()));

        if !first && !is_continuation {
            all_rows.push(Line::from(""));
        }
        first = false;

        let row_start = all_rows.len();
        let msg_lines = wrap_chat_entry_to_lines(
            &msg.body,
            &stamp,
            &prefix,
            width,
            author_style,
            body_style,
            mentions_us,
            is_continuation,
        );
        all_rows.extend(msg_lines);

        let body_start = if is_continuation {
            row_start
        } else {
            row_start + 1
        };
        selected_ranges.insert(msg.id, (body_start, all_rows.len()));
        highlighted_ranges.insert(msg.id, (row_start, all_rows.len()));

        prev_user_id = Some(msg.user_id);
        prev_created = Some(msg.created);
    }

    cache.width = width;
    cache.fingerprint = fingerprint;
    cache.all_rows = all_rows;
    cache.selected_ranges = selected_ranges;
    cache.highlighted_ranges = highlighted_ranges;
}

fn visible_chat_rows(
    cache: &ChatRowsCache,
    selected_message_id: Option<Uuid>,
    highlighted_message_id: Option<Uuid>,
    height: usize,
) -> Vec<Line<'static>> {
    let total_rows = cache.all_rows.len();
    if total_rows == 0 {
        return Vec::new();
    }

    let selected_row_range =
        selected_message_id.and_then(|id| cache.selected_ranges.get(&id).copied());
    let highlighted_row_range =
        highlighted_message_id.and_then(|id| cache.highlighted_ranges.get(&id).copied());
    let focus_range = selected_row_range.or(highlighted_row_range);
    let scroll = effective_chat_scroll(total_rows, height, focus_range);
    let visible_end = total_rows.saturating_sub(scroll);
    let visible_start = visible_end.saturating_sub(height);
    let mut lines = cache.all_rows[visible_start..visible_end].to_vec();

    if let Some((start, end)) = highlighted_row_range {
        let start = start.max(visible_start);
        let end = end.min(visible_end);
        for idx in start..end {
            for span in &mut lines[idx - visible_start].spans {
                span.style = span.style.bg(theme::BG_SELECTION);
            }
        }
    }

    if let Some((start, end)) = selected_row_range {
        let start = start.max(visible_start);
        let end = end.min(visible_end);
        for idx in start..end {
            let row = &mut lines[idx - visible_start];
            if let Some(first_span) = row.spans.first()
                && (first_span.content == " " || first_span.content == "│")
            {
                row.spans[0] = Span::styled("▸", Style::default().fg(theme::AMBER));
            }
        }
    }

    if lines.len() < height {
        let pad = height - lines.len();
        let mut padded = vec![Line::from(""); pad];
        padded.append(&mut lines);
        return padded;
    }

    lines
}

fn effective_chat_scroll(
    total_rows: usize,
    height: usize,
    selected_row_range: Option<(usize, usize)>,
) -> usize {
    const SELECTED_SCROLL_MARGIN: usize = 2;

    let max_scroll = total_rows.saturating_sub(height);
    let scroll = 0;

    let Some((start, end)) = selected_row_range else {
        return scroll;
    };

    let visible_end = total_rows.saturating_sub(scroll);
    let visible_start = visible_end.saturating_sub(height);
    let selected_end = end.min(total_rows);
    let selected_len = selected_end.saturating_sub(start);
    let margin = SELECTED_SCROLL_MARGIN.min(height.saturating_sub(1) / 2);

    let target_end = if selected_len >= height || start < visible_start {
        let target_start = start.saturating_sub(margin);
        (target_start + height).min(total_rows)
    } else if selected_end > visible_end.saturating_sub(margin) {
        (selected_end + margin).min(total_rows)
    } else {
        visible_end
    };

    total_rows.saturating_sub(target_end).min(max_scroll)
}

// ── Small helpers ───────────────────────────────────────────

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

fn dm_label(
    room: &late_core::models::chat_room::ChatRoom,
    current_user_id: Uuid,
    usernames: &HashMap<Uuid, String>,
) -> String {
    let other_id = if room.dm_user_a == Some(current_user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    };
    other_id
        .and_then(|id| usernames.get(&id))
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| "DM".to_string())
}

// ── Mention autocomplete popup ──────────────────────────────

fn draw_mention_autocomplete(frame: &mut Frame, anchor: Rect, matches: &[String], selected: usize) {
    if matches.is_empty() {
        return;
    }

    let visible = matches.len().min(8) as u16;
    let width = 26u16.min(anchor.width);
    let height = visible + 2; // borders
    let x = anchor.x + 1;
    let y = anchor.y.saturating_sub(height);
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" @mentions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE));

    let items: Vec<Line> = matches
        .iter()
        .enumerate()
        .take(8)
        .map(|(i, name)| {
            let style = if i == selected {
                Style::default()
                    .fg(theme::AMBER)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            let prefix = if i == selected { " > " } else { "   " };
            Line::from(Span::styled(format!("{prefix}@{name}"), style))
        })
        .collect();

    frame.render_widget(Paragraph::new(items).block(block), popup);
}

// ── Main chat screen ────────────────────────────────────────

pub struct ChatRenderInput<'a> {
    pub news_selected: bool,
    pub news_unread_count: i64,
    pub news_view: super::news::ui::ArticleListView<'a>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub chat_rooms: &'a [(
        late_core::models::chat_room::ChatRoom,
        Vec<late_core::models::chat_message::ChatMessage>,
    )],
    pub overlay: Option<&'a Overlay>,
    pub usernames: &'a HashMap<Uuid, String>,
    pub badges: &'a HashMap<Uuid, BadgeTier>,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub selected_room_id: Option<Uuid>,
    pub selected_message_id: Option<Uuid>,
    pub highlighted_message_id: Option<Uuid>,
    pub composer: &'a str,
    pub composer_rows: &'a [ComposerRow],
    pub composer_cursor: usize,
    pub composing: bool,
    pub current_user_id: Uuid,
    pub cursor_visible: bool,
    pub mention_matches: &'a [String],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
    pub news_composer: &'a str,
    pub news_composing: bool,
    pub news_processing: bool,
    pub notifications_selected: bool,
    pub notifications_unread_count: i64,
    pub notifications_view: super::notifications::ui::NotificationListView<'a>,
}

pub fn draw_chat(frame: &mut Frame, area: Rect, view: ChatRenderInput<'_>) {
    let chat_rooms = view.chat_rooms;
    let usernames = view.usernames;
    let unread_counts = view.unread_counts;
    let news_unread_count = view.news_unread_count;
    let selected_room_id = view.selected_room_id;
    let composer = view.composer;
    let composing = view.composing;
    let current_user_id = view.current_user_id;
    let news_selected = view.news_selected;
    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if chat_rooms.is_empty() {
        let empty = Paragraph::new("No chat rooms yet.")
            .style(Style::default().fg(theme::TEXT_DIM))
            .centered();
        frame.render_widget(empty, inner);
        return;
    }

    let composer_text_width = inner.width.saturating_sub(3).max(1) as usize;
    let total_composer_lines = if view.notifications_selected {
        1
    } else if news_selected {
        composer_line_count(view.news_composer, composer_text_width)
    } else {
        composer_line_count_for_rows(composer, view.composer_rows)
    };
    let visible_composer_lines = total_composer_lines.min(5);
    let composer_height = visible_composer_lines as u16 + 2;
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(inner);
    let body = layout[0];
    let composer_area = layout[1];
    let body_layout = Layout::horizontal([Constraint::Length(26), Constraint::Fill(1)]).split(body);
    let rooms_area = body_layout[0];
    let messages_area = body_layout[1];

    let room_line =
        |room: &late_core::models::chat_room::ChatRoom, label: String, is_selected: bool| -> Line {
            let unread = unread_counts.get(&room.id).copied().unwrap_or(0);
            let prefix = if is_selected { ">" } else { " " };
            let style = if is_selected {
                Style::default()
                    .fg(theme::AMBER)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            let text = if unread > 0 {
                format!("{prefix} {label} ({unread})")
            } else {
                format!("{prefix} {label}")
            };
            Line::from(Span::styled(text, style))
        };
    let section_divider = |label: &str, width: u16| -> Line {
        let prefix = "── ";
        let suffix_len = (width as usize).saturating_sub(prefix.len() + label.len() + 1); // +1 for space after label
        let suffix: String = "─".repeat(suffix_len);
        Line::from(Span::styled(
            format!("{prefix}{label} {suffix}"),
            Style::default().fg(theme::TEXT_FAINT),
        ))
    };
    let rooms_width = rooms_area.width.saturating_sub(2); // inner width minus borders

    let mut room_lines: Vec<Line> = Vec::new();

    // ── Core (hardcoded order: general, announcements, news) ──
    room_lines.push(section_divider("Core", rooms_width));
    let core_order = ["general", "announcements", "suggestions", "bugs"];
    for slug in &core_order {
        if let Some((room, _)) = chat_rooms
            .iter()
            .find(|(r, _)| r.permanent && r.slug.as_deref() == Some(slug))
        {
            room_lines.push(room_line(
                room,
                slug.to_string(),
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id),
            ));
        }
    }
    // Any other permanent rooms not in the hardcoded list
    for (room, _) in chat_rooms.iter().filter(|(r, _)| {
        r.kind != "dm" && r.permanent && !core_order.contains(&r.slug.as_deref().unwrap_or(""))
    }) {
        let label = room
            .slug
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| room.kind.clone());
        room_lines.push(room_line(
            room,
            label,
            !news_selected && !view.notifications_selected && selected_room_id == Some(room.id),
        ));
    }
    // News virtual room
    {
        let prefix = if news_selected { ">" } else { " " };
        let style = if news_selected {
            Style::default()
                .fg(theme::AMBER)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let label = if news_unread_count > 0 {
            format!("{prefix} news ({news_unread_count})")
        } else {
            format!("{prefix} news")
        };
        room_lines.push(Line::from(Span::styled(label, style)));
    }
    // Mentions / notifications virtual room
    {
        let notifications_selected = view.notifications_selected;
        let notifications_unread_count = view.notifications_unread_count;
        let prefix = if notifications_selected { ">" } else { " " };
        let style = if notifications_selected {
            Style::default()
                .fg(theme::AMBER)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let label = if notifications_unread_count > 0 {
            format!("{prefix} mentions ({notifications_unread_count})")
        } else {
            format!("{prefix} mentions")
        };
        room_lines.push(Line::from(Span::styled(label, style)));
    }

    // ── Rooms (public, via /create, alpha sorted) ──
    let mut public_rooms: Vec<_> = chat_rooms
        .iter()
        .filter(|(r, _)| r.kind != "dm" && !r.permanent && r.auto_join)
        .collect();
    public_rooms.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
    if !public_rooms.is_empty() {
        room_lines.push(Line::from(""));
        room_lines.push(section_divider("Rooms", rooms_width));
        for (room, _) in &public_rooms {
            let label = room
                .slug
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| room.kind.clone());
            room_lines.push(room_line(
                room,
                label,
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id),
            ));
        }
    }

    // ── Private (via /join, alpha sorted) ──
    let mut private_rooms: Vec<_> = chat_rooms
        .iter()
        .filter(|(r, _)| r.kind != "dm" && !r.permanent && !r.auto_join)
        .collect();
    private_rooms.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
    if !private_rooms.is_empty() {
        room_lines.push(Line::from(""));
        room_lines.push(section_divider("Private", rooms_width));
        for (room, _) in &private_rooms {
            let label = room
                .slug
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| room.kind.clone());
            room_lines.push(room_line(
                room,
                label,
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id),
            ));
        }
    }

    // ── DMs (alpha sorted) ──
    let mut dm_rooms: Vec<_> = chat_rooms.iter().filter(|(r, _)| r.kind == "dm").collect();
    dm_rooms.sort_by(|(a, _), (b, _)| {
        let name_a = dm_label(a, current_user_id, usernames);
        let name_b = dm_label(b, current_user_id, usernames);
        name_a.cmp(&name_b)
    });
    if !dm_rooms.is_empty() {
        room_lines.push(Line::from(""));
        room_lines.push(section_divider("DMs", rooms_width));
        for (room, _) in &dm_rooms {
            let label = dm_label(room, current_user_id, usernames);
            room_lines.push(room_line(
                room,
                label,
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id),
            ));
        }
    }

    room_lines.push(Line::from(""));
    room_lines.push(section_divider("Mobile", rooms_width));
    room_lines.push(Line::from(vec![
        Span::styled(" c", Style::default().fg(theme::AMBER_DIM)),
        Span::styled(" open web chat", Style::default().fg(theme::TEXT_DIM)),
    ]));
    room_lines.push(Line::from(Span::styled(
        " 24h link, scan QR",
        Style::default().fg(theme::TEXT_FAINT),
    )));

    let rooms_block = Block::default()
        .title(" Rooms (h/l) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER));
    let rooms_paragraph = Paragraph::new(room_lines).block(rooms_block);
    frame.render_widget(rooms_paragraph, rooms_area);

    if view.notifications_selected {
        super::notifications::ui::draw_notification_list(
            frame,
            messages_area,
            &view.notifications_view,
        );
    } else if news_selected {
        super::news::ui::draw_article_list(frame, messages_area, &view.news_view);
    } else {
        let selected_room = selected_room_id
            .and_then(|id| chat_rooms.iter().find(|(room, _)| room.id == id))
            .or_else(|| chat_rooms.first());

        let (message_title, message_lines): (String, Vec<Line>) =
            if let Some((room, messages)) = selected_room {
                let title = if room.kind == "dm" {
                    let other_id = if room.dm_user_a == Some(current_user_id) {
                        room.dm_user_b
                    } else {
                        room.dm_user_a
                    };
                    other_id
                        .and_then(|id| usernames.get(&id))
                        .map(|name| format!("@{name}"))
                        .unwrap_or_else(|| "DM".to_string())
                } else {
                    room.slug
                        .as_deref()
                        .map(str::to_string)
                        .unwrap_or_else(|| room.kind.clone())
                };
                let height = messages_area.height.saturating_sub(2).max(1) as usize;
                let width = messages_area.width.saturating_sub(2).max(1) as usize;

                ensure_chat_rows_cache(
                    view.rows_cache,
                    messages.iter().collect(),
                    width,
                    ChatRowsContext {
                        current_user_id,
                        usernames,
                        badges: view.badges,
                        bonsai_glyphs: view.bonsai_glyphs,
                    },
                );
                let mut lines = visible_chat_rows(
                    view.rows_cache,
                    view.selected_message_id,
                    view.highlighted_message_id,
                    height,
                );

                if lines.is_empty() {
                    lines = vec![Line::from(Span::styled(
                        "No messages yet",
                        Style::default().fg(theme::TEXT_DIM),
                    ))];
                }
                (format!(" #{} ", title), lines)
            } else {
                (
                    " Messages ".to_string(),
                    vec![Line::from(Span::styled(
                        "Select a room.",
                        Style::default().fg(theme::TEXT_DIM),
                    ))],
                )
            };

        let messages_block = Block::default()
            .title(message_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE));
        let inner_area = messages_block.inner(messages_area);
        let messages_paragraph = Paragraph::new(message_lines).block(messages_block);
        frame.render_widget(messages_paragraph, messages_area);
        if let Some(overlay) = view.overlay {
            draw_overlay(frame, inner_area, overlay);
        }
    }

    if view.notifications_selected {
        let hint_block = Block::default()
            .title(" Mentions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER));
        let hint_text = Paragraph::new(Line::from(Span::styled(
            " j/k navigate · Enter jump to room",
            Style::default().fg(theme::TEXT_DIM),
        )))
        .block(hint_block);
        frame.render_widget(hint_text, composer_area);
    } else if news_selected {
        if view.news_processing || view.news_composing {
            let (title, border_style) = if view.news_processing {
                (
                    " Processing URL... ".to_string(),
                    Style::default().fg(theme::AMBER),
                )
            } else {
                (
                    " Paste URL (Enter submit, Esc cancel) ".to_string(),
                    Style::default().fg(theme::BORDER_ACTIVE),
                )
            };
            let news_block = Block::default()
                .title(title.as_str())
                .borders(Borders::ALL)
                .border_style(border_style);
            let news_composer_lines = build_composer_lines(
                view.news_composer,
                view.news_composer.len(),
                true,
                view.cursor_visible && !view.news_processing,
                composer_text_width,
            );
            let news_paragraph = Paragraph::new(news_composer_lines).block(news_block);
            frame.render_widget(news_paragraph, composer_area);
        } else {
            let hint_block = Block::default()
                .title(" Share URL ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER));
            let hint_text = Paragraph::new(Line::from(Span::styled(
                " j/k navigate · Enter copy · i paste URL",
                Style::default().fg(theme::TEXT_DIM),
            )))
            .block(hint_block);
            frame.render_widget(hint_text, composer_area);
        }
    } else {
        let composer_scroll =
            composer_cursor_scroll_for_rows(view.composer_rows, view.composer_cursor, 5);
        let composer_title = if composing {
            if let Some(author) = view.reply_author {
                format!(" Reply to @{author} (Enter send, Alt+Enter newline, Esc cancel) ")
            } else {
                " Compose (Enter send, Alt+Enter newline, Esc cancel) ".to_string()
            }
        } else {
            " Compose (press i) ".to_string()
        };
        let composer_style = if composing {
            Style::default().fg(theme::BORDER_ACTIVE)
        } else {
            Style::default().fg(theme::BORDER)
        };
        let composer_block = Block::default()
            .title(composer_title.as_str())
            .borders(Borders::ALL)
            .border_style(composer_style);
        let composer_lines = build_composer_lines_from_rows(
            composer,
            view.composer_rows,
            view.composer_cursor,
            composing,
            view.cursor_visible,
        );
        let composer_paragraph = Paragraph::new(composer_lines)
            .block(composer_block)
            .scroll((composer_scroll, 0));
        frame.render_widget(composer_paragraph, composer_area);

        if view.mention_active {
            draw_mention_autocomplete(
                frame,
                composer_area,
                view.mention_matches,
                view.mention_selected,
            );
        }
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_user_id_returns_first_eight_chars() {
        let id = Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap();
        assert_eq!(short_user_id(id), "01234567");
    }

    #[test]
    fn short_user_id_handles_nil() {
        assert_eq!(short_user_id(Uuid::nil()), "00000000");
    }

    #[test]
    fn effective_chat_scroll_keeps_selected_message_off_top_edge() {
        let scroll = effective_chat_scroll(40, 10, Some((24, 25)));
        assert_eq!(scroll, 8);
    }

    #[test]
    fn effective_chat_scroll_keeps_selected_message_off_bottom_edge() {
        let scroll = effective_chat_scroll(40, 10, Some((29, 31)));
        assert_eq!(scroll, 3);
    }
}
