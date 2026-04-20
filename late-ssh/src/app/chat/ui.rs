use late_core::models::chat_message::ChatMessage;
use late_core::models::chat_message_reaction::ChatMessageReactionSummary;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_textarea::TextArea;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use crate::app::common::{
    composer::composer_line_count,
    overlay::{Overlay, draw_overlay},
    theme,
};
use late_core::models::leaderboard::BadgeTier;

use super::state::{MentionMatch, ROOM_JUMP_KEYS};
use super::ui_text::wrap_chat_entry_to_lines;

fn contributor_badge_for_username(username: &str) -> Option<&'static str> {
    match username.trim().to_ascii_lowercase().as_str() {
        "mevanlc" | "yawner" => Some(" 🔧"),
        _ => None,
    }
}

// ── Dashboard chat card ─────────────────────────────────────

pub struct DashboardChatView<'a> {
    pub messages: &'a [ChatMessage],
    pub overlay: Option<&'a Overlay>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub usernames: &'a HashMap<Uuid, String>,
    pub countries: &'a HashMap<Uuid, String>,
    pub badges: &'a HashMap<Uuid, BadgeTier>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub current_user_id: Uuid,
    pub selected_message_id: Option<Uuid>,
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
}

/// Shared composer block rendering for both the dashboard card and the chat
/// page. New composer states (editing, replying, …) wire here once.
pub(super) struct ComposerBlockView<'a> {
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub selected_message: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub mention_active: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
}

/// Pick the longest tier whose display width fits inside a titled `Block`
/// of the given outer `block_width`. Titles sit on the top border between
/// the two corner glyphs, so the available cells are `block_width - 2`.
/// Tiers should be ordered longest → shortest; the last one is returned
/// if none fit (so include `""` as a terminal fallback).
///
/// Padding convention: any " " around the title text (" Compose … ") is
/// baked into the tier string itself, not reserved by this function. We
/// may later want to make "1 col of padding on each side" a style-guide
/// rule enforced by a layout helper (which would shift the budget to
/// `block_width - 4` and strip authored padding). For now, padding is a
/// design choice of the tier-list author. Tradeoffs either way:
///   - padding-in-string: self-documenting ("what you see is what renders")
///     and easy to vary per tier (e.g. drop padding at the tightest tier).
///   - padding-in-layout: centralized, uniform, lets the title be
///     right-aligned or centered without extra machinery.
///
/// Keeping this a free function for now — if a second caller wants the
/// same collapse behavior, promote to a `TitledCollapseBlock` widget that
/// owns the `Block` builder plus the tier list.
fn pick_title_that_fits<'a>(block_width: u16, tiers: &[&'a str]) -> &'a str {
    let available = block_width.saturating_sub(2) as usize;
    tiers
        .iter()
        .copied()
        .find(|t| UnicodeWidthStr::width(*t) <= available)
        .unwrap_or("")
}

fn composer_title(view: &ComposerBlockView<'_>, block_width: u16) -> String {
    if !view.composing {
        return pick_title_that_fits(
            block_width,
            &[" Compose (press i) ", " (press i) ", " i ", ""],
        )
        .to_string();
    }

    if let Some(author) = view.reply_author {
        let long =
            format!(" Reply to @{author} (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel) ");
        let mid = format!(" Reply to @{author} (⏎ send, Alt+S stay, Alt+⏎ newline, Esc cancel) ");
        let short = format!(" Reply to @{author} (⏎ send, Esc cancel) ");
        let minimal = format!(" Reply to @{author} (Esc) ");
        let name_only = format!(" Reply to @{author} ");
        return pick_title_that_fits(
            block_width,
            &[
                long.as_str(),
                mid.as_str(),
                short.as_str(),
                minimal.as_str(),
                name_only.as_str(),
                " Reply ",
                " Esc ",
                "",
            ],
        )
        .to_string();
    }

    if view.is_editing {
        return pick_title_that_fits(
            block_width,
            &[
                " Edit message (Enter save, Alt+S stay, Alt+Enter newline, Esc cancel) ",
                " Edit message (⏎ save, Alt+S stay, Alt+⏎ newline, Esc cancel) ",
                " Edit message (⏎ save, Esc cancel) ",
                " Edit message (Esc) ",
                " Edit message ",
                " Edit ",
                " Esc ",
                "",
            ],
        )
        .to_string();
    }

    pick_title_that_fits(
        block_width,
        &[
            " Compose (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel) ",
            " (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel) ",
            " (⏎ send, Alt+S stay, Alt+⏎ newline, Esc cancel) ",
            " (⏎ send, Esc cancel) ",
            " (Esc cancel) ",
            " Esc ",
            "",
        ],
    )
    .to_string()
}

fn empty_composer_placeholder(view: &ComposerBlockView<'_>) -> Paragraph<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());

    if view.composing {
        return Paragraph::new(Line::from(vec![
            Span::styled(
                "T",
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::TEXT_DIM()),
            ),
            Span::styled("ype a message...", dim),
        ]));
    }

    let placeholder_text = if view.selected_message {
        "1-5 react · r reply · e edit · d delete · p profile · c copy · i compose"
    } else {
        "Type a message · j/k select · /help"
    };

    Paragraph::new(Line::from(Span::styled(placeholder_text, dim)))
}

pub(super) fn draw_composer_block(frame: &mut Frame, area: Rect, view: &ComposerBlockView<'_>) {
    let composer_title = composer_title(view, area.width);
    let composer_style = if view.composing {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER())
    };
    let composer_block = Block::default()
        .title(composer_title.as_str())
        .borders(Borders::ALL)
        .border_style(composer_style);
    let composer_inner = composer_block.inner(area);
    frame.render_widget(composer_block, area);

    let text_area = horizontal_inset(composer_inner, 1);

    if view.composer.is_empty() && !view.mention_active {
        frame.render_widget(empty_composer_placeholder(view), text_area);
    } else {
        frame.render_widget(view.composer, text_area);
    }

    if view.mention_active {
        draw_mention_autocomplete(frame, area, view.mention_matches, view.mention_selected);
    }
}

fn horizontal_inset(rect: Rect, pad: u16) -> Rect {
    let pad = pad.min(rect.width / 2);
    Rect {
        x: rect.x + pad,
        y: rect.y,
        width: rect.width.saturating_sub(pad * 2),
        height: rect.height,
    }
}

fn chat_composer_lines_for_height(textarea: &TextArea<'static>, width: usize) -> usize {
    let text = textarea.lines().join("\n");
    composer_line_count(&text, width)
}

pub fn draw_dashboard_chat_card(frame: &mut Frame, area: Rect, view: DashboardChatView<'_>) {
    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let composer_text_width = inner.width.saturating_sub(4).max(1) as usize;
    let total_composer_lines = chat_composer_lines_for_height(view.composer, composer_text_width);
    let visible_composer_lines = total_composer_lines.min(5);
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
            Style::default().fg(theme::TEXT_DIM()),
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
                countries: view.countries,
                badges: view.badges,
                bonsai_glyphs: view.bonsai_glyphs,
                message_reactions: view.message_reactions,
            },
        );
        lines = visible_chat_rows(view.rows_cache, view.selected_message_id, None, height);
    }

    frame.render_widget(Paragraph::new(lines), messages_area);
    if let Some(overlay) = view.overlay {
        draw_overlay(frame, messages_area, overlay);
    }

    if let Some(area) = composer_area {
        draw_composer_block(
            frame,
            area,
            &ComposerBlockView {
                composer: view.composer,
                composing: view.composing,
                selected_message: view.selected_message_id.is_some(),
                reply_author: view.reply_author,
                is_editing: view.is_editing,
                mention_active: view.mention_active,
                mention_matches: view.mention_matches,
                mention_selected: view.mention_selected,
            },
        );
    }
}

// ── Chat rows cache & scroll ────────────────────────────────

struct ChatRowsContext<'a> {
    current_user_id: Uuid,
    usernames: &'a HashMap<Uuid, String>,
    countries: &'a HashMap<Uuid, String>,
    badges: &'a HashMap<Uuid, BadgeTier>,
    bonsai_glyphs: &'a HashMap<Uuid, String>,
    message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
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
        ctx.countries.get(&msg.user_id).hash(&mut hasher);
        ctx.badges
            .get(&msg.user_id)
            .map(|badge| badge.label())
            .hash(&mut hasher);
        ctx.bonsai_glyphs.get(&msg.user_id).hash(&mut hasher);
        ctx.message_reactions.get(&msg.id).hash(&mut hasher);
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
        let raw_author = ctx
            .usernames
            .get(&msg.user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or("");
        let author = if raw_author.is_empty() {
            short_user_id(msg.user_id)
        } else {
            format_username_with_country(msg.user_id, raw_author, ctx.countries)
        };
        let contributor_badge = contributor_badge_for_username(raw_author).unwrap_or_default();
        let is_bot = raw_author == "bot" || raw_author == "graybeard";
        let badge = if !is_bot {
            ctx.badges.get(&msg.user_id).copied()
        } else {
            None
        };
        let author_style = if is_own {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else if is_bot {
            Style::default().fg(theme::BOT())
        } else {
            Style::default().fg(theme::CHAT_AUTHOR())
        };
        let body_style = Style::default().fg(theme::CHAT_BODY());
        let contributor_badge = if is_bot { "" } else { contributor_badge };
        let streak_badge = badge.map(|b| format!(" {}", b.label())).unwrap_or_default();
        let bonsai_badge = ctx
            .bonsai_glyphs
            .get(&msg.user_id)
            .map(|g| format!(" {}", g))
            .unwrap_or_default();
        let prefix = format!("{author}{contributor_badge}{streak_badge}{bonsai_badge}");
        let reactions = ctx
            .message_reactions
            .get(&msg.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

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
            reactions,
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
                span.style = span.style.bg(theme::BG_SELECTION());
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
                row.spans[0] = Span::styled("▸", Style::default().fg(theme::AMBER()));
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

/// Scroll the rooms sidebar so the selected row stays at or above 2/3 of the
/// visible height. No selection, or a selection that already fits without
/// scrolling, yields 0.
fn rooms_scroll_for_selection(
    total_rows: usize,
    visible_height: usize,
    selected_row_index: Option<usize>,
) -> usize {
    if visible_height == 0 {
        return 0;
    }
    let max_scroll = total_rows.saturating_sub(visible_height);
    let Some(idx) = selected_row_index else {
        return 0;
    };
    let threshold = (visible_height * 2) / 3;
    idx.saturating_sub(threshold).min(max_scroll)
}

// ── Small helpers ───────────────────────────────────────────

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

fn format_username_with_country(
    _user_id: Uuid,
    username: &str,
    _countries: &HashMap<Uuid, String>,
) -> String {
    username.to_string()
}

fn dm_label(
    room: &late_core::models::chat_room::ChatRoom,
    current_user_id: Uuid,
    usernames: &HashMap<Uuid, String>,
    countries: &HashMap<Uuid, String>,
) -> String {
    let other_id = if room.dm_user_a == Some(current_user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    };
    other_id
        .and_then(|id| {
            usernames
                .get(&id)
                .map(|name| format_username_with_country(id, name, countries))
        })
        .unwrap_or_else(|| "DM".to_string())
}

// ── Mention autocomplete popup ──────────────────────────────

fn draw_mention_autocomplete(
    frame: &mut Frame,
    anchor: Rect,
    matches: &[MentionMatch],
    selected: usize,
) {
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
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

    let items: Vec<Line> = matches
        .iter()
        .enumerate()
        .take(8)
        .map(|(i, m)| {
            let is_selected = i == selected;
            let style = match (is_selected, m.online) {
                (true, _) => Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
                (false, true) => Style::default().fg(theme::TEXT()),
                (false, false) => Style::default().fg(theme::TEXT_FAINT()),
            };
            let prefix = if is_selected { " > " } else { "   " };
            Line::from(Span::styled(format!("{prefix}@{}", m.name), style))
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
    pub countries: &'a HashMap<Uuid, String>,
    pub badges: &'a HashMap<Uuid, BadgeTier>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub selected_room_id: Option<Uuid>,
    pub room_jump_active: bool,
    pub selected_message_id: Option<Uuid>,
    pub highlighted_message_id: Option<Uuid>,
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub current_user_id: Uuid,
    pub cursor_visible: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
    pub news_composer: &'a TextArea<'static>,
    pub news_composing: bool,
    pub news_processing: bool,
    pub notifications_selected: bool,
    pub notifications_unread_count: i64,
    pub notifications_view: super::notifications::ui::NotificationListView<'a>,
}

fn room_jump_prefix(key: Option<u8>, active: bool, is_selected: bool) -> String {
    if active {
        key.map(|key| format!("[{}] ", key as char))
            .unwrap_or_else(|| "    ".to_string())
    } else if is_selected {
        "> ".to_string()
    } else {
        "  ".to_string()
    }
}

pub fn draw_chat(frame: &mut Frame, area: Rect, view: ChatRenderInput<'_>) {
    let chat_rooms = view.chat_rooms;
    let usernames = view.usernames;
    let countries = view.countries;
    let unread_counts = view.unread_counts;
    let news_unread_count = view.news_unread_count;
    let selected_room_id = view.selected_room_id;
    let room_jump_active = view.room_jump_active;
    let composer = view.composer;
    let composing = view.composing;
    let current_user_id = view.current_user_id;
    let news_selected = view.news_selected;
    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if chat_rooms.is_empty() {
        let empty = Paragraph::new("No chat rooms yet.")
            .style(Style::default().fg(theme::TEXT_DIM()))
            .centered();
        frame.render_widget(empty, inner);
        return;
    }

    let composer_text_width = inner.width.saturating_sub(4).max(1) as usize;
    let total_composer_lines = if view.notifications_selected {
        1
    } else if news_selected {
        chat_composer_lines_for_height(view.news_composer, composer_text_width)
    } else {
        chat_composer_lines_for_height(composer, composer_text_width)
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
    let mut jump_keys = ROOM_JUMP_KEYS.iter().copied();

    let room_line = |room: &late_core::models::chat_room::ChatRoom,
                     label: String,
                     is_selected: bool,
                     jump_key: Option<u8>|
     -> Line {
        let unread = unread_counts.get(&room.id).copied().unwrap_or(0);
        let style = if is_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let prefix = room_jump_prefix(jump_key, room_jump_active, is_selected);
        let text = if unread > 0 {
            format!("{prefix}{label} ({unread})")
        } else {
            format!("{prefix}{label}")
        };
        Line::from(Span::styled(text, style))
    };
    let section_divider = |label: &str, width: u16| -> Line {
        let prefix = "── ";
        let suffix_len = (width as usize).saturating_sub(prefix.len() + label.len() + 1); // +1 for space after label
        let suffix: String = "─".repeat(suffix_len);
        Line::from(Span::styled(
            format!("{prefix}{label} {suffix}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ))
    };
    let rooms_width = rooms_area.width.saturating_sub(2); // inner width minus borders

    let mut room_lines: Vec<Line> = Vec::new();
    let mut selected_row_index: Option<usize> = None;

    // ── Core (hardcoded order: general, announcements, news) ──
    room_lines.push(section_divider("Core", rooms_width));
    let core_order = ["general", "announcements", "suggestions", "bugs"];
    for slug in &core_order {
        if let Some((room, _)) = chat_rooms
            .iter()
            .find(|(r, _)| r.permanent && r.slug.as_deref() == Some(slug))
        {
            let is_selected =
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id);
            room_lines.push(room_line(
                room,
                slug.to_string(),
                is_selected,
                room_jump_active.then(|| jump_keys.next()).flatten(),
            ));
            if is_selected {
                selected_row_index = Some(room_lines.len() - 1);
            }
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
        let is_selected =
            !news_selected && !view.notifications_selected && selected_room_id == Some(room.id);
        room_lines.push(room_line(
            room,
            label,
            is_selected,
            room_jump_active.then(|| jump_keys.next()).flatten(),
        ));
        if is_selected {
            selected_row_index = Some(room_lines.len() - 1);
        }
    }
    // News virtual room
    {
        let prefix = room_jump_prefix(
            room_jump_active.then(|| jump_keys.next()).flatten(),
            room_jump_active,
            news_selected,
        );
        let style = if news_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let label = if news_unread_count > 0 {
            format!("{prefix}news ({news_unread_count})")
        } else {
            format!("{prefix}news")
        };
        room_lines.push(Line::from(Span::styled(label, style)));
        if news_selected {
            selected_row_index = Some(room_lines.len() - 1);
        }
    }
    // Mentions / notifications virtual room
    {
        let notifications_selected = view.notifications_selected;
        let notifications_unread_count = view.notifications_unread_count;
        let prefix = room_jump_prefix(
            room_jump_active.then(|| jump_keys.next()).flatten(),
            room_jump_active,
            notifications_selected,
        );
        let style = if notifications_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let label = if notifications_unread_count > 0 {
            format!("{prefix}mentions ({notifications_unread_count})")
        } else {
            format!("{prefix}mentions")
        };
        room_lines.push(Line::from(Span::styled(label, style)));
        if notifications_selected {
            selected_row_index = Some(room_lines.len() - 1);
        }
    }

    // ── Rooms (public visibility, alpha sorted) ──
    let mut public_rooms: Vec<_> = chat_rooms
        .iter()
        .filter(|(r, _)| r.kind != "dm" && !r.permanent && r.visibility == "public")
        .collect();
    public_rooms.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
    if !public_rooms.is_empty() {
        room_lines.push(Line::from(""));
        room_lines.push(section_divider("Public", rooms_width));
        for (room, _) in &public_rooms {
            let label = room
                .slug
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| room.kind.clone());
            let is_selected =
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id);
            room_lines.push(room_line(
                room,
                label,
                is_selected,
                room_jump_active.then(|| jump_keys.next()).flatten(),
            ));
            if is_selected {
                selected_row_index = Some(room_lines.len() - 1);
            }
        }
    }

    // ── Private (private visibility, alpha sorted) ──
    let mut private_rooms: Vec<_> = chat_rooms
        .iter()
        .filter(|(r, _)| r.kind != "dm" && !r.permanent && r.visibility == "private")
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
            let is_selected =
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id);
            room_lines.push(room_line(
                room,
                label,
                is_selected,
                room_jump_active.then(|| jump_keys.next()).flatten(),
            ));
            if is_selected {
                selected_row_index = Some(room_lines.len() - 1);
            }
        }
    }

    // ── DMs (alpha sorted) ──
    let mut dm_rooms: Vec<_> = chat_rooms.iter().filter(|(r, _)| r.kind == "dm").collect();
    dm_rooms.sort_by(|(a, _), (b, _)| {
        let name_a = dm_label(a, current_user_id, usernames, countries);
        let name_b = dm_label(b, current_user_id, usernames, countries);
        name_a.cmp(&name_b)
    });
    if !dm_rooms.is_empty() {
        room_lines.push(Line::from(""));
        room_lines.push(section_divider("DMs", rooms_width));
        for (room, _) in &dm_rooms {
            let label = dm_label(room, current_user_id, usernames, countries);
            let is_selected =
                !news_selected && !view.notifications_selected && selected_room_id == Some(room.id);
            room_lines.push(room_line(
                room,
                label,
                is_selected,
                room_jump_active.then(|| jump_keys.next()).flatten(),
            ));
            if is_selected {
                selected_row_index = Some(room_lines.len() - 1);
            }
        }
    }

    room_lines.push(Line::from(""));
    room_lines.push(section_divider("Mobile", rooms_width));
    room_lines.push(Line::from(vec![
        Span::styled(" c", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" open web chat", Style::default().fg(theme::TEXT_DIM())),
    ]));
    room_lines.push(Line::from(Span::styled(
        " 24h link, scan QR",
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    let rooms_block = Block::default()
        .title(if room_jump_active {
            " Rooms (h/l) Space/Esc cancel jump "
        } else {
            " Rooms (h/l) Space jump "
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let rooms_scroll = rooms_scroll_for_selection(
        room_lines.len(),
        rooms_area.height.saturating_sub(2) as usize,
        selected_row_index,
    );
    let rooms_paragraph = Paragraph::new(room_lines)
        .block(rooms_block)
        .scroll((rooms_scroll as u16, 0));
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
                        .and_then(|id| {
                            usernames
                                .get(&id)
                                .map(|name| format_username_with_country(id, name, countries))
                        })
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
                        countries,
                        badges: view.badges,
                        bonsai_glyphs: view.bonsai_glyphs,
                        message_reactions: view.message_reactions,
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
                        Style::default().fg(theme::TEXT_DIM()),
                    ))];
                }
                (format!(" #{} ", title), lines)
            } else {
                (
                    " Messages ".to_string(),
                    vec![Line::from(Span::styled(
                        "Select a room.",
                        Style::default().fg(theme::TEXT_DIM()),
                    ))],
                )
            };

        let messages_block = Block::default()
            .title(message_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
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
            .border_style(Style::default().fg(theme::BORDER()));
        let hint_text = Paragraph::new(Line::from(Span::styled(
            " j/k navigate · Enter jump to room",
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .block(hint_block);
        frame.render_widget(hint_text, composer_area);
    } else if news_selected {
        if view.news_processing || view.news_composing {
            let (title, border_style) = if view.news_processing {
                (
                    " Processing URL... ".to_string(),
                    Style::default().fg(theme::AMBER()),
                )
            } else {
                (
                    " Paste URL (Enter submit, Esc cancel) ".to_string(),
                    Style::default().fg(theme::BORDER_ACTIVE()),
                )
            };
            let news_block = Block::default()
                .title(title.as_str())
                .borders(Borders::ALL)
                .border_style(border_style);
            let news_inner = news_block.inner(composer_area);
            frame.render_widget(news_block, composer_area);
            let text_area = horizontal_inset(news_inner, 1);
            frame.render_widget(view.news_composer, text_area);
        } else {
            let hint_block = Block::default()
                .title(" Share URL ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER()));
            let hint_text = Paragraph::new(Line::from(Span::styled(
                " j/k navigate · Enter copy · i paste URL",
                Style::default().fg(theme::TEXT_DIM()),
            )))
            .block(hint_block);
            frame.render_widget(hint_text, composer_area);
        }
    } else {
        draw_composer_block(
            frame,
            composer_area,
            &ComposerBlockView {
                composer,
                composing,
                selected_message: view.selected_message_id.is_some(),
                reply_author: view.reply_author,
                is_editing: view.is_editing,
                mention_active: view.mention_active,
                mention_matches: view.mention_matches,
                mention_selected: view.mention_selected,
            },
        );
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

    fn composer_view<'a>(textarea: &'a TextArea<'static>) -> ComposerBlockView<'a> {
        ComposerBlockView {
            composer: textarea,
            composing: true,
            selected_message: false,
            reply_author: None,
            is_editing: false,
            mention_active: false,
            mention_matches: &[],
            mention_selected: 0,
        }
    }

    #[test]
    fn pick_title_that_fits_selects_longest_tier_that_fits() {
        let tiers = ["aaaaaa", "bbbb", "cc", ""];
        // block_width = N, available for title = N - 2.
        assert_eq!(pick_title_that_fits(8, &tiers), "aaaaaa");
        assert_eq!(pick_title_that_fits(7, &tiers), "bbbb");
        assert_eq!(pick_title_that_fits(5, &tiers), "cc");
        assert_eq!(pick_title_that_fits(3, &tiers), "");
    }

    #[test]
    fn pick_title_that_fits_uses_display_width_not_byte_length() {
        // ⏎ is 3 bytes but 1 display column.
        let tiers = ["⏎⏎⏎⏎", ""];
        assert_eq!(pick_title_that_fits(6, &tiers), "⏎⏎⏎⏎");
    }

    #[test]
    fn composer_title_collapses_across_block_widths() {
        let ta = TextArea::default();
        let view = composer_view(&ta);
        let full = " Compose (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel) ";
        let long = " (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel) ";
        let short = " (⏎ send, Alt+S stay, Alt+⏎ newline, Esc cancel) ";
        let minimal = " (⏎ send, Esc cancel) ";
        let cancel = " (Esc cancel) ";
        let esc = " Esc ";
        let need = |title: &str| (UnicodeWidthStr::width(title) + 2) as u16;

        assert_eq!(composer_title(&view, need(full)), full);
        assert_eq!(composer_title(&view, need(full) - 1), long);

        assert_eq!(composer_title(&view, need(long)), long);
        assert_eq!(composer_title(&view, need(long) - 1), short);

        assert_eq!(composer_title(&view, need(short)), short);
        assert_eq!(composer_title(&view, need(short) - 1), minimal);

        assert_eq!(composer_title(&view, need(minimal)), minimal);
        assert_eq!(composer_title(&view, need(minimal) - 1), cancel);

        assert_eq!(composer_title(&view, need(cancel)), cancel);
        assert_eq!(composer_title(&view, need(cancel) - 1), esc);

        assert_eq!(composer_title(&view, need(esc)), esc);
        assert_eq!(composer_title(&view, need(esc) - 1), "");
    }

    #[test]
    fn composer_title_reply_state_degrades_through_name_only_and_label() {
        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.reply_author = Some("alice");
        assert_eq!(
            composer_title(&view, 100),
            " Reply to @alice (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel) "
        );
        // Far too narrow for even the shortest reply form → drops to " Reply ".
        // " Reply " = 7 cols → needs block_w ≥ 9.
        assert_eq!(composer_title(&view, 10), " Reply ");
        assert_eq!(composer_title(&view, 9), " Reply ");
        // " Esc " = 5 cols → needs block_w ≥ 7.
        assert_eq!(composer_title(&view, 8), " Esc ");
        assert_eq!(composer_title(&view, 7), " Esc ");
        assert_eq!(composer_title(&view, 6), "");
    }

    #[test]
    fn composer_title_when_not_composing_shows_press_i_prompt() {
        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.composing = false;
        assert_eq!(composer_title(&view, 30), " Compose (press i) ");
        assert_eq!(composer_title(&view, 13), " (press i) ");
        // " i " = 3 cols → needs block_w ≥ 5.
        assert_eq!(composer_title(&view, 5), " i ");
        assert_eq!(composer_title(&view, 4), "");
    }

    #[test]
    fn composer_title_never_truncates_across_block_widths() {
        use ratatui::{Terminal, backend::TestBackend};
        // Render the composer block at every block width where a non-empty
        // title is expected (≥7 for the " Esc " fallback). At each width,
        // confirm the picked title survives intact in the top border row.
        let ta = TextArea::default();
        let view = composer_view(&ta);
        for block_w in 7u16..=120 {
            let backend = TestBackend::new(block_w, 3);
            let mut terminal = Terminal::new(backend).expect("term");
            let expected_title = composer_title(&view, block_w);
            terminal
                .draw(|f| draw_composer_block(f, Rect::new(0, 0, block_w, 3), &view))
                .unwrap();
            let buf = terminal.backend().buffer();
            let row: String = (0..block_w)
                .map(|x| buf[(x, 0)].symbol().to_string())
                .collect();
            assert!(
                row.contains(&expected_title),
                "title {expected_title:?} truncated at block_w={block_w}: rendered {row:?}",
            );
        }
    }

    #[test]
    fn empty_composer_placeholder_is_dim_while_composing() {
        use ratatui::{Terminal, backend::TestBackend};

        let ta = TextArea::default();
        let view = composer_view(&ta);
        let placeholder = empty_composer_placeholder(&view);
        let width = 20u16;
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("term");

        terminal
            .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
            .unwrap();

        let buf = terminal.backend().buffer();
        let rendered: String = (0..17).map(|x| buf[(x, 0)].symbol()).collect();
        assert_eq!(rendered, "Type a message...");
        assert_eq!(buf[(0, 0)].fg, theme::BG_CANVAS());
        assert_eq!(buf[(0, 0)].bg, theme::TEXT_DIM());
        assert_eq!(buf[(1, 0)].fg, theme::TEXT_DIM());
    }

    #[test]
    fn empty_composer_placeholder_uses_hint_text_when_not_composing() {
        use ratatui::{Terminal, backend::TestBackend};

        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.composing = false;

        let placeholder = empty_composer_placeholder(&view);
        let expected = "Type a message · j/k select · /help";
        let width = expected.chars().count() as u16;
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("term");

        terminal
            .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
            .unwrap();

        let buf = terminal.backend().buffer();
        let rendered: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();
        assert_eq!(rendered, expected);
        assert_eq!(buf[(0, 0)].fg, theme::TEXT_DIM());
    }

    #[test]
    fn rooms_scroll_keeps_selection_above_two_thirds_threshold() {
        // height=9 → threshold = 6. idx=6 still fits without scroll.
        assert_eq!(rooms_scroll_for_selection(20, 9, Some(6)), 0);
        // idx=7 passes the threshold → scroll by 1.
        assert_eq!(rooms_scroll_for_selection(20, 9, Some(7)), 1);
        // Selections near the end clamp to max_scroll = total - height.
        assert_eq!(rooms_scroll_for_selection(20, 9, Some(19)), 11);
    }

    #[test]
    fn rooms_scroll_with_no_selection_does_not_scroll() {
        assert_eq!(rooms_scroll_for_selection(50, 10, None), 0);
    }

    #[test]
    fn rooms_scroll_when_content_fits_returns_zero() {
        assert_eq!(rooms_scroll_for_selection(5, 10, Some(4)), 0);
    }

    #[test]
    fn room_jump_prefix_shows_jump_key_when_active() {
        assert_eq!(room_jump_prefix(Some(b'a'), true, false), "[a] ");
    }

    #[test]
    fn room_jump_prefix_shows_selected_marker_when_inactive() {
        assert_eq!(room_jump_prefix(None, false, true), "> ");
        assert_eq!(room_jump_prefix(None, false, false), "  ");
    }
}
