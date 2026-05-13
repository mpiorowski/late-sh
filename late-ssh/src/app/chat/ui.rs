use late_core::models::chat_message_reaction::ChatMessageReactionSummary;
use late_core::models::{chat_message::ChatMessage, chat_room::ChatRoom};
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

use super::state::{
    MentionMatch, ROOM_JUMP_KEYS, RoomSlot, SelectedRoomSlotState, is_chat_list_room,
    is_selected_slot, visual_order_for_rooms,
};
use super::ui_text::{reaction_label, wrap_chat_entry_to_lines};

const REACTION_PICKER_KEYS: [i16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const VOICE_DISCORD_INVITE: &str = "discord.gg/ZDSyxSX7hk";

fn is_bot_author(username: &str) -> bool {
    matches!(
        username.trim().to_ascii_lowercase().as_str(),
        "bot" | "graybeard" | "dealer"
    )
}

// ── Dashboard chat card ─────────────────────────────────────

pub struct DashboardChatView<'a> {
    pub messages: &'a [ChatMessage],
    pub overlay: Option<&'a Overlay>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub usernames: &'a HashMap<Uuid, String>,
    pub countries: &'a HashMap<Uuid, String>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub current_user_id: Uuid,
    pub selected_message_id: Option<Uuid>,
    pub selected_news_message: bool,
    pub highlighted_message_id: Option<Uuid>,
    pub reaction_picker_active: bool,
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
    pub inline_images: &'a HashMap<Uuid, Vec<Line<'static>>>,
}

/// Shared composer block rendering for both the dashboard card and the chat
/// page. New composer states (editing, replying, …) wire here once.
pub(super) struct ComposerBlockView<'a> {
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub selected_message: bool,
    pub selected_news_message: bool,
    pub reaction_picker_active: bool,
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
    let picked = pick_composer_title_text(view, block_width);
    if picked.is_empty() {
        String::new()
    } else {
        format!("──{picked}")
    }
}

fn pick_composer_title_text(view: &ComposerBlockView<'_>, block_width: u16) -> String {
    if !view.composing {
        return pick_title_that_fits(
            block_width,
            &[" Compose (press i) ", " (press i) ", " i ", ""],
        )
        .to_string();
    }

    if let Some(author) = view.reply_author {
        let long = format!(
            " Reply to @{author} (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) "
        );
        let mid =
            format!(" Reply to @{author} (⏎ send, Alt+S stay, Alt+⏎/Ctrl+J newline, Esc cancel) ");
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
                " Edit message (Enter save, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ",
                " Edit message (⏎ save, Alt+S stay, Alt+⏎/Ctrl+J newline, Esc cancel) ",
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
            " Compose (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ",
            " (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ",
            " (⏎ send, Alt+S stay, Alt+⏎/Ctrl+J newline, Esc cancel) ",
            " Compose (Enter send, Esc cancel) ",
            " (⏎ send, Esc cancel) ",
            " (Esc cancel) ",
            " Esc ",
            "",
        ],
    )
    .to_string()
}

fn reaction_picker_placeholder_lines(dim: Style) -> Vec<Line<'static>> {
    let mut reaction_spans = Vec::new();
    for (index, key) in REACTION_PICKER_KEYS.iter().copied().enumerate() {
        if index > 0 {
            reaction_spans.push(Span::styled("  ", dim));
        }
        reaction_spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
        reaction_spans.push(Span::styled(" ", dim));
        reaction_spans.push(Span::styled(reaction_label(key), dim));
    }
    reaction_spans.push(Span::styled("  ", dim));
    reaction_spans.push(Span::styled(
        "f",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    reaction_spans.push(Span::styled(" list", dim));

    vec![Line::from(reaction_spans)]
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

    let placeholder = if view.reaction_picker_active {
        reaction_picker_placeholder_lines(dim)
    } else if view.selected_news_message {
        vec![Line::from(Span::styled(
            "f react · r reply · e edit · d delete · p profile · c copy · Enter view/copy link",
            dim,
        ))]
    } else if view.selected_message {
        vec![Line::from(Span::styled(
            "f react · r reply · e edit · d delete · p profile · c copy · Enter jump to reply",
            dim,
        ))]
    } else {
        vec![Line::from(Span::styled(
            "Type a message · j/k select · Ctrl+/ jump room · or just ask @bot about anything",
            dim,
        ))]
    };

    Paragraph::new(placeholder)
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
        .borders(Borders::TOP | Borders::BOTTOM)
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

pub(crate) fn chat_composer_lines_for_height(textarea: &TextArea<'static>, width: usize) -> usize {
    let text = textarea.lines().join("\n");
    composer_line_count(&text, width)
}

pub(crate) fn chat_composer_placeholder_lines(
    composer: &TextArea<'static>,
    mention_active: bool,
    reaction_picker_active: bool,
) -> usize {
    if composer.is_empty() && !mention_active && reaction_picker_active {
        reaction_picker_placeholder_lines(Style::default()).len()
    } else {
        0
    }
}

fn composer_placeholder_lines(view: &ComposerBlockView<'_>) -> usize {
    chat_composer_placeholder_lines(
        view.composer,
        view.mention_active,
        view.reaction_picker_active,
    )
}

pub fn draw_dashboard_chat_card(frame: &mut Frame, area: Rect, view: DashboardChatView<'_>) {
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let total_composer_lines = chat_composer_lines_for_height(view.composer, composer_text_width)
        .max(composer_placeholder_lines(&ComposerBlockView {
            composer: view.composer,
            composing: view.composing,
            selected_message: view.selected_message_id.is_some(),
            selected_news_message: view.selected_news_message,
            reaction_picker_active: view.reaction_picker_active,
            reply_author: view.reply_author,
            is_editing: view.is_editing,
            mention_active: view.mention_active,
            mention_matches: view.mention_matches,
            mention_selected: view.mention_selected,
        }));
    let visible_composer_lines = total_composer_lines.min(5);
    let composer_height = visible_composer_lines as u16 + 2;
    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(composer_height),
    ])
    .split(area);
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
                bonsai_glyphs: view.bonsai_glyphs,
                message_reactions: view.message_reactions,
                inline_images: view.inline_images,
            },
        );
        lines = visible_chat_rows(
            view.rows_cache,
            view.selected_message_id,
            view.highlighted_message_id,
            height,
        );
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
                selected_news_message: view.selected_news_message,
                reaction_picker_active: view.reaction_picker_active,
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
    bonsai_glyphs: &'a HashMap<Uuid, String>,
    message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    inline_images: &'a HashMap<Uuid, Vec<Line<'static>>>,
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
    theme::current_kind().hash(&mut hasher);
    // Include current minute so relative timestamps ("5 mins ago") stay fresh.
    (chrono::Utc::now().timestamp() / 60).hash(&mut hasher);

    for msg in messages {
        msg.id.hash(&mut hasher);
        msg.user_id.hash(&mut hasher);
        msg.created.hash(&mut hasher);
        msg.body.hash(&mut hasher);
        ctx.usernames.get(&msg.user_id).hash(&mut hasher);
        ctx.countries.get(&msg.user_id).hash(&mut hasher);
        ctx.bonsai_glyphs.get(&msg.user_id).hash(&mut hasher);
        ctx.message_reactions.get(&msg.id).hash(&mut hasher);
        if let Some(lines) = ctx.inline_images.get(&msg.id) {
            true.hash(&mut hasher);
            lines.len().hash(&mut hasher);
            lines
                .iter()
                .map(|line| line.spans.len())
                .sum::<usize>()
                .hash(&mut hasher);
        } else {
            false.hash(&mut hasher);
        }
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
        let is_bot = is_bot_author(raw_author);
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
        let bonsai_badge = ctx
            .bonsai_glyphs
            .get(&msg.user_id)
            .map(|g| format!(" {}", g))
            .unwrap_or_default();
        let prefix = format!("{author}{bonsai_badge}");
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
            ctx.inline_images.get(&msg.id).map(Vec::as_slice),
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

/// Scroll the rooms sidebar so the selected row lands near the vertical
/// center when the list is longer than the visible rail.
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
    let anchor = visible_height / 2;
    idx.saturating_sub(anchor).min(max_scroll)
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

// ── Mention autocomplete popup ──────────────────────────────

pub(crate) fn draw_mention_autocomplete(
    frame: &mut Frame,
    anchor: Rect,
    matches: &[MentionMatch],
    selected: usize,
) {
    if matches.is_empty() {
        return;
    }

    let visible_count = matches.len().min(8);
    let visible = visible_count as u16;
    let first_prefix = matches.first().map(|m| m.prefix).unwrap_or("@");
    let is_commands = first_prefix == "/";
    let width = if is_commands { 52 } else { 26 }.min(anchor.width);
    let height = visible + 2; // borders
    let x = anchor.x + 1;
    let y = anchor.y.saturating_sub(height);
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);

    let title = match first_prefix {
        "/" => " /commands ",
        "#" => " #rooms ",
        _ => " @mentions ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

    let items: Vec<Line> = matches
        .iter()
        .enumerate()
        .skip(selected.saturating_sub(visible_count.saturating_sub(1)))
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
            let mut spans = vec![Span::styled(
                format!("{prefix}{}{}", m.prefix, m.name),
                style,
            )];
            if let Some(description) = m.description {
                let name_width = m.prefix.len() + m.name.len();
                let pad = " ".repeat(16usize.saturating_sub(name_width).max(2));
                spans.push(Span::styled(pad, Style::default().fg(theme::TEXT_DIM())));
                spans.push(Span::styled(
                    description,
                    Style::default().fg(theme::TEXT_DIM()),
                ));
            }
            Line::from(spans)
        })
        .collect();

    frame.render_widget(Paragraph::new(items).block(block), popup);
}

// ── Main chat screen ────────────────────────────────────────

pub struct ChatRenderInput<'a> {
    pub feeds_selected: bool,
    pub feeds_processing: bool,
    pub feeds_unread_count: i64,
    pub feeds_view: super::feeds::ui::FeedListView<'a>,
    pub news_selected: bool,
    pub news_unread_count: i64,
    pub news_view: super::news::ui::ArticleListView<'a>,
    pub discover_selected: bool,
    pub discover_view: super::discover::ui::DiscoverListView<'a>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub chat_rooms: &'a [(
        late_core::models::chat_room::ChatRoom,
        Vec<late_core::models::chat_message::ChatMessage>,
    )],
    pub overlay: Option<&'a Overlay>,
    pub usernames: &'a HashMap<Uuid, String>,
    pub countries: &'a HashMap<Uuid, String>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub inline_images: &'a HashMap<Uuid, Vec<Line<'static>>>,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub favorite_room_ids: &'a [Uuid],
    pub selected_room_id: Option<Uuid>,
    pub room_jump_active: bool,
    pub selected_message_id: Option<Uuid>,
    pub selected_news_message: bool,
    pub reaction_picker_active: bool,
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
    pub showcase_selected: bool,
    pub showcase_unread_count: i64,
    pub showcase_view: super::showcase::ui::ShowcaseListView<'a>,
    pub showcase_state: Option<&'a super::showcase::state::State>,
    pub showcase_composing: bool,
    pub work_selected: bool,
    pub work_unread_count: i64,
    pub work_view: super::work::ui::WorkListView<'a>,
    pub work_state: Option<&'a super::work::state::State>,
    pub work_composing: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChatSelectionMode {
    Compact,
    Composer { lines: usize, max_lines: usize },
}

impl ChatSelectionMode {
    fn composer_height(self) -> u16 {
        let lines = match self {
            Self::Compact => 1,
            Self::Composer { lines, max_lines } => lines.min(max_lines),
        };
        lines as u16 + 2
    }
}

pub(crate) struct ChatRoomListView<'a> {
    pub chat_rooms: &'a [(ChatRoom, Vec<ChatMessage>)],
    pub usernames: &'a HashMap<Uuid, String>,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub favorite_room_ids: &'a [Uuid],
    pub selected_room_id: Option<Uuid>,
    pub room_jump_active: bool,
    pub current_user_id: Uuid,
    pub feeds_available: bool,
    pub feeds_selected: bool,
    pub feeds_unread_count: i64,
    pub news_selected: bool,
    pub news_unread_count: i64,
    pub notifications_selected: bool,
    pub notifications_unread_count: i64,
    pub discover_selected: bool,
    pub showcase_selected: bool,
    pub showcase_unread_count: i64,
    pub work_selected: bool,
    pub work_unread_count: i64,
}

pub struct EmbeddedRoomChatView<'a> {
    pub title: &'a str,
    pub messages: &'a [ChatMessage],
    pub overlay: Option<&'a Overlay>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub usernames: &'a HashMap<Uuid, String>,
    pub countries: &'a HashMap<Uuid, String>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub inline_images: &'a HashMap<Uuid, Vec<Line<'static>>>,
    pub current_user_id: Uuid,
    pub selected_message_id: Option<Uuid>,
    pub highlighted_message_id: Option<Uuid>,
    pub reaction_picker_active: bool,
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
}

pub fn draw_embedded_room_chat(frame: &mut Frame, area: Rect, view: EmbeddedRoomChatView<'_>) {
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let total_composer_lines = chat_composer_lines_for_height(view.composer, composer_text_width)
        .max(composer_placeholder_lines(&ComposerBlockView {
            composer: view.composer,
            composing: view.composing,
            selected_message: view.selected_message_id.is_some(),
            selected_news_message: false,
            reaction_picker_active: view.reaction_picker_active,
            reply_author: view.reply_author,
            is_editing: view.is_editing,
            mention_active: view.mention_active,
            mention_matches: view.mention_matches,
            mention_selected: view.mention_selected,
        }));
    let composer_height = total_composer_lines.min(4) as u16 + 2;
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(area);
    let messages_area = layout[0];
    let composer_area = layout[1];

    let messages_block = Block::default()
        .title(format!("── {} ", view.title))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER()));
    let messages_inner = messages_block.inner(messages_area);
    let messages_text_area = horizontal_inset(messages_inner, 1);

    let height = messages_text_area.height.max(1) as usize;
    let width = messages_text_area.width.max(1) as usize;
    ensure_chat_rows_cache(
        view.rows_cache,
        view.messages.iter().collect(),
        width,
        ChatRowsContext {
            current_user_id: view.current_user_id,
            usernames: view.usernames,
            countries: view.countries,
            bonsai_glyphs: view.bonsai_glyphs,
            message_reactions: view.message_reactions,
            inline_images: view.inline_images,
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

    frame.render_widget(messages_block, messages_area);
    frame.render_widget(Paragraph::new(lines), messages_text_area);
    if let Some(overlay) = view.overlay {
        draw_overlay(frame, messages_text_area, overlay);
    }

    draw_composer_block(
        frame,
        composer_area,
        &ComposerBlockView {
            composer: view.composer,
            composing: view.composing,
            selected_message: view.selected_message_id.is_some(),
            selected_news_message: false,
            reaction_picker_active: view.reaction_picker_active,
            reply_author: view.reply_author,
            is_editing: view.is_editing,
            mention_active: view.mention_active,
            mention_matches: view.mention_matches,
            mention_selected: view.mention_selected,
        },
    );
}

struct RoomListRows {
    lines: Vec<Line<'static>>,
    hit_slots: Vec<Option<RoomSlot>>,
    selected_row_index: Option<usize>,
}

#[cfg(test)]
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

fn chat_selection_mode(view: &ChatRenderInput<'_>, area: Rect) -> ChatSelectionMode {
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    if view.notifications_selected || view.discover_selected || view.feeds_selected {
        ChatSelectionMode::Compact
    } else if view.news_selected {
        ChatSelectionMode::Composer {
            lines: chat_composer_lines_for_height(view.news_composer, composer_text_width),
            max_lines: 8,
        }
    } else if view.showcase_selected {
        ChatSelectionMode::Composer {
            lines: if view.showcase_composing { 8 } else { 1 },
            max_lines: 8,
        }
    } else if view.work_selected {
        ChatSelectionMode::Composer {
            lines: if view.work_composing { 9 } else { 1 },
            max_lines: 9,
        }
    } else {
        ChatSelectionMode::Composer {
            lines: chat_composer_lines_for_height(view.composer, composer_text_width).max(
                composer_placeholder_lines(&ComposerBlockView {
                    composer: view.composer,
                    composing: view.composing,
                    selected_message: view.selected_message_id.is_some(),
                    selected_news_message: view.selected_news_message,
                    reaction_picker_active: view.reaction_picker_active,
                    reply_author: view.reply_author,
                    is_editing: view.is_editing,
                    mention_active: view.mention_active,
                    mention_matches: view.mention_matches,
                    mention_selected: view.mention_selected,
                }),
            ),
            max_lines: 8,
        }
    }
}

#[cfg(test)]
fn chat_layout_for_selection(
    area: Rect,
    selection_mode: ChatSelectionMode,
) -> (Rect, Rect, Rect, Rect) {
    let composer_height = selection_mode.composer_height();
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(area);
    let body = layout[0];
    let composer_area = layout[1];
    let body_layout = Layout::horizontal([Constraint::Length(26), Constraint::Fill(1)]).split(body);
    (body, body_layout[0], body_layout[1], composer_area)
}

#[cfg(test)]
pub(crate) fn room_list_area(area: Rect, selection_mode: ChatSelectionMode) -> Rect {
    let (_, rooms_area, _, _) = chat_layout_for_selection(area, selection_mode);
    rooms_area
}

fn room_list_view_from_render_input<'a>(view: &'a ChatRenderInput<'a>) -> ChatRoomListView<'a> {
    ChatRoomListView {
        chat_rooms: view.chat_rooms,
        usernames: view.usernames,
        unread_counts: view.unread_counts,
        favorite_room_ids: view.favorite_room_ids,
        selected_room_id: view.selected_room_id,
        room_jump_active: view.room_jump_active,
        current_user_id: view.current_user_id,
        feeds_available: view.feeds_view.has_feeds,
        feeds_selected: view.feeds_selected,
        feeds_unread_count: view.feeds_unread_count,
        news_selected: view.news_selected,
        news_unread_count: view.news_unread_count,
        notifications_selected: view.notifications_selected,
        notifications_unread_count: view.notifications_unread_count,
        discover_selected: view.discover_selected,
        showcase_selected: view.showcase_selected,
        showcase_unread_count: view.showcase_unread_count,
        work_selected: view.work_selected,
        work_unread_count: view.work_unread_count,
    }
}

#[cfg(test)]
fn build_room_list_rows(view: &ChatRoomListView<'_>, rooms_area: Rect) -> RoomListRows {
    let chat_rooms = view.chat_rooms;
    let rooms_width = rooms_area.width.saturating_sub(2);
    let mut jump_keys = ROOM_JUMP_KEYS.iter().copied();
    let mut lines = Vec::new();
    let mut hit_slots = Vec::new();
    let mut selected_row_index = None;

    let mut push_row = |line: Line<'static>, slot: Option<RoomSlot>, selected: bool| {
        lines.push(line);
        hit_slots.push(slot);
        if selected {
            selected_row_index = Some(lines.len() - 1);
        }
    };

    let room_line = |room: &ChatRoom,
                     label: String,
                     is_selected: bool,
                     jump_key: Option<u8>|
     -> Line<'static> {
        let unread = view.unread_counts.get(&room.id).copied().unwrap_or(0);
        let style = if is_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let prefix = room_jump_prefix(jump_key, view.room_jump_active, is_selected);
        let text = if unread > 0 {
            format!("{prefix}{label} ({unread})")
        } else {
            format!("{prefix}{label}")
        };
        Line::from(Span::styled(text, style))
    };
    let section_divider = |label: &str| -> Line<'static> {
        let prefix = "── ";
        let suffix_len = (rooms_width as usize).saturating_sub(prefix.len() + label.len() + 1);
        let suffix = "─".repeat(suffix_len);
        Line::from(Span::styled(
            format!("{prefix}{label} {suffix}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ))
    };

    let room_selected = |room_id| {
        !view.feeds_selected
            && !view.news_selected
            && !view.notifications_selected
            && !view.discover_selected
            && !view.showcase_selected
            && !view.work_selected
            && view.selected_room_id == Some(room_id)
    };

    push_row(section_divider("Core"), None, false);
    let core_order = ["general", "announcements", "suggestions", "bugs"];
    for slug in &core_order {
        if let Some((room, _)) = chat_rooms
            .iter()
            .find(|(r, _)| is_chat_list_room(r) && r.permanent && r.slug.as_deref() == Some(slug))
        {
            let is_selected = room_selected(room.id);
            push_row(
                room_line(
                    room,
                    room_display_label(room, view.usernames, view.current_user_id),
                    is_selected,
                    view.room_jump_active.then(|| jump_keys.next()).flatten(),
                ),
                Some(RoomSlot::Room(room.id)),
                is_selected,
            );
        }
    }
    for (room, _) in chat_rooms.iter().filter(|(r, _)| {
        is_chat_list_room(r)
            && r.kind != "dm"
            && r.permanent
            && !core_order.contains(&r.slug.as_deref().unwrap_or(""))
    }) {
        let is_selected = room_selected(room.id);
        push_row(
            room_line(
                room,
                room_display_label(room, view.usernames, view.current_user_id),
                is_selected,
                view.room_jump_active.then(|| jump_keys.next()).flatten(),
            ),
            Some(RoomSlot::Room(room.id)),
            is_selected,
        );
    }

    if view.feeds_available {
        let feeds_line = {
            let prefix = room_jump_prefix(
                view.room_jump_active.then(|| jump_keys.next()).flatten(),
                view.room_jump_active,
                view.feeds_selected,
            );
            let style = if view.feeds_selected {
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let label = if view.feeds_unread_count > 0 {
                format!("{prefix}feeds ({})", view.feeds_unread_count)
            } else {
                format!("{prefix}feeds")
            };
            Line::from(Span::styled(label, style))
        };
        push_row(feeds_line, Some(RoomSlot::Feeds), view.feeds_selected);
    }

    let news_line = {
        let prefix = room_jump_prefix(
            view.room_jump_active.then(|| jump_keys.next()).flatten(),
            view.room_jump_active,
            view.news_selected,
        );
        let style = if view.news_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let label = if view.news_unread_count > 0 {
            format!("{prefix}news ({})", view.news_unread_count)
        } else {
            format!("{prefix}news")
        };
        Line::from(Span::styled(label, style))
    };
    push_row(news_line, Some(RoomSlot::News), view.news_selected);

    let showcase_line = {
        let prefix = room_jump_prefix(
            view.room_jump_active.then(|| jump_keys.next()).flatten(),
            view.room_jump_active,
            view.showcase_selected,
        );
        let style = if view.showcase_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let label = if view.showcase_unread_count > 0 {
            format!("{prefix}showcases ({})", view.showcase_unread_count)
        } else {
            format!("{prefix}showcases")
        };
        Line::from(Span::styled(label, style))
    };
    push_row(
        showcase_line,
        Some(RoomSlot::Showcase),
        view.showcase_selected,
    );

    let work_line = {
        let prefix = room_jump_prefix(
            view.room_jump_active.then(|| jump_keys.next()).flatten(),
            view.room_jump_active,
            view.work_selected,
        );
        let style = if view.work_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let label = if view.work_unread_count > 0 {
            format!("{prefix}work ({})", view.work_unread_count)
        } else {
            format!("{prefix}work")
        };
        Line::from(Span::styled(label, style))
    };
    push_row(work_line, Some(RoomSlot::Work), view.work_selected);

    let notifications_line = {
        let prefix = room_jump_prefix(
            view.room_jump_active.then(|| jump_keys.next()).flatten(),
            view.room_jump_active,
            view.notifications_selected,
        );
        let style = if view.notifications_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let label = if view.notifications_unread_count > 0 {
            format!("{prefix}mentions ({})", view.notifications_unread_count)
        } else {
            format!("{prefix}mentions")
        };
        Line::from(Span::styled(label, style))
    };
    push_row(
        notifications_line,
        Some(RoomSlot::Notifications),
        view.notifications_selected,
    );

    let mut public_rooms: Vec<_> = chat_rooms
        .iter()
        .filter(|(r, _)| {
            is_chat_list_room(r) && r.kind != "dm" && !r.permanent && r.visibility == "public"
        })
        .collect();
    public_rooms.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
    if !public_rooms.is_empty() {
        push_row(Line::from(""), None, false);
        push_row(section_divider("Public"), None, false);
        for (room, _) in &public_rooms {
            let is_selected = room_selected(room.id);
            push_row(
                room_line(
                    room,
                    room_display_label(room, view.usernames, view.current_user_id),
                    is_selected,
                    view.room_jump_active.then(|| jump_keys.next()).flatten(),
                ),
                Some(RoomSlot::Room(room.id)),
                is_selected,
            );
        }
    }

    let mut private_rooms: Vec<_> = chat_rooms
        .iter()
        .filter(|(r, _)| {
            is_chat_list_room(r) && r.kind != "dm" && !r.permanent && r.visibility == "private"
        })
        .collect();
    private_rooms.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
    if !private_rooms.is_empty() {
        push_row(Line::from(""), None, false);
        push_row(section_divider("Private"), None, false);
        for (room, _) in &private_rooms {
            let is_selected = room_selected(room.id);
            push_row(
                room_line(
                    room,
                    room_display_label(room, view.usernames, view.current_user_id),
                    is_selected,
                    view.room_jump_active.then(|| jump_keys.next()).flatten(),
                ),
                Some(RoomSlot::Room(room.id)),
                is_selected,
            );
        }
    }

    let mut dm_rooms: Vec<_> = chat_rooms.iter().filter(|(r, _)| r.kind == "dm").collect();
    dm_rooms.sort_by(|(a, _), (b, _)| {
        let name_a = dm_display_label(a, view.usernames, view.current_user_id);
        let name_b = dm_display_label(b, view.usernames, view.current_user_id);
        name_a.cmp(&name_b)
    });
    if !dm_rooms.is_empty() {
        push_row(Line::from(""), None, false);
        push_row(section_divider("DMs"), None, false);
        for (room, _) in &dm_rooms {
            let is_selected = room_selected(room.id);
            push_row(
                room_line(
                    room,
                    dm_display_label(room, view.usernames, view.current_user_id),
                    is_selected,
                    view.room_jump_active.then(|| jump_keys.next()).flatten(),
                ),
                Some(RoomSlot::Room(room.id)),
                is_selected,
            );
        }
    }

    let browse_rooms_line = {
        let prefix = room_jump_prefix(
            view.room_jump_active.then(|| jump_keys.next()).flatten(),
            view.room_jump_active,
            view.discover_selected,
        );
        let style = if view.discover_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        let label = format!("{prefix}+ browse rooms");
        Line::from(Span::styled(label, style))
    };
    push_row(
        browse_rooms_line,
        Some(RoomSlot::Discover),
        view.discover_selected,
    );

    push_row(Line::from(""), None, false);
    push_row(section_divider("Mobile"), None, false);
    push_row(
        Line::from(vec![
            Span::styled(" C", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" open web chat", Style::default().fg(theme::TEXT_DIM())),
        ]),
        None,
        false,
    );
    push_row(
        Line::from(Span::styled(
            " 24h link, scan QR",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        None,
        false,
    );

    RoomListRows {
        lines,
        hit_slots,
        selected_row_index,
    }
}

pub(crate) fn room_list_hit_test(
    rooms_area: Rect,
    view: &ChatRoomListView<'_>,
    x: u16,
    y: u16,
) -> Option<RoomSlot> {
    if view.chat_rooms.is_empty() {
        return None;
    }

    let inner = Rect {
        x: rooms_area.x + 2,
        y: rooms_area.y + 1,
        width: rooms_area.width.saturating_sub(4),
        height: rooms_area.height.saturating_sub(1),
    };
    let hint_rows = build_rail_nav_hint_lines().len() as u16;
    let footer_reserve = hint_rows + 2;
    let list_area = if inner.height > footer_reserve + 2 {
        Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_reserve)]).split(inner)[0]
    } else {
        inner
    };
    if x < list_area.x || x >= list_area.right() || y < list_area.y || y >= list_area.bottom() {
        return None;
    }

    let room_rows = build_cozy_room_rail_rows(view, rooms_area.width.saturating_sub(2));
    let scroll = rooms_scroll_for_selection(
        room_rows.lines.len(),
        list_area.height as usize,
        room_rows.selected_row_index,
    );
    let row_index = scroll + (y - list_area.y) as usize;
    room_rows.hit_slots.get(row_index).copied().flatten()
}

pub(crate) fn room_list_panel_contains(
    rooms_area: Rect,
    view: &ChatRoomListView<'_>,
    x: u16,
    y: u16,
) -> bool {
    if view.chat_rooms.is_empty() {
        return false;
    }

    x >= rooms_area.x && x < rooms_area.right() && y >= rooms_area.y && y < rooms_area.bottom()
}

/// Cozy room rail for the merged shell. Anchored by a single thin vertical
/// separator on its RIGHT edge; the rest is borderless. Quiet section labels,
/// left-bar accent on the active row, dim trailing unread numbers.
pub fn draw_room_list_rail(frame: &mut Frame, area: Rect, view: &ChatRenderInput<'_>) {
    // Right-edge vertical separator anchors the rail visually.
    let sep_x = area.right().saturating_sub(1);
    crate::app::common::sidebar::paint_vertical_separator(frame, sep_x, area.y, area.height);

    let room_list_view = room_list_view_from_render_input(view);
    let room_rows = build_cozy_room_rail_rows(&room_list_view, area.width.saturating_sub(2));

    // Content lives inside: 2 cols left padding, 2 cols right (separator + 1).
    // Bottom slice is reserved for the pinned nav-hint footer.
    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(1),
    };

    let hint_lines = build_rail_nav_hint_lines();
    let hint_rows = hint_lines.len() as u16;
    // Reserve: top border + hint rows + bottom border. If the rail is too
    // short, skip hints.
    let footer_reserve = hint_rows + 2;
    let (list_area, hint_area) = if inner.height > footer_reserve + 2 {
        let split = Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_reserve)])
            .split(inner);
        (split[0], Some(split[1]))
    } else {
        (inner, None)
    };

    let scroll = rooms_scroll_for_selection(
        room_rows.lines.len(),
        list_area.height as usize,
        room_rows.selected_row_index,
    );
    let visible_height = list_area.height as usize;

    // Repaint any active-row accent bar in the list area's leftmost gutter.
    let buf = frame.buffer_mut();
    for (i, line) in room_rows
        .lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .enumerate()
    {
        let y = list_area.y + i as u16;
        if y >= list_area.bottom() {
            break;
        }
        if line
            .spans
            .first()
            .is_some_and(|s| s.content.as_ref() == "▌")
            && let Some(cell) = buf.cell_mut((area.x + 1, y))
        {
            cell.set_symbol("▌").set_fg(theme::AMBER());
        }
    }

    // Strip the sentinel marker span before rendering text.
    let display_lines: Vec<Line<'static>> = room_rows
        .lines
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .map(|line| {
            if line
                .spans
                .first()
                .is_some_and(|s| s.content.as_ref() == "▌")
            {
                Line::from(line.spans.into_iter().skip(1).collect::<Vec<_>>())
            } else {
                line
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(display_lines), list_area);

    if let Some(hint_area) = hint_area {
        let buf = frame.buffer_mut();
        for dx in 0..hint_area.width {
            if let Some(cell) = buf.cell_mut((hint_area.x + dx, hint_area.y)) {
                cell.set_symbol("─").set_fg(theme::BORDER_DIM());
            }
            if let Some(cell) =
                buf.cell_mut((hint_area.x + dx, hint_area.bottom().saturating_sub(1)))
            {
                cell.set_symbol("─").set_fg(theme::BORDER_DIM());
            }
        }

        // Render the hint lines between the footer separators.
        let hint_render_area = Rect {
            x: hint_area.x,
            y: hint_area.y + 1,
            width: hint_area.width,
            height: hint_area.height.saturating_sub(2),
        };
        frame.render_widget(Paragraph::new(hint_lines), hint_render_area);
    }
}

/// Builds the cozy rail rows. Active rows are tagged with a sentinel `▌` span
/// at index 0 so the renderer can paint a one-column accent bar in the gutter.
/// That sentinel is stripped before final paint.
fn build_cozy_room_rail_rows(view: &ChatRoomListView<'_>, width: u16) -> RoomListRows {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut hit_slots: Vec<Option<RoomSlot>> = Vec::new();
    let mut selected_row_index = None;
    let inner_width = width.saturating_sub(3) as usize; // 2 left gutter + 1 right margin
    let order = visual_order_for_rooms(
        view.chat_rooms,
        view.current_user_id,
        view.usernames,
        view.feeds_available,
        view.favorite_room_ids,
    );
    let jump_targets: HashMap<RoomSlot, u8> = order
        .iter()
        .copied()
        .zip(ROOM_JUMP_KEYS.iter().copied())
        .collect();

    let blank = || Line::raw("");
    let section_label = |s: &str| -> Line<'static> {
        Line::from(Span::styled(
            s.to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ))
    };
    let item_row =
        |label: String, unread: i64, active: bool, jump_key: Option<u8>| -> Line<'static> {
            let key_prefix = if view.room_jump_active {
                jump_key
                    .map(|key| format!("{} ", key as char))
                    .unwrap_or_else(|| "  ".to_string())
            } else {
                String::new()
            };
            let key_width = UnicodeWidthStr::width(key_prefix.as_str());
            let label_max = inner_width.saturating_sub(key_width + 4);
            let display_label =
                if UnicodeWidthStr::width(label.as_str()) > label_max && label_max > 1 {
                    let mut s = String::new();
                    let mut w = 0usize;
                    for c in label.chars() {
                        let cw = UnicodeWidthStr::width(c.to_string().as_str());
                        if w + cw > label_max.saturating_sub(1) {
                            break;
                        }
                        s.push(c);
                        w += cw;
                    }
                    s.push('…');
                    s
                } else {
                    label
                };
            let display = format!("{key_prefix}{display_label}");
            let used = UnicodeWidthStr::width(display.as_str());
            let unread_str = if unread > 0 {
                format!("{unread}")
            } else {
                String::new()
            };
            let pad =
                inner_width.saturating_sub(used + UnicodeWidthStr::width(unread_str.as_str()));
            let mut spans = Vec::new();
            if active {
                spans.push(Span::raw("▌"));
            }
            let name_color = if active {
                theme::AMBER()
            } else if unread > 0 {
                theme::TEXT()
            } else {
                theme::TEXT_DIM()
            };
            let name_modifier = if active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            spans.push(Span::styled(
                display,
                Style::default().fg(name_color).add_modifier(name_modifier),
            ));
            if !unread_str.is_empty() {
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(Span::styled(
                    unread_str,
                    Style::default().fg(theme::AMBER_DIM()),
                ));
            }
            Line::from(spans)
        };

    let mut push_row = |line: Line<'static>, slot: Option<RoomSlot>, selected: bool| {
        lines.push(line);
        hit_slots.push(slot);
        if selected {
            selected_row_index = Some(lines.len() - 1);
        }
    };
    let push_slot =
        |slot: RoomSlot, push_row: &mut dyn FnMut(Line<'static>, Option<RoomSlot>, bool)| {
            let active = cozy_slot_selected(view, slot);
            let (label, unread) = room_slot_label_and_unread(view, slot);
            push_row(
                item_row(label, unread, active, jump_targets.get(&slot).copied()),
                Some(slot),
                active,
            );
        };

    let favorite_slots: Vec<RoomSlot> = order
        .iter()
        .copied()
        .take_while(|slot| match slot {
            RoomSlot::Room(room_id) => view.favorite_room_ids.contains(room_id),
            _ => false,
        })
        .collect();
    let favorite_ids: std::collections::HashSet<Uuid> = favorite_slots
        .iter()
        .filter_map(|slot| match slot {
            RoomSlot::Room(room_id) => Some(*room_id),
            _ => None,
        })
        .collect();
    if !favorite_slots.is_empty() {
        push_row(section_label("favorites"), None, false);
        for slot in favorite_slots {
            push_slot(slot, &mut push_row);
        }
        push_row(blank(), None, false);
    }

    let core_order = ["general", "announcements", "suggestions", "bugs"];
    push_row(section_label("core"), None, false);
    for slug in &core_order {
        if let Some((room, _)) = view.chat_rooms.iter().find(|(r, _)| {
            is_chat_list_room(r)
                && r.permanent
                && r.slug.as_deref() == Some(slug)
                && !favorite_ids.contains(&r.id)
        }) {
            push_slot(RoomSlot::Room(room.id), &mut push_row);
        }
    }
    push_slot(RoomSlot::Notifications, &mut push_row);

    let channels: Vec<&(ChatRoom, Vec<ChatMessage>)> = view
        .chat_rooms
        .iter()
        .filter(|(r, _)| {
            is_chat_list_room(r)
                && r.kind != "dm"
                && !core_order.contains(&r.slug.as_deref().unwrap_or(""))
                && !favorite_ids.contains(&r.id)
        })
        .collect();
    if !channels.is_empty() {
        push_row(blank(), None, false);
        push_row(section_label("channels"), None, false);
        for (room, _) in channels {
            push_slot(RoomSlot::Room(room.id), &mut push_row);
        }
    }

    push_row(blank(), None, false);
    push_row(section_label("feeds"), None, false);
    for slot in [
        RoomSlot::News,
        RoomSlot::Feeds,
        RoomSlot::Showcase,
        RoomSlot::Work,
    ] {
        if slot == RoomSlot::Feeds && !view.feeds_available {
            continue;
        }
        push_slot(slot, &mut push_row);
    }

    let mut dms: Vec<&(ChatRoom, Vec<ChatMessage>)> = view
        .chat_rooms
        .iter()
        .filter(|(r, _)| is_chat_list_room(r) && r.kind == "dm" && !favorite_ids.contains(&r.id))
        .collect();
    dms.sort_by(|(a, _), (b, _)| {
        let a = dm_display_label(a, view.usernames, view.current_user_id);
        let b = dm_display_label(b, view.usernames, view.current_user_id);
        a.cmp(&b)
    });
    if !dms.is_empty() {
        push_row(blank(), None, false);
        push_row(section_label("dms"), None, false);
        for (room, _) in dms {
            push_slot(RoomSlot::Room(room.id), &mut push_row);
        }
    }

    push_row(blank(), None, false);
    push_row(section_label("voice"), None, false);
    push_row(
        Line::from(Span::styled(
            VOICE_DISCORD_INVITE,
            Style::default().fg(theme::TEXT_DIM()),
        )),
        None,
        false,
    );

    push_row(blank(), None, false);
    push_slot(RoomSlot::Discover, &mut push_row);

    RoomListRows {
        lines,
        hit_slots,
        selected_row_index,
    }
}

fn room_slot_label_and_unread(view: &ChatRoomListView<'_>, slot: RoomSlot) -> (String, i64) {
    match slot {
        RoomSlot::Room(room_id) => {
            let Some((room, _)) = view.chat_rooms.iter().find(|(room, _)| room.id == room_id)
            else {
                return ("room".to_string(), 0);
            };
            let label = room_display_label(room, view.usernames, view.current_user_id);
            let unread = view.unread_counts.get(&room.id).copied().unwrap_or(0);
            (label, unread)
        }
        RoomSlot::Feeds => ("rss".to_string(), view.feeds_unread_count),
        RoomSlot::News => ("news".to_string(), view.news_unread_count),
        RoomSlot::Notifications => ("mentions".to_string(), view.notifications_unread_count),
        RoomSlot::Discover => ("+ browse rooms".to_string(), 0),
        RoomSlot::Showcase => ("showcase".to_string(), view.showcase_unread_count),
        RoomSlot::Work => ("work".to_string(), view.work_unread_count),
    }
}

fn room_display_label(
    room: &ChatRoom,
    usernames: &HashMap<Uuid, String>,
    current_user_id: Uuid,
) -> String {
    if room.kind == "dm" {
        return dm_display_label(room, usernames, current_user_id);
    }
    let base_label = room
        .slug
        .as_deref()
        .map(room_slug_display_label)
        .map(str::to_string)
        .unwrap_or_else(|| room.kind.clone());
    if room.visibility == "private" {
        format!("🔒 {}", base_label)
    } else {
        base_label
    }
}

fn room_slug_display_label(slug: &str) -> &str {
    match slug {
        "general" => "lounge",
        _ => slug,
    }
}

/// Nav-hint footer. Caller pins this to the bottom of the rail so the
/// hints stay anchored regardless of how long the room list is.
fn build_rail_nav_hint_lines() -> Vec<Line<'static>> {
    let key = |k: &str| -> Span<'static> {
        Span::styled(
            k.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        )
    };
    let hint = |s: &str| -> Span<'static> {
        Span::styled(s.to_string(), Style::default().fg(theme::TEXT_FAINT()))
    };
    vec![
        Line::from(vec![key("h l space"), hint(" jump room")]),
        Line::from(vec![key("f"), hint("         favorite")]),
        Line::from(vec![key("ctrl+/"), hint("    find room")]),
    ]
}

fn cozy_slot_selected(view: &ChatRoomListView<'_>, slot: RoomSlot) -> bool {
    is_selected_slot(
        slot,
        SelectedRoomSlotState {
            selected_room_id: view.selected_room_id,
            feeds_selected: view.feeds_selected,
            news_selected: view.news_selected,
            notifications_selected: view.notifications_selected,
            discover_selected: view.discover_selected,
            showcase_selected: view.showcase_selected,
            work_selected: view.work_selected,
        },
    )
}

fn dm_display_label(
    room: &ChatRoom,
    usernames: &HashMap<Uuid, String>,
    current_user_id: Uuid,
) -> String {
    let other = if room.dm_user_a == Some(current_user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    };
    let name = other
        .and_then(|id| usernames.get(&id).cloned())
        .unwrap_or_else(|| "?".to_string());
    format!("@ {}", name)
}

/// Center pane for the merged Home/Chat shell. The room rail is rendered by
/// the outer shell, so this draws only the selected room/feed content plus the
/// relevant composer or hint row.
pub fn draw_chat_center(frame: &mut Frame, area: Rect, view: ChatRenderInput<'_>) {
    if view.chat_rooms.is_empty() {
        let empty = Paragraph::new("No chat rooms yet.")
            .style(Style::default().fg(theme::TEXT_DIM()))
            .centered();
        frame.render_widget(empty, area);
        return;
    }

    let selection_mode = chat_selection_mode(&view, area);
    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(selection_mode.composer_height()),
    ])
    .split(area);
    let messages_area = layout[0];
    let composer_area = layout[1];

    draw_selected_content(frame, messages_area, composer_area, view);
}

fn draw_selected_content(
    frame: &mut Frame,
    messages_area: Rect,
    composer_area: Rect,
    view: ChatRenderInput<'_>,
) {
    let selected_room_id = view.selected_room_id;
    let current_user_id = view.current_user_id;
    let feeds_selected = view.feeds_selected;
    let news_selected = view.news_selected;

    if feeds_selected {
        super::feeds::ui::draw_feed_list(frame, messages_area, &view.feeds_view);
    } else if view.notifications_selected {
        super::notifications::ui::draw_notification_list(
            frame,
            messages_area,
            &view.notifications_view,
        );
    } else if view.discover_selected {
        super::discover::ui::draw_discover_list(frame, messages_area, &view.discover_view);
    } else if view.showcase_selected {
        super::showcase::ui::draw_showcase_list(frame, messages_area, &view.showcase_view);
    } else if view.work_selected {
        super::work::ui::draw_work_list(frame, messages_area, &view.work_view);
    } else if news_selected {
        super::news::ui::draw_article_list(frame, messages_area, &view.news_view);
    } else {
        let selected_room = selected_room_id
            .and_then(|id| view.chat_rooms.iter().find(|(room, _)| room.id == id))
            .filter(|(room, _)| is_chat_list_room(room))
            .or_else(|| {
                view.chat_rooms
                    .iter()
                    .find(|(room, _)| is_chat_list_room(room))
            });

        let message_lines: Vec<Line> = if let Some((_room, messages)) = selected_room {
            let height = messages_area.height.max(1) as usize;
            let width = messages_area.width.max(1) as usize;

            ensure_chat_rows_cache(
                view.rows_cache,
                messages.iter().collect(),
                width,
                ChatRowsContext {
                    current_user_id,
                    usernames: view.usernames,
                    countries: view.countries,
                    bonsai_glyphs: view.bonsai_glyphs,
                    message_reactions: view.message_reactions,
                    inline_images: view.inline_images,
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
            lines
        } else {
            vec![Line::from(Span::styled(
                "Select a room.",
                Style::default().fg(theme::TEXT_DIM()),
            ))]
        };

        let messages_paragraph = Paragraph::new(message_lines);
        frame.render_widget(messages_paragraph, messages_area);
        if let Some(overlay) = view.overlay {
            draw_overlay(frame, messages_area, overlay);
        }
    }

    if feeds_selected {
        if view.feeds_processing {
            let hint_block = Block::default()
                .title(" Processing URL... ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::AMBER()));
            let hint_text = Paragraph::new(Line::from(Span::styled(
                " Sharing feed entry to news · Esc cancel",
                Style::default().fg(theme::TEXT_DIM()),
            )))
            .block(hint_block);
            frame.render_widget(hint_text, composer_area);
        } else {
            let hint_block = Block::default()
                .title(" Feeds ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER()));
            let hint_text = Paragraph::new(Line::from(Span::styled(
                " j/k navigate · s share · Enter copy link · d dismiss · r refresh",
                Style::default().fg(theme::TEXT_DIM()),
            )))
            .block(hint_block);
            frame.render_widget(hint_text, composer_area);
        }
    } else if view.notifications_selected {
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
    } else if view.showcase_selected {
        if let Some(showcase_state) = view.showcase_state {
            super::showcase::ui::draw_showcase_composer(
                frame,
                composer_area,
                &super::showcase::ui::ShowcaseComposerView {
                    state: showcase_state,
                },
            );
        }
    } else if view.work_selected {
        if let Some(work_state) = view.work_state {
            super::work::ui::draw_work_composer(
                frame,
                composer_area,
                &super::work::ui::WorkComposerView { state: work_state },
            );
        }
    } else if view.discover_selected {
        let hint_block = Block::default()
            .title(" Discover ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER()));
        let hint_text = Paragraph::new(Line::from(Span::styled(
            " j/k navigate · Enter join room",
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
                " j/k navigate · Enter copy link · i paste URL · / filter mine",
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
                composer: view.composer,
                composing: view.composing,
                selected_message: view.selected_message_id.is_some(),
                selected_news_message: view.selected_news_message,
                reaction_picker_active: view.reaction_picker_active,
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
    use chrono::Utc;
    use late_core::models::chat_room::ChatRoom;
    use std::{collections::HashMap, sync::OnceLock};

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
    fn is_bot_author_matches_all_ghost_users() {
        assert!(is_bot_author("bot"));
        assert!(is_bot_author("graybeard"));
        assert!(is_bot_author("dealer"));
        assert!(is_bot_author(" Dealer "));
        assert!(!is_bot_author("mat"));
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

    #[test]
    fn chat_rows_fingerprint_changes_when_theme_changes() {
        let room_id = Uuid::from_u128(1);
        let user_id = Uuid::from_u128(2);
        let message = ChatMessage {
            id: Uuid::from_u128(3),
            created: Utc::now(),
            updated: Utc::now(),
            pinned: false,
            reply_to_message_id: None,
            room_id,
            user_id,
            body: "hello".to_string(),
        };
        let usernames = HashMap::from([(user_id, "alice".to_string())]);
        let countries = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let message_reactions = HashMap::new();
        let inline_images = HashMap::new();

        let messages = vec![&message];
        let ctx = ChatRowsContext {
            current_user_id: user_id,
            usernames: &usernames,
            countries: &countries,
            bonsai_glyphs: &bonsai_glyphs,
            message_reactions: &message_reactions,
            inline_images: &inline_images,
        };

        theme::set_current_by_id("late");
        let late_fingerprint = chat_rows_fingerprint(&messages, &ctx, 80);
        theme::set_current_by_id("contrast");
        let contrast_fingerprint = chat_rows_fingerprint(&messages, &ctx, 80);

        assert_ne!(late_fingerprint, contrast_fingerprint);
    }

    fn composer_view<'a>(textarea: &'a TextArea<'static>) -> ComposerBlockView<'a> {
        ComposerBlockView {
            composer: textarea,
            composing: true,
            selected_message: false,
            selected_news_message: false,
            reaction_picker_active: false,
            reply_author: None,
            is_editing: false,
            mention_active: false,
            mention_matches: &[],
            mention_selected: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn chat_view<'a>(
        rows_cache: &'a mut ChatRowsCache,
        rooms: &'a [(ChatRoom, Vec<ChatMessage>)],
        selected_room_id: Option<Uuid>,
        usernames: &'a HashMap<Uuid, String>,
        countries: &'a HashMap<Uuid, String>,
        message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
        unread_counts: &'a HashMap<Uuid, i64>,
        bonsai_glyphs: &'a HashMap<Uuid, String>,
        composer: &'a TextArea<'static>,
        news_composer: &'a TextArea<'static>,
    ) -> ChatRenderInput<'a> {
        static INLINE_IMAGES: OnceLock<HashMap<Uuid, Vec<Line<'static>>>> = OnceLock::new();

        ChatRenderInput {
            feeds_selected: false,
            feeds_processing: false,
            feeds_unread_count: 0,
            feeds_view: crate::app::chat::feeds::ui::FeedListView {
                entries: &[],
                selected_index: 0,
                has_feeds: false,
                marker_read_at: None,
            },
            news_selected: false,
            news_unread_count: 0,
            news_view: crate::app::chat::news::ui::ArticleListView {
                articles: &[],
                selected_index: 0,
                marker_read_at: None,
                mine_only: false,
            },
            discover_selected: false,
            discover_view: crate::app::chat::discover::ui::DiscoverListView {
                items: &[],
                selected_index: 0,
                loading: false,
            },
            rows_cache,
            chat_rooms: rooms,
            overlay: None,
            usernames,
            countries,
            message_reactions,
            inline_images: INLINE_IMAGES.get_or_init(HashMap::new),
            unread_counts,
            favorite_room_ids: &[],
            selected_room_id,
            room_jump_active: false,
            selected_message_id: None,
            selected_news_message: false,
            reaction_picker_active: false,
            highlighted_message_id: None,
            composer,
            composing: false,
            current_user_id: Uuid::nil(),
            cursor_visible: false,
            mention_matches: &[],
            mention_selected: 0,
            mention_active: false,
            reply_author: None,
            is_editing: false,
            bonsai_glyphs,
            news_composer,
            news_composing: false,
            news_processing: false,
            notifications_selected: false,
            notifications_unread_count: 0,
            notifications_view: crate::app::chat::notifications::ui::NotificationListView {
                items: &[],
                selected_index: 0,
                marker_read_at: None,
            },
            showcase_selected: false,
            showcase_unread_count: 0,
            showcase_view: crate::app::chat::showcase::ui::ShowcaseListView {
                items: &[],
                selected_index: 0,
                current_user_id: Uuid::nil(),
                is_admin: false,
                marker_read_at: None,
                mine_only: false,
            },
            showcase_state: None,
            showcase_composing: false,
            work_selected: false,
            work_unread_count: 0,
            work_view: crate::app::chat::work::ui::WorkListView {
                items: &[],
                selected_index: 0,
                current_user_id: Uuid::nil(),
                is_admin: false,
                marker_read_at: None,
                profile_base_url: "http://localhost:3000",
                mine_only: false,
            },
            work_state: None,
            work_composing: false,
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
        let full = " Compose (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ";
        let long = " (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ";
        let short = " (⏎ send, Alt+S stay, Alt+⏎/Ctrl+J newline, Esc cancel) ";
        let compact = " Compose (Enter send, Esc cancel) ";
        let minimal = " (⏎ send, Esc cancel) ";
        let cancel = " (Esc cancel) ";
        let esc = " Esc ";
        let need = |title: &str| (UnicodeWidthStr::width(title) + 2) as u16;
        let titled = |title: &str| format!("──{title}");

        assert_eq!(composer_title(&view, need(full)), titled(full));
        assert_eq!(composer_title(&view, need(full) - 1), titled(long));

        assert_eq!(composer_title(&view, need(long)), titled(long));
        assert_eq!(composer_title(&view, need(long) - 1), titled(short));

        assert_eq!(composer_title(&view, need(short)), titled(short));
        assert_eq!(composer_title(&view, need(short) - 1), titled(compact));

        assert_eq!(composer_title(&view, need(compact)), titled(compact));
        assert_eq!(composer_title(&view, need(compact) - 1), titled(minimal));

        assert_eq!(composer_title(&view, need(minimal)), titled(minimal));
        assert_eq!(composer_title(&view, need(minimal) - 1), titled(cancel));

        assert_eq!(composer_title(&view, need(cancel)), titled(cancel));
        assert_eq!(composer_title(&view, need(cancel) - 1), titled(esc));

        assert_eq!(composer_title(&view, need(esc)), titled(esc));
        assert_eq!(composer_title(&view, need(esc) - 1), "");
    }

    #[test]
    fn visible_rows_paint_background_for_selected_highlighted_message() {
        let message_id = Uuid::now_v7();
        let mut cache = ChatRowsCache {
            all_rows: vec![
                Line::from(Span::raw("alice")),
                Line::from(Span::raw("hello")),
            ],
            ..Default::default()
        };
        cache.selected_ranges.insert(message_id, (1, 2));
        cache.highlighted_ranges.insert(message_id, (0, 2));

        let rows = visible_chat_rows(&cache, Some(message_id), Some(message_id), 4);
        assert!(
            rows.iter()
                .flat_map(|row| row.spans.iter())
                .any(|span| span.style.bg == Some(theme::BG_SELECTION())),
            "expected selected highlighted message to receive background"
        );
    }

    #[test]
    fn composer_title_reply_state_degrades_through_name_only_and_label() {
        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.reply_author = Some("alice");
        assert_eq!(
            composer_title(&view, 100),
            "── Reply to @alice (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) "
        );
        // Far too narrow for even the shortest reply form → drops to " Reply ".
        // " Reply " = 7 cols → needs block_w ≥ 9.
        assert_eq!(composer_title(&view, 10), "── Reply ");
        assert_eq!(composer_title(&view, 9), "── Reply ");
        // " Esc " = 5 cols → needs block_w ≥ 7.
        assert_eq!(composer_title(&view, 8), "── Esc ");
        assert_eq!(composer_title(&view, 7), "── Esc ");
        assert_eq!(composer_title(&view, 6), "");
    }

    #[test]
    fn composer_title_when_not_composing_shows_press_i_prompt() {
        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.composing = false;
        assert_eq!(composer_title(&view, 30), "── Compose (press i) ");
        assert_eq!(composer_title(&view, 13), "── (press i) ");
        // " i " = 3 cols → needs block_w ≥ 5.
        assert_eq!(composer_title(&view, 5), "── i ");
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
    fn reaction_picker_placeholder_uses_one_line() {
        let lines = reaction_picker_placeholder_lines(Style::default());
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn draw_composer_block_renders_reaction_picker_in_placeholder() {
        use ratatui::{Terminal, backend::TestBackend};

        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.reaction_picker_active = true;
        view.composing = false;
        view.selected_message = true;

        let backend = TestBackend::new(96, 3);
        let mut terminal = Terminal::new(backend).expect("term");

        terminal
            .draw(|f| draw_composer_block(f, Rect::new(0, 0, 96, 3), &view))
            .unwrap();

        let buf = terminal.backend().buffer();
        let row_1: String = (0..96).map(|x| buf[(x, 1)].symbol().to_string()).collect();
        assert!(
            row_1.contains("1 👍"),
            "reaction choices missing from {row_1:?}",
        );
        assert!(
            row_1.contains("8 🤔"),
            "extended reaction choices missing from {row_1:?}",
        );
        assert!(
            row_1.contains("f list"),
            "reaction owner hint missing from {row_1:?}",
        );
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
        let expected =
            "Type a message · j/k select · Ctrl+/ jump room · or just ask @bot about anything";
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
    fn empty_composer_placeholder_contextualizes_selected_news_message() {
        use ratatui::{Terminal, backend::TestBackend};

        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.composing = false;
        view.selected_message = true;
        view.selected_news_message = true;

        let placeholder = empty_composer_placeholder(&view);
        let expected =
            "f react · r reply · e edit · d delete · p profile · c copy · Enter view/copy link";
        let width = expected.chars().count() as u16;
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("term");

        terminal
            .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
            .unwrap();

        let buf = terminal.backend().buffer();
        let rendered: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();
        assert_eq!(rendered, expected);
    }

    #[test]
    fn rooms_scroll_keeps_selection_near_center() {
        // height=9 -> anchor row = 4, leaving context above and below.
        assert_eq!(rooms_scroll_for_selection(20, 9, Some(4)), 0);
        assert_eq!(rooms_scroll_for_selection(20, 9, Some(7)), 3);
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

    #[test]
    fn room_list_rows_display_general_as_lounge() {
        let general = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "general".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("general".to_string()),
            permanent: true,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rooms = vec![(general.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let composer = TextArea::default();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            Some(general.id),
            &usernames,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &composer,
            &news_composer,
        );

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));
        let rendered = room_rows
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert!(
            rendered.iter().any(|line| line.contains("lounge")),
            "expected room list to show lounge: {rendered:?}"
        );
        assert!(
            !rendered.iter().any(|line| line.contains("general")),
            "general should stay an internal slug, not the nav label: {rendered:?}"
        );
    }

    #[test]
    fn room_list_rows_place_work_after_showcases() {
        let rooms = Vec::new();
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let composer = TextArea::default();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            None,
            &usernames,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &composer,
            &news_composer,
        );

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));
        let hit_slots: Vec<_> = room_rows.hit_slots.into_iter().flatten().collect();

        assert_eq!(
            hit_slots,
            vec![
                RoomSlot::News,
                RoomSlot::Showcase,
                RoomSlot::Work,
                RoomSlot::Notifications,
                RoomSlot::Discover,
            ]
        );
    }

    #[test]
    fn room_list_rows_skip_game_rooms() {
        let general = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "general".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("general".to_string()),
            permanent: true,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let game = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "game".to_string(),
            visibility: "public".to_string(),
            auto_join: false,
            slug: Some("bj-abc123".to_string()),
            permanent: false,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rooms = vec![(general.clone(), Vec::new()), (game.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let composer = TextArea::default();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            Some(general.id),
            &usernames,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &composer,
            &news_composer,
        );

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));

        assert!(!room_rows.hit_slots.contains(&Some(RoomSlot::Room(game.id))));
    }

    #[test]
    fn room_list_hit_test_maps_public_room_row_to_room_slot() {
        let general = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "general".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("general".to_string()),
            permanent: true,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rust = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "topic".to_string(),
            visibility: "public".to_string(),
            auto_join: false,
            slug: Some("rust".to_string()),
            permanent: false,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rooms = vec![(general.clone(), Vec::new()), (rust.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let composer = TextArea::default();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            Some(general.id),
            &usernames,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &composer,
            &news_composer,
        );

        let area = Rect::new(1, 1, 74, 30);
        let rooms_area = room_list_area(area, chat_selection_mode(&view, area));
        let room_list_view = room_list_view_from_render_input(&view);
        let inner = Rect {
            x: rooms_area.x + 2,
            y: rooms_area.y + 1,
            width: rooms_area.width.saturating_sub(4),
            height: rooms_area.height.saturating_sub(1),
        };
        let hint_rows = build_rail_nav_hint_lines().len() as u16;
        let footer_reserve = hint_rows + 2;
        let list_area = if inner.height > footer_reserve + 2 {
            Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_reserve)]).split(inner)
                [0]
        } else {
            inner
        };
        let room_rows =
            build_cozy_room_rail_rows(&room_list_view, rooms_area.width.saturating_sub(2));
        let rust_row = room_rows
            .hit_slots
            .iter()
            .position(|slot| *slot == Some(RoomSlot::Room(rust.id)))
            .expect("rust room row");

        assert_eq!(
            room_list_hit_test(
                rooms_area,
                &room_list_view,
                list_area.x,
                list_area.y + rust_row as u16
            ),
            Some(RoomSlot::Room(rust.id))
        );
        assert_eq!(
            room_list_hit_test(rooms_area, &room_list_view, list_area.x, list_area.y),
            None
        );
        assert!(room_list_panel_contains(
            rooms_area,
            &room_list_view,
            rooms_area.x,
            rooms_area.y
        ));
        assert!(!room_list_panel_contains(
            rooms_area,
            &room_list_view,
            rooms_area.right(),
            rooms_area.y
        ));
    }
}
