use chrono::{DateTime, Utc};
use late_core::models::chat_message_reaction::ChatMessageReactionSummary;
use late_core::models::chat_poll::{ActiveChatPoll, ChatPollOptionSummary};
use late_core::models::{chat_message::ChatMessage, chat_room::ChatRoom};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_textarea::TextArea;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use crate::app::common::{
    composer::composer_line_count,
    overlay::{Overlay, draw_overlay},
    theme,
};
use crate::app::files::{
    inline_image::InlineImagePreview,
    terminal_image::{
        TerminalImageData, TerminalImageFrame, TerminalImagePlacement, TerminalImageProtocol,
    },
};
use crate::app::hub::shop::svc::ActiveChatRoomEffect;
use crate::usernames::UsernameLookup;

use super::state::{
    MentionMatch, ROOM_JUMP_KEYS, RoomSection, RoomSlot, RoomVisualOrderInput,
    SelectedRoomSlotState, compare_dm_rooms_for_nav, is_chat_list_room, is_selected_slot,
    visual_order_for_rooms,
};
use super::ui_text::{reaction_label, wrap_chat_entry_to_lines};

const REACTION_PICKER_KEYS: [i16; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
const CHAT_COMPOSER_GAP_HEIGHT: u16 = 1;
const AUTHOR_BADGE_SEPARATOR: &str = " ";
const FRIEND_BADGE: &str = "★";
const AFK_BADGE: &str = "🌙";

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
    pub image_modal: Option<ImageModalView<'a>>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub usernames: &'a UsernameLookup<'a>,
    pub countries: &'a HashMap<Uuid, String>,
    pub friend_user_ids: &'a HashSet<Uuid>,
    pub afk_user_ids: &'a HashSet<Uuid>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub current_user_id: Uuid,
    pub show_flag_fallback: bool,
    pub selected_message_id: Option<Uuid>,
    pub selected_image_message: bool,
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
    pub chat_badges: &'a HashMap<Uuid, String>,
    pub profile_award_badges: &'a HashMap<Uuid, String>,
    pub bot_username_color_active: bool,
    pub active_room_effects: &'a [ActiveChatRoomEffect],
    pub active_poll: Option<&'a ActiveChatPoll>,
    pub inline_images: &'a HashMap<Uuid, InlineImagePreview>,
    pub keep_composer_focused: bool,
    /// Cell that, when present, receives the composer block rect so mouse
    /// hit-testing in `app::input` can detect double-clicks into the bar.
    pub composer_rect_slot: Option<&'a std::cell::Cell<Option<Rect>>>,
    /// Cell that, when present, receives this frame's chat-scroll hit
    /// layout so `app::input` can map clicks in the message area to a
    /// message id, header segment, or inline-image row.
    pub(crate) chat_hit_slot: Option<&'a std::cell::Cell<Option<ChatHitLayout>>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ImageModalView<'a> {
    pub message_id: Uuid,
    pub url: &'a str,
    pub preview: Option<&'a InlineImagePreview>,
    pub terminal_image: Option<&'a TerminalImageData>,
    pub terminal_image_protocol: Option<TerminalImageProtocol>,
}

/// Shared composer block rendering for both the dashboard card and the chat
/// page. New composer states (editing, replying, …) wire here once.
pub(super) struct ComposerBlockView<'a> {
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub selected_message: bool,
    pub selected_image_message: bool,
    pub selected_news_message: bool,
    pub reaction_picker_active: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub mention_active: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
    /// When true, Enter sends without closing the composer and Alt+S is a
    /// no-op. Drives the title-hint tier swap.
    pub keep_composer_focused: bool,
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
        if view.keep_composer_focused {
            let long = format!(
                " Reply to @{author} (Enter send & stay, Alt+Enter/Ctrl+J newline, Esc cancel) "
            );
            let mid =
                format!(" Reply to @{author} (⏎ send & stay, Alt+⏎/Ctrl+J newline, Esc cancel) ");
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
        if view.keep_composer_focused {
            return pick_title_that_fits(
                block_width,
                &[
                    " Edit message (Enter save & stay, Alt+Enter/Ctrl+J newline, Esc cancel) ",
                    " Edit message (⏎ save & stay, Alt+⏎/Ctrl+J newline, Esc cancel) ",
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

    if view.keep_composer_focused {
        return pick_title_that_fits(
            block_width,
            &[
                " Compose (Enter send & stay, Alt+Enter/Ctrl+J newline, Esc cancel) ",
                " (Enter send & stay, Alt+Enter/Ctrl+J newline, Esc cancel) ",
                " (⏎ send & stay, Alt+⏎/Ctrl+J newline, Esc cancel) ",
                " Compose (Enter send, Esc cancel) ",
                " (⏎ send, Esc cancel) ",
                " (Esc cancel) ",
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

fn reaction_picker_choice_width(key: i16) -> usize {
    1 + 1 + reaction_label(key).width()
}

fn push_reaction_picker_choice(reaction_spans: &mut Vec<Span<'static>>, dim: Style, key: i16) {
    reaction_spans.push(Span::styled(
        key.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    reaction_spans.push(Span::styled(" ", dim));
    reaction_spans.push(Span::styled(reaction_label(key), dim));
}

fn reaction_picker_placeholder_lines(dim: Style, width: usize) -> Vec<Line<'static>> {
    let available_width = width.max(1);
    let mut lines = Vec::new();
    let mut current_spans = Vec::new();
    let mut current_width = 0usize;

    for key in REACTION_PICKER_KEYS {
        let separator_width = usize::from(!current_spans.is_empty()) * 2;
        let choice_width = reaction_picker_choice_width(key);
        if !current_spans.is_empty()
            && current_width + separator_width + choice_width > available_width
        {
            lines.push(Line::from(std::mem::take(&mut current_spans)));
            current_width = 0;
        }
        if !current_spans.is_empty() {
            current_spans.push(Span::styled("  ", dim));
            current_width += 2;
        }
        push_reaction_picker_choice(&mut current_spans, dim, key);
        current_width += choice_width;
    }

    let owner_hint_width = 8;
    if !current_spans.is_empty() && current_width + owner_hint_width > available_width {
        lines.push(Line::from(std::mem::take(&mut current_spans)));
    } else if !current_spans.is_empty() {
        current_spans.push(Span::styled("  ", dim));
    }
    current_spans.push(Span::styled(
        "f",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    current_spans.push(Span::styled(" list", dim));

    lines.push(Line::from(current_spans));
    lines
}

fn empty_composer_placeholder(view: &ComposerBlockView<'_>, width: usize) -> Paragraph<'static> {
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
        reaction_picker_placeholder_lines(dim, width)
    } else if view.selected_image_message {
        vec![Line::from(Span::styled(
            "f react · r reply · e edit · d delete · p profile · c copy · Enter view image",
            dim,
        ))]
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
            "Type a message · j/k select · Ctrl+] icon picker · or just ask @bot about anything",
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
        frame.render_widget(
            empty_composer_placeholder(view, text_area.width as usize),
            text_area,
        );
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
    width: usize,
) -> usize {
    if composer.is_empty() && !mention_active && reaction_picker_active {
        reaction_picker_placeholder_lines(Style::default(), width).len()
    } else {
        0
    }
}

fn composer_placeholder_lines(view: &ComposerBlockView<'_>, width: usize) -> usize {
    chat_composer_placeholder_lines(
        view.composer,
        view.mention_active,
        view.reaction_picker_active,
        width,
    )
}

fn split_chat_and_composer(area: Rect, composer_height: u16) -> (Rect, Rect) {
    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(CHAT_COMPOSER_GAP_HEIGHT),
        Constraint::Length(composer_height),
    ])
    .split(area);
    (layout[0], layout[2])
}

fn draw_room_page_effects(frame: &mut Frame, area: Rect, effects: &[ActiveChatRoomEffect]) {
    if effects.is_empty() || area.is_empty() {
        return;
    }
    if has_room_effect(effects, "room_glow") {
        draw_room_glow(frame, area);
    }
    if has_room_effect(effects, "room_pulse") {
        draw_room_pulse(frame, area);
    }
    if has_room_effect(effects, "room_spark") {
        draw_room_sparkles(frame, area);
    }
}

fn draw_room_glow(frame: &mut Frame, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }
    let tick = Utc::now().timestamp_millis().div_euclid(260) as u16;
    let buf = frame.buffer_mut();
    let max_x = area.right().saturating_sub(1);
    let max_y = area.bottom().saturating_sub(1);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let left = x.saturating_sub(area.x);
            let right = max_x.saturating_sub(x);
            let top = y.saturating_sub(area.y);
            let bottom = max_y.saturating_sub(y);
            let edge_distance = left.min(right).min(top).min(bottom);
            if edge_distance > 1 {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, y))
                && (edge_distance == 0 || x.wrapping_add(y).wrapping_add(tick) % 5 == 0)
            {
                let symbol = if edge_distance == 0 { "·" } else { "░" };
                cell.set_symbol(symbol).set_fg(theme::AMBER_GLOW());
            }
        }
    }

    let shimmer_count = (u16::min(area.width, area.height).max(3) / 3).clamp(2, 8);
    for index in 0..shimmer_count {
        let x = area.x + (tick.wrapping_mul(5).wrapping_add(index * 13) % area.width);
        let y = area.y + (tick.wrapping_add(index * 7) % area.height);
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_symbol("·").set_fg(theme::AMBER_DIM());
        }
    }
}

fn draw_room_pulse(frame: &mut Frame, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }
    let tick = Utc::now().timestamp_millis().div_euclid(120) as u16;
    let wave_x = area.x + tick.wrapping_mul(3) % area.width;
    let wave_y = area.y + tick % area.height;
    let buf = frame.buffer_mut();
    for x in area.x..area.right() {
        if x.wrapping_add(tick) % 2 == 0
            && let Some(cell) = buf.cell_mut((x, wave_y))
        {
            cell.set_symbol("·").set_fg(theme::SUCCESS());
        }
    }
    for y in area.y..area.bottom() {
        if y.wrapping_add(tick) % 2 == 0
            && let Some(cell) = buf.cell_mut((wave_x, y))
        {
            cell.set_symbol("·").set_fg(theme::AMBER_DIM());
        }
    }
}

fn draw_room_sparkles(frame: &mut Frame, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }
    const GLYPHS: [&str; 4] = ["*", "+", "✦", "·"];
    let seed = Utc::now().timestamp_millis().div_euclid(180) as u64;
    let cell_count = u64::from(area.width) * u64::from(area.height);
    let sparkle_count = (cell_count / 70).clamp(8, 36);
    let buf = frame.buffer_mut();
    for index in 0..sparkle_count {
        let mixed = seed
            .wrapping_mul(1_103_515_245)
            .wrapping_add(index.wrapping_mul(2_654_435_761))
            .wrapping_add(12_345);
        let x = area.x + (mixed % u64::from(area.width)) as u16;
        let y = area.y + ((mixed / 97) % u64::from(area.height)) as u16;
        if let Some(cell) = buf.cell_mut((x, y)) {
            let glyph = GLYPHS[(mixed as usize) % GLYPHS.len()];
            cell.set_symbol(glyph).set_fg(room_sparkle_color(mixed));
        }
    }
}

fn room_sparkle_color(seed: u64) -> Color {
    match seed % 3 {
        0 => theme::AMBER_GLOW(),
        1 => theme::SUCCESS(),
        _ => theme::TEXT_BRIGHT(),
    }
}

fn split_poll_and_messages(area: Rect, poll: Option<&ActiveChatPoll>) -> (Option<Rect>, Rect) {
    let Some(poll) = poll else {
        return (None, area);
    };
    if area.width < 24 {
        return (None, area);
    }
    // One row per option, plus the top and bottom borders.
    let poll_height = poll.options.len().max(1) as u16 + 2;
    // Keep at least a few rows for the conversation itself.
    if area.height < poll_height + 3 {
        return (None, area);
    }
    let split = Layout::vertical([Constraint::Length(poll_height), Constraint::Min(1)]).split(area);
    (Some(split[0]), split[1])
}

fn draw_poll_strip(frame: &mut Frame, area: Rect, poll: &ActiveChatPoll) {
    let bg = Style::default().bg(theme::BG_CANVAS());
    let inner_width = area.width.saturating_sub(2) as usize;

    let total_votes = poll
        .options
        .iter()
        .map(|option| option.vote_count.max(0))
        .sum::<i64>();

    // Top border: question on the left, countdown + tally on the right.
    let remaining_secs = (poll.poll.ends_at - Utc::now()).num_seconds();
    let meta_spans = poll_meta_spans(remaining_secs, total_votes);
    let meta_width: usize = meta_spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum();
    // Reserve the meta title, the " Poll · " + trailing-space chrome (9
    // cells), and a 1-cell gap so a long question never collides with the
    // right-aligned countdown.
    let question_budget = inner_width.saturating_sub(meta_width + 10).max(4);
    let question = truncate_cells(poll.poll.question.as_str(), question_budget);
    let title_left = Line::from(vec![Span::styled(
        format!(" Poll · {question} "),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(bg)
        .title_top(title_left)
        .title_top(Line::from(meta_spans).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if poll.options.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " No options yet ",
                Style::default().fg(theme::TEXT_DIM()),
            )))
            .style(bg),
            inner,
        );
        return;
    }

    // Shared column widths so every row's slider starts and ends in the
    // same place regardless of label length or vote tally.
    let max_label = poll
        .options
        .iter()
        .map(|option| UnicodeWidthStr::width(option.label.as_str()))
        .max()
        .unwrap_or(4);
    let stats: Vec<String> = poll
        .options
        .iter()
        .map(|option| poll_stat_text(option.vote_count.max(0), total_votes))
        .collect();
    let stats_width = stats
        .iter()
        .map(|stat| UnicodeWidthStr::width(stat.as_str()))
        .max()
        .unwrap_or(0);

    // pad(1) marker(1) key(2) sp(1) label sp(1) bar sp(1) stats pad(1).
    const FIXED: usize = 8;
    const MIN_BAR: usize = 6;
    let mut label_width = max_label.clamp(4, 18);
    if label_width + stats_width + FIXED + MIN_BAR > inner_width {
        let over = label_width + stats_width + FIXED + MIN_BAR - inner_width;
        label_width = label_width.saturating_sub(over).max(3);
    }
    let bar_width = inner_width
        .saturating_sub(label_width + stats_width + FIXED)
        .max(1);

    let lines: Vec<Line<'static>> = poll
        .options
        .iter()
        .zip(stats.iter())
        .map(|(option, stat)| {
            poll_option_row(
                option,
                stat,
                poll.my_vote_option_id == Some(option.id),
                total_votes,
                label_width,
                bar_width,
                stats_width,
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).style(bg), inner);
}

/// Right-aligned border meta: "ends in 9m · 2 votes".
fn poll_meta_spans(remaining_secs: i64, total_votes: i64) -> Vec<Span<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut spans = vec![Span::styled(" ", dim)];
    if remaining_secs <= 0 {
        spans.push(Span::styled("ended", faint));
    } else {
        spans.push(Span::styled("ends in ", dim));
        spans.push(Span::styled(
            format_poll_remaining(remaining_secs),
            Style::default().fg(theme::AMBER()),
        ));
    }
    spans.push(Span::styled(" · ", faint));
    spans.push(Span::styled(
        format!(
            "{total_votes} vote{}",
            if total_votes == 1 { "" } else { "s" }
        ),
        dim,
    ));
    spans.push(Span::styled(" ", dim));
    spans
}

fn format_poll_remaining(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", (secs + 59) / 60)
    } else {
        format!("{}h", (secs + 3599) / 3600)
    }
}

fn poll_stat_text(count: i64, total: i64) -> String {
    let pct = if total > 0 {
        ((count * 100 + total / 2) / total).clamp(0, 100)
    } else {
        0
    };
    format!("{count} · {pct}%")
}

/// A single option as a labelled horizontal slider:
/// `▸ v1 yes        ███████░░░░░░░  2 · 100%`
fn poll_option_row(
    option: &ChatPollOptionSummary,
    stat: &str,
    selected: bool,
    total: i64,
    label_width: usize,
    bar_width: usize,
    stats_width: usize,
) -> Line<'static> {
    let count = option.vote_count.max(0);
    let filled = if total > 0 {
        (((count * bar_width as i64) + total / 2) / total).clamp(0, bar_width as i64) as usize
    } else {
        0
    };

    let accent = if selected {
        theme::SUCCESS()
    } else {
        theme::AMBER()
    };
    let fill_style = Style::default().fg(if selected {
        theme::SUCCESS()
    } else {
        theme::AMBER_GLOW()
    });
    let empty_style = Style::default().fg(theme::TEXT_FAINT());
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };

    let marker = if selected {
        Span::styled(
            "▸",
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw(" ")
    };

    let label = pad_to_width(&truncate_cells(&option.label, label_width), label_width);
    let stat_cell = pad_left_to_width(stat, stats_width);

    Line::from(vec![
        Span::raw(" "),
        marker,
        Span::styled(
            format!("v{}", option.position),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(label, label_style),
        Span::raw(" "),
        Span::styled("█".repeat(filled), fill_style),
        Span::styled("░".repeat(bar_width.saturating_sub(filled)), empty_style),
        Span::raw(" "),
        Span::styled(stat_cell, Style::default().fg(theme::TEXT_DIM())),
        Span::raw(" "),
    ])
}

fn pad_to_width(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    if used >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - used))
    }
}

fn pad_left_to_width(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    if used >= width {
        text.to_string()
    } else {
        format!("{}{text}", " ".repeat(width - used))
    }
}

fn truncate_cells(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    let ellipsis_width = UnicodeWidthStr::width("…");
    for ch in text.chars() {
        let width = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + width + ellipsis_width > max_width {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push('…');
    out
}

pub fn draw_dashboard_chat_card(
    frame: &mut Frame,
    area: Rect,
    view: DashboardChatView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let total_composer_lines = chat_composer_lines_for_height(view.composer, composer_text_width)
        .max(composer_placeholder_lines(
            &ComposerBlockView {
                composer: view.composer,
                composing: view.composing,
                selected_message: view.selected_message_id.is_some(),
                selected_image_message: view.selected_image_message,
                selected_news_message: view.selected_news_message,
                reaction_picker_active: view.reaction_picker_active,
                reply_author: view.reply_author,
                is_editing: view.is_editing,
                mention_active: view.mention_active,
                mention_matches: view.mention_matches,
                mention_selected: view.mention_selected,
                keep_composer_focused: view.keep_composer_focused,
            },
            composer_text_width,
        ));
    let visible_composer_lines = total_composer_lines.min(5);
    let composer_height = visible_composer_lines as u16 + 2;
    let (messages_area, composer_area) = split_chat_and_composer(area, composer_height);
    let (poll_area, messages_area) = split_poll_and_messages(messages_area, view.active_poll);

    let lines: Vec<Line<'static>>;
    let mut chat_hits: Option<Vec<ChatRowHit>> = None;
    if view.messages.is_empty() {
        lines = vec![Line::from(Span::styled(
            "No messages yet.",
            Style::default().fg(theme::TEXT_DIM()),
        ))];
    } else {
        let height = messages_area.height.max(1) as usize;
        let width = messages_area.width.max(1) as usize;
        ensure_chat_rows_cache(
            view.rows_cache,
            view.messages.iter().collect(),
            width,
            ChatRowsContext {
                current_user_id: view.current_user_id,
                afk_user_ids: view.afk_user_ids,
                show_flag_fallback: view.show_flag_fallback,
                usernames: view.usernames,
                countries: view.countries,
                friend_user_ids: view.friend_user_ids,
                bonsai_glyphs: view.bonsai_glyphs,
                chat_badges: view.chat_badges,
                profile_award_badges: view.profile_award_badges,
                bot_username_color_active: view.bot_username_color_active,
                message_reactions: view.message_reactions,
                inline_images: view.inline_images,
            },
        );
        let visible = visible_chat_rows(
            view.rows_cache,
            view.selected_message_id,
            view.highlighted_message_id,
            height,
        );
        lines = visible.lines;
        chat_hits = Some(visible.hits);
    }

    if let Some(poll) = view.active_poll
        && let Some(poll_area) = poll_area
    {
        draw_poll_strip(frame, poll_area, poll);
    }
    frame.render_widget(Paragraph::new(lines), messages_area);
    draw_room_page_effects(frame, messages_area, view.active_room_effects);
    // Only publish the chat-scroll hit layout when nothing is painted on
    // top of the messages (overlay or image modal) — those intercept
    // clicks via their own input paths, so a stale layout here would
    // route clicks to the wrong target.
    if let (Some(slot), Some(hits)) = (view.chat_hit_slot, chat_hits)
        && view.overlay.is_none()
        && view.image_modal.is_none()
    {
        slot.set(Some(ChatHitLayout {
            content: messages_area,
            rows: hits,
        }));
    }
    if let Some(overlay) = view.overlay {
        draw_overlay(frame, messages_area, overlay);
    }
    if let Some(image_modal) = view.image_modal {
        draw_image_modal(frame, messages_area, image_modal, terminal_images);
    }

    draw_composer_block(
        frame,
        composer_area,
        &ComposerBlockView {
            composer: view.composer,
            composing: view.composing,
            selected_message: view.selected_message_id.is_some(),
            selected_image_message: view.selected_image_message,
            selected_news_message: view.selected_news_message,
            reaction_picker_active: view.reaction_picker_active,
            reply_author: view.reply_author,
            is_editing: view.is_editing,
            mention_active: view.mention_active,
            mention_matches: view.mention_matches,
            mention_selected: view.mention_selected,
            keep_composer_focused: view.keep_composer_focused,
        },
    );
    if let Some(slot) = view.composer_rect_slot {
        slot.set(Some(composer_area));
    }
}

// ── Chat rows cache & scroll ────────────────────────────────

struct ChatRowsContext<'a> {
    current_user_id: Uuid,
    afk_user_ids: &'a HashSet<Uuid>,
    show_flag_fallback: bool,
    usernames: &'a UsernameLookup<'a>,
    countries: &'a HashMap<Uuid, String>,
    friend_user_ids: &'a HashSet<Uuid>,
    bonsai_glyphs: &'a HashMap<Uuid, String>,
    chat_badges: &'a HashMap<Uuid, String>,
    profile_award_badges: &'a HashMap<Uuid, String>,
    bot_username_color_active: bool,
    message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    inline_images: &'a HashMap<Uuid, InlineImagePreview>,
}

// ── Mouse hit-test types ────────────────────────────────────
//
// These describe the geometry of the painted chat scroll so `app::input`
// can resolve a click coordinate into a concrete action (select a
// message, open a profile, open the shop on Badges/Flags, open an image
// modal, etc.) without re-running the row builder.
//
// `ChatHitLayout::rows` is aligned 1:1 with the painted screen rows
// returned by `visible_chat_rows` — including the leading blank padding
// rows it inserts when content is shorter than the viewport — so a
// click at screen-row `y - content.y` is a direct index.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderTarget {
    /// Username, friend badge, special badges, bonsai glyph, or BRB
    /// badge — anything author-identifying. Resolves to the profile modal
    /// (debounced; a fast second click instead inserts a mention).
    Profile,
    /// The currently equipped chat-shop badge. Resolves to the Hub
    /// Shop opened on the Badges sub-store.
    StoreBadge,
    /// The currently equipped chat flag. Resolves to the Hub Shop opened
    /// on the Flags sub-store.
    StoreFlag,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HeaderSegment {
    /// Inclusive start column relative to the painted line's first cell
    /// (i.e. column 0 is the leading pad cell).
    pub start_col: u16,
    /// Exclusive end column.
    pub end_col: u16,
    pub target: HeaderTarget,
}

impl HeaderSegment {
    /// `true` when `col` falls inside this segment's half-open
    /// `[start_col, end_col)` range. Used by the chat-scroll click
    /// dispatcher to map a click column onto a username/badge target.
    pub fn contains(&self, col: u16) -> bool {
        col >= self.start_col && col < self.end_col
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) enum ChatRowKind {
    /// Blank padding row (top viewport pad or the separator line
    /// between distinct authors). Clicks fall through.
    #[default]
    None,
    /// Body / reaction-footer row. Clicks select the message.
    Body,
    /// Inline image preview row. Clicks open the image modal.
    Image,
    /// Author header row. Segments tell which sub-region was clicked.
    Header(Vec<HeaderSegment>),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ChatRowHit {
    pub message_id: Option<Uuid>,
    pub kind: ChatRowKind,
}

#[derive(Clone, Debug)]
pub(crate) struct ChatHitLayout {
    /// The rect the message paragraph was painted into. For block-bordered
    /// surfaces this is the inner content rect, not the bordered frame.
    pub content: Rect,
    /// One entry per painted screen row, top to bottom.
    pub rows: Vec<ChatRowHit>,
}

/// Compact per-`all_rows` row classification used internally by
/// `ChatRowsCache`. Lifted into a full `ChatRowKind` (which carries the
/// header segments) when building the per-frame `ChatHitLayout`.
#[derive(Clone, Copy, Debug, Default)]
enum RowKindLite {
    #[default]
    Blank,
    Header,
    Body,
    Image,
}

#[derive(Default)]
pub struct ChatRowsCache {
    width: usize,
    fingerprint: u64,
    all_rows: Vec<Line<'static>>,
    selected_ranges: HashMap<Uuid, (usize, usize)>,
    highlighted_ranges: HashMap<Uuid, (usize, usize)>,
    /// Parallel to `all_rows`: which message id owns each painted row.
    /// `None` on the blank separator inserted between distinct authors.
    row_message: Vec<Option<Uuid>>,
    /// Parallel to `all_rows`: row classification for hit-testing.
    row_kind: Vec<RowKindLite>,
    /// Per-message header column ranges. Only populated when the message
    /// emits a header row (non-news, non-continuation).
    header_segments: HashMap<Uuid, Vec<HeaderSegment>>,
}

fn chat_rows_fingerprint(
    messages: &[&ChatMessage],
    ctx: &ChatRowsContext<'_>,
    width: usize,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    ctx.current_user_id.hash(&mut hasher);
    ctx.show_flag_fallback.hash(&mut hasher);
    ctx.bot_username_color_active.hash(&mut hasher);
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
        ctx.friend_user_ids.contains(&msg.user_id).hash(&mut hasher);
        ctx.afk_user_ids.contains(&msg.user_id).hash(&mut hasher);
        ctx.bonsai_glyphs.get(&msg.user_id).hash(&mut hasher);
        ctx.chat_badges.get(&msg.user_id).hash(&mut hasher);
        ctx.profile_award_badges.get(&msg.user_id).hash(&mut hasher);
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
    let mut row_message: Vec<Option<Uuid>> = Vec::new();
    let mut row_kind: Vec<RowKindLite> = Vec::new();
    let mut selected_ranges = HashMap::new();
    let mut highlighted_ranges = HashMap::new();
    let mut header_segments: HashMap<Uuid, Vec<HeaderSegment>> = HashMap::new();
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
        let is_friend = ctx.friend_user_ids.contains(&msg.user_id);
        let author_style = if is_own {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else if is_friend {
            Style::default()
                .fg(theme::BADGE_GOLD())
                .add_modifier(Modifier::BOLD)
        } else if is_bot && ctx.bot_username_color_active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else if is_bot {
            Style::default().fg(theme::BOT())
        } else {
            Style::default().fg(theme::CHAT_AUTHOR())
        };
        let body_style = Style::default().fg(theme::CHAT_BODY());

        let special_list = super::special_badges::special_badges(&author);
        let raw_chat_badge_opt = ctx
            .chat_badges
            .get(&msg.user_id)
            .map(String::as_str)
            .filter(|s| !s.is_empty());
        let chat_badges = raw_chat_badge_opt.map_or_else(Vec::new, |badge| {
            chat_badge_display_parts(badge, ctx.show_flag_fallback)
        });
        let chat_badge_refs = chat_badges
            .iter()
            .map(|(target, text)| (*target, text.as_ref()))
            .collect::<Vec<_>>();
        let bonsai_opt = ctx
            .bonsai_glyphs
            .get(&msg.user_id)
            .map(String::as_str)
            .filter(|s| !s.is_empty());
        let profile_award_badge = ctx
            .profile_award_badges
            .get(&msg.user_id)
            .map(String::as_str)
            .filter(|s| !s.is_empty());
        let afk_badge = ctx.afk_user_ids.contains(&msg.user_id).then_some(AFK_BADGE);
        let (prefix, segments) = build_author_prefix_and_segments_with_chat_badges(
            is_friend,
            &author,
            special_list,
            &chat_badge_refs,
            bonsai_opt,
            profile_award_badge,
            afk_badge,
        );

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
            row_message.push(None);
            row_kind.push(RowKindLite::Blank);
        }
        first = false;

        let row_start = all_rows.len();
        let image_lines = ctx.inline_images.get(&msg.id).map(Vec::as_slice);
        let wrapped = wrap_chat_entry_to_lines(
            &msg.body,
            &stamp,
            &prefix,
            width,
            author_style,
            body_style,
            mentions_us,
            is_continuation,
            image_lines,
            reactions,
        );
        let line_count = wrapped.lines.len();
        all_rows.extend(wrapped.lines);

        // Classify each row this message contributed, in lockstep with
        // `all_rows`. Reaction-footer rows fall through to `Body`, which
        // means a click on a reaction chip still selects the message —
        // acceptable since reactions are keyboard-only today.
        for i in 0..line_count {
            row_message.push(Some(msg.id));
            let kind = if wrapped.header_line_index == Some(i) {
                RowKindLite::Header
            } else if wrapped
                .image_line_range
                .is_some_and(|(s, e)| i >= s && i < e)
            {
                RowKindLite::Image
            } else {
                RowKindLite::Body
            };
            row_kind.push(kind);
        }

        if wrapped.header_line_index.is_some() && !segments.is_empty() {
            header_segments.insert(msg.id, segments);
        }

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

    debug_assert_eq!(all_rows.len(), row_message.len());
    debug_assert_eq!(all_rows.len(), row_kind.len());

    cache.width = width;
    cache.fingerprint = fingerprint;
    cache.all_rows = all_rows;
    cache.row_message = row_message;
    cache.row_kind = row_kind;
    cache.selected_ranges = selected_ranges;
    cache.highlighted_ranges = highlighted_ranges;
    cache.header_segments = header_segments;
}

/// Output of `visible_chat_rows`: the painted screen lines and a parallel
/// per-row hit vector. `hits.len() == lines.len()`, top-aligned to the
/// viewport (so any leading padding rows added when content is shorter
/// than `height` have matching `ChatRowHit { message_id: None, kind:
/// ChatRowKind::None }` entries). Callers feed `hits` into the
/// `ChatHitLayout` cell so `app::input` can map clicks back to messages.
pub(crate) struct VisibleChatRows {
    pub lines: Vec<Line<'static>>,
    pub hits: Vec<ChatRowHit>,
}

fn visible_chat_rows(
    cache: &ChatRowsCache,
    selected_message_id: Option<Uuid>,
    highlighted_message_id: Option<Uuid>,
    height: usize,
) -> VisibleChatRows {
    let total_rows = cache.all_rows.len();
    if total_rows == 0 {
        return VisibleChatRows {
            lines: Vec::new(),
            hits: Vec::new(),
        };
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
    let mut hits: Vec<ChatRowHit> = (visible_start..visible_end)
        .map(|idx| {
            let kind = match cache.row_kind.get(idx).copied().unwrap_or_default() {
                RowKindLite::Blank => ChatRowKind::None,
                RowKindLite::Body => ChatRowKind::Body,
                RowKindLite::Image => ChatRowKind::Image,
                RowKindLite::Header => {
                    let segs = cache
                        .row_message
                        .get(idx)
                        .and_then(|maybe| maybe.as_ref())
                        .and_then(|id| cache.header_segments.get(id).cloned())
                        .unwrap_or_default();
                    ChatRowKind::Header(segs)
                }
            };
            ChatRowHit {
                message_id: cache.row_message.get(idx).copied().flatten(),
                kind,
            }
        })
        .collect();

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
        // Leading blank rows pad the top of the viewport, so prepend
        // matching "no-op" hit entries to keep the vectors aligned 1:1.
        let mut padded_lines = vec![Line::from(""); pad];
        padded_lines.append(&mut lines);
        let mut padded_hits = vec![ChatRowHit::default(); pad];
        padded_hits.append(&mut hits);
        return VisibleChatRows {
            lines: padded_lines,
            hits: padded_hits,
        };
    }

    VisibleChatRows { lines, hits }
}

fn draw_image_modal(
    frame: &mut Frame,
    anchor: Rect,
    view: ImageModalView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if anchor.width < 16 || anchor.height < 7 {
        return;
    }

    let max_popup_width = anchor.width.saturating_sub(4).clamp(12, 132);
    let max_popup_height = anchor.height.saturating_sub(2).max(5);
    let modal_bg = Style::default().bg(theme::BG_CANVAS());

    let terminal_image = view.terminal_image.filter(|data| {
        if view.terminal_image_protocol != Some(TerminalImageProtocol::Sixel) {
            return true;
        }
        let max_image_width = max_popup_width.saturating_sub(4).max(1);
        let max_image_height = max_popup_height.saturating_sub(4).max(1);
        data.display_cols <= max_image_width && data.display_rows <= max_image_height
    });

    if let Some(data) = terminal_image {
        let max_image_width = max_popup_width.saturating_sub(4).max(1);
        let max_image_height = max_popup_height.saturating_sub(4).max(1);
        let (image_width, image_height) = fit_terminal_image_cells(
            data.display_cols,
            data.display_rows,
            max_image_width,
            max_image_height,
        );
        let popup_width = image_width
            .saturating_add(4)
            .max(18)
            .min(max_popup_width)
            .max(1);
        let popup_height = image_height
            .saturating_add(3)
            .max(5)
            .min(max_popup_height)
            .max(1);
        let title = pick_title_that_fits(popup_width, &[" Image Preview ", " Image ", ""]);
        let popup = centered_rect_in(anchor, popup_width, popup_height);
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .title(title)
            .title_style(
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
        // Kitty images sit behind text cells; keep this block background-free
        // or the modal will paint over the native image.
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let footer_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
        let image_slots_height = inner.height.saturating_sub(1);
        let image_area = Rect::new(
            inner.x + inner.width.saturating_sub(image_width) / 2,
            inner.y + image_slots_height.saturating_sub(image_height) / 2,
            image_width.min(inner.width),
            image_height.min(image_slots_height),
        );

        if image_area.width > 0 && image_area.height > 0 {
            terminal_images.push(TerminalImagePlacement {
                message_id: view.message_id,
                area: image_area,
                data: data.clone(),
            });
        }

        frame.render_widget(
            Paragraph::new(image_modal_footer(footer_area.width)),
            footer_area,
        );
        return;
    }

    let fallback_lines = image_modal_fallback_lines(view);
    let widest = fallback_lines
        .iter()
        .map(line_display_width)
        .max()
        .unwrap_or(0) as u16;
    let popup_width = widest.saturating_add(4).max(34).min(max_popup_width).max(1);
    let content_height = (fallback_lines.len() as u16)
        .min(max_popup_height.saturating_sub(3).max(1))
        .max(1);
    let popup_height = content_height
        .saturating_add(3)
        .max(5)
        .min(max_popup_height)
        .max(1);
    let title = pick_title_that_fits(popup_width, &[" Image Preview ", " Image ", ""]);
    let popup = centered_rect_in(anchor, popup_width, popup_height);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(modal_bg);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let footer_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
    let content_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );
    frame.render_widget(Paragraph::new(fallback_lines).style(modal_bg), content_area);
    frame.render_widget(
        Paragraph::new(image_modal_footer(footer_area.width)).style(modal_bg),
        footer_area,
    );
}

fn fit_terminal_image_cells(cols: u16, rows: u16, max_cols: u16, max_rows: u16) -> (u16, u16) {
    if cols == 0 || rows == 0 || max_cols == 0 || max_rows == 0 {
        return (1, 1);
    }

    let mut fitted_cols = cols.min(max_cols).max(1);
    let mut fitted_rows = ((u32::from(fitted_cols) * u32::from(rows))
        .div_ceil(u32::from(cols))
        .max(1)) as u16;
    if fitted_rows > max_rows {
        fitted_rows = max_rows.max(1);
        fitted_cols = ((u32::from(fitted_rows) * u32::from(cols))
            .div_ceil(u32::from(rows))
            .max(1) as u16)
            .min(max_cols)
            .max(1);
    }

    (fitted_cols, fitted_rows)
}

fn centered_rect_in(anchor: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(anchor.width);
    let height = height.min(anchor.height);
    Rect::new(
        anchor.x + anchor.width.saturating_sub(width) / 2,
        anchor.y + anchor.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn image_modal_fallback_lines(view: ImageModalView<'_>) -> Vec<Line<'static>> {
    if let Some(preview) = view.preview {
        return preview.clone();
    }

    vec![
        Line::from(Span::styled(
            "Loading image preview...",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            view.url.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]
}

fn image_modal_footer(width: u16) -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default().fg(theme::AMBER_DIM());
    if width >= 32 {
        return Line::from(vec![
            Span::styled(" Enter/c", key),
            Span::styled(" copy", dim),
            Span::styled("  · ", Style::default().fg(theme::BORDER())),
            Span::styled("Esc/q", key),
            Span::styled(" close", dim),
        ]);
    }
    if width >= 20 {
        return Line::from(vec![
            Span::styled(" Enter", key),
            Span::styled(" copy ", dim),
            Span::styled("Esc", key),
            Span::styled(" close", dim),
        ]);
    }
    Line::from(vec![Span::styled("Esc", key), Span::styled(" close", dim)])
}

fn line_display_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
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

fn chat_badge_display(badge: &str, show_flag_fallback: bool) -> Cow<'_, str> {
    if show_flag_fallback {
        if let Some((label, rest)) = regional_flag_label_prefix(badge) {
            return Cow::Owned(format!("{label}{rest}"));
        }
        if let Some((label, rest)) = subdivision_flag_label_prefix(badge) {
            return Cow::Owned(format!("{label}{rest}"));
        }
    }
    Cow::Borrowed(badge)
}

fn chat_badge_display_parts(
    badge: &str,
    show_flag_fallback: bool,
) -> Vec<(HeaderTarget, Cow<'_, str>)> {
    let Some((flag, rest)) = chat_flag_display_prefix(badge, show_flag_fallback) else {
        return vec![(
            HeaderTarget::StoreBadge,
            chat_badge_display(badge, show_flag_fallback),
        )];
    };
    let mut parts = Vec::new();
    let rest = rest.trim_start();
    if !rest.is_empty() {
        parts.push((HeaderTarget::StoreBadge, Cow::Borrowed(rest)));
    }
    parts.push((HeaderTarget::StoreFlag, flag));
    parts
}

fn chat_flag_display_prefix(badge: &str, show_flag_fallback: bool) -> Option<(Cow<'_, str>, &str)> {
    if show_flag_fallback {
        if let Some((label, rest)) = regional_flag_label_prefix(badge) {
            return Some((Cow::Owned(label), rest));
        }
        if let Some((label, rest)) = subdivision_flag_label_prefix(badge) {
            return Some((Cow::Borrowed(label), rest));
        }
        return None;
    }
    if let Some((flag, rest)) = regional_flag_prefix(badge) {
        return Some((Cow::Borrowed(flag), rest));
    }
    subdivision_flag_prefix(badge).map(|(flag, rest)| (Cow::Borrowed(flag), rest))
}

fn regional_flag_prefix(badge: &str) -> Option<(&str, &str)> {
    let mut chars = badge.char_indices();
    let (_, a) = chars.next()?;
    regional_indicator_letter(a)?;
    let (b_idx, b) = chars.next()?;
    regional_indicator_letter(b)?;
    let end = b_idx + b.len_utf8();
    Some((&badge[..end], &badge[end..]))
}

fn regional_flag_label_prefix(badge: &str) -> Option<(String, &str)> {
    let mut chars = badge.chars();
    let a = regional_indicator_letter(chars.next()?)?;
    let b = regional_indicator_letter(chars.next()?)?;
    let rest = chars.as_str();
    Some((format!("{a}{b}"), rest))
}

fn regional_indicator_letter(ch: char) -> Option<char> {
    let code = ch as u32;
    (0x1F1E6..=0x1F1FF)
        .contains(&code)
        .then(|| char::from_u32(('A' as u32) + code - 0x1F1E6))
        .flatten()
}

fn subdivision_flag_label_prefix(badge: &str) -> Option<(&'static str, &str)> {
    let (tag, rest) = subdivision_flag_tag_prefix(badge)?;
    match tag.as_str() {
        "gbeng" => Some(("england", rest)),
        "gbsct" => Some(("scotland", rest)),
        "gbwls" => Some(("wales", rest)),
        _ => None,
    }
}

fn subdivision_flag_tag_prefix(badge: &str) -> Option<(String, &str)> {
    let mut chars = badge.chars();
    (chars.next()? == '🏴').then_some(())?;
    let mut tag = String::new();
    while let Some(ch) = chars.next() {
        let code = ch as u32;
        if code == 0xE007F {
            return Some((tag, chars.as_str()));
        }
        if (0xE0061..=0xE007A).contains(&code) {
            tag.push(char::from_u32(('a' as u32) + code - 0xE0061)?);
        }
    }
    None
}

fn subdivision_flag_prefix(badge: &str) -> Option<(&str, &str)> {
    let mut chars = badge.char_indices();
    let (_, first) = chars.next()?;
    (first == '🏴').then_some(())?;
    for (idx, ch) in chars {
        let code = ch as u32;
        if code == 0xE007F {
            let end = idx + ch.len_utf8();
            return Some((&badge[..end], &badge[end..]));
        }
        if !(0xE0061..=0xE007A).contains(&code) {
            return None;
        }
    }
    None
}

/// Build the chat-author prefix string and matching per-segment column
/// ranges for mouse hit-testing in one pass. The returned `prefix` is
/// byte-for-byte what `format!("{FRIEND_BADGE} {author}{author_badges}")`
/// (or the no-friend variant) used to produce — the legacy
/// `format_author_badge_suffix` regression tests still pin that shape.
///
/// Returned column ranges are relative to the start of the painted
/// line, where column 0 is the leading pad cell (`" "` or `"│"`) and
/// the prefix begins at column 1. Badges render in the canonical order:
/// special badges, bonsai stage, equipped store badge, equipped flag, then
/// AFK. Special badges, the bonsai glyph, and the AFK badge map to
/// `HeaderTarget::Profile`; equipped chat-shop badges map to
/// `HeaderTarget::StoreBadge`, and equipped chat flags map to
/// `HeaderTarget::StoreFlag`. The trailing `[stamp]` span and the gap
/// spaces between badges are intentionally omitted — clicks there fall
/// through to body-select.
#[cfg(test)]
fn build_author_prefix_and_segments(
    is_friend: bool,
    author: &str,
    special_badges: &[&str],
    chat_badge: Option<&str>,
    bonsai_glyph: Option<&str>,
    profile_award_badge: Option<&str>,
    afk_badge: Option<&str>,
) -> (String, Vec<HeaderSegment>) {
    let mut chat_badges = Vec::new();
    if let Some(chat_badge) = chat_badge {
        chat_badges.push((HeaderTarget::StoreBadge, chat_badge));
    }
    build_author_prefix_and_segments_with_chat_badges(
        is_friend,
        author,
        special_badges,
        &chat_badges,
        bonsai_glyph,
        profile_award_badge,
        afk_badge,
    )
}

fn build_author_prefix_and_segments_with_chat_badges(
    is_friend: bool,
    author: &str,
    special_badges: &[&str],
    chat_badges: &[(HeaderTarget, &str)],
    bonsai_glyph: Option<&str>,
    profile_award_badge: Option<&str>,
    afk_badge: Option<&str>,
) -> (String, Vec<HeaderSegment>) {
    let mut prefix = String::new();
    let mut segments: Vec<HeaderSegment> = Vec::new();
    // The painted line is `[pad (1 cell)][prefix][ stamp]`, so prefix
    // begins at column 1. Pad width is fixed at 1 across both the
    // `" "` and `"│"` mention variants.
    let mut col: u16 = 1;

    if is_friend {
        let glyph_w = UnicodeWidthStr::width(FRIEND_BADGE) as u16;
        if glyph_w > 0 {
            segments.push(HeaderSegment {
                start_col: col,
                end_col: col + glyph_w,
                target: HeaderTarget::Profile,
            });
        }
        prefix.push_str(FRIEND_BADGE);
        col += glyph_w;
        prefix.push(' ');
        col += 1;
    }

    let author_w = UnicodeWidthStr::width(author) as u16;
    if author_w > 0 {
        segments.push(HeaderSegment {
            start_col: col,
            end_col: col + author_w,
            target: HeaderTarget::Profile,
        });
    }
    prefix.push_str(author);
    col += author_w;

    let mut typed_badges: Vec<(HeaderTarget, &str)> = Vec::with_capacity(
        special_badges.len()
            + chat_badges.len()
            + bonsai_glyph.is_some() as usize
            + profile_award_badge.is_some() as usize
            + afk_badge.is_some() as usize,
    );
    for s in special_badges.iter().copied().filter(|s| !s.is_empty()) {
        typed_badges.push((HeaderTarget::Profile, s));
    }
    if let Some(s) = bonsai_glyph.filter(|s| !s.is_empty()) {
        typed_badges.push((HeaderTarget::Profile, s));
    }
    if let Some(s) = profile_award_badge.filter(|s| !s.is_empty()) {
        typed_badges.push((HeaderTarget::Profile, s));
    }
    for (target, s) in chat_badges.iter().copied().filter(|(_, s)| !s.is_empty()) {
        typed_badges.push((target, s));
    }
    if let Some(s) = afk_badge.filter(|s| !s.is_empty()) {
        typed_badges.push((HeaderTarget::Profile, s));
    }
    if !typed_badges.is_empty() {
        prefix.push(' ');
        col += 1;
        let sep_w = UnicodeWidthStr::width(AUTHOR_BADGE_SEPARATOR) as u16;
        for (i, (target, text)) in typed_badges.iter().enumerate() {
            if i > 0 {
                prefix.push_str(AUTHOR_BADGE_SEPARATOR);
                col += sep_w;
            }
            let w = UnicodeWidthStr::width(*text) as u16;
            if w > 0 {
                segments.push(HeaderSegment {
                    start_col: col,
                    end_col: col + w,
                    target: *target,
                });
            }
            prefix.push_str(text);
            col += w;
        }
    }

    (prefix, segments)
}

/// Legacy badge-suffix formatter. Production code now builds the author
/// prefix piece-by-piece in `build_author_prefix_and_segments` so it can
/// capture per-segment column ranges for mouse hit-testing, but the
/// existing unit tests for the badge-ordering invariant still call this
/// helper — they double as a regression check that the inline build
/// keeps the same `" {joined}"` shape.
#[cfg(test)]
fn format_author_badge_suffix(
    special_badges: &[&str],
    chat_badge: Option<&str>,
    bonsai_badge: Option<&str>,
) -> String {
    let extra_badge = usize::from(chat_badge.is_some()) + usize::from(bonsai_badge.is_some());
    let mut badges = Vec::with_capacity(special_badges.len() + extra_badge);
    badges.extend(
        special_badges
            .iter()
            .copied()
            .filter(|badge| !badge.is_empty()),
    );
    if let Some(badge) = bonsai_badge.filter(|badge| !badge.is_empty()) {
        badges.push(badge);
    }
    if let Some(badge) = chat_badge.filter(|badge| !badge.is_empty()) {
        badges.push(badge);
    }

    if badges.is_empty() {
        String::new()
    } else {
        format!(" {}", badges.join(AUTHOR_BADGE_SEPARATOR))
    }
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
    pub image_modal: Option<ImageModalView<'a>>,
    pub usernames: &'a UsernameLookup<'a>,
    pub countries: &'a HashMap<Uuid, String>,
    pub friend_user_ids: &'a HashSet<Uuid>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub inline_images: &'a HashMap<Uuid, InlineImagePreview>,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub room_last_message_at: &'a HashMap<Uuid, Option<DateTime<Utc>>>,
    pub favorite_room_ids: &'a [Uuid],
    pub active_room_effects: &'a HashMap<Uuid, Vec<ActiveChatRoomEffect>>,
    pub active_poll: Option<&'a ActiveChatPoll>,
    pub collapsed_sections: &'a HashSet<RoomSection>,
    pub selected_room_id: Option<Uuid>,
    pub selected_bumped_join_room_id: Option<Uuid>,
    pub room_jump_active: bool,
    pub room_section_prefix_armed: bool,
    pub selected_message_id: Option<Uuid>,
    pub selected_image_message: bool,
    pub selected_news_message: bool,
    pub reaction_picker_active: bool,
    pub highlighted_message_id: Option<Uuid>,
    pub composer: &'a TextArea<'static>,
    pub composing: bool,
    pub current_user_id: Uuid,
    pub afk_user_ids: &'a HashSet<Uuid>,
    pub show_flag_fallback: bool,
    pub cursor_visible: bool,
    pub mention_matches: &'a [MentionMatch],
    pub mention_selected: usize,
    pub mention_active: bool,
    pub reply_author: Option<&'a str>,
    pub is_editing: bool,
    pub bonsai_glyphs: &'a HashMap<Uuid, String>,
    pub chat_badges: &'a HashMap<Uuid, String>,
    pub profile_award_badges: &'a HashMap<Uuid, String>,
    pub bot_username_color_active: bool,
    pub news_composer: &'a TextArea<'static>,
    pub news_composing: bool,
    pub news_processing: bool,
    pub notifications_selected: bool,
    pub notifications_unread_count: i64,
    pub notifications_view: super::notifications::ui::NotificationListView<'a>,
    pub voice_selected: bool,
    pub voice_participant_count: usize,
    pub voice_view: crate::app::voice::ui::VoiceRoomView<'a>,
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
    pub keep_composer_focused: bool,
    /// Cell that, when present, receives the composer block rect so mouse
    /// hit-testing in `app::input` can detect double-clicks into the bar.
    pub composer_rect_slot: Option<&'a std::cell::Cell<Option<Rect>>>,
    /// Cell that, when present, receives this frame's chat-scroll hit
    /// layout — only set in the real-room message branch (synthetic
    /// entries like Discover/News/Showcase don't produce one).
    pub(crate) chat_hit_slot: Option<&'a std::cell::Cell<Option<ChatHitLayout>>>,
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
    pub usernames: &'a UsernameLookup<'a>,
    pub unread_counts: &'a HashMap<Uuid, i64>,
    pub room_last_message_at: &'a HashMap<Uuid, Option<DateTime<Utc>>>,
    pub favorite_room_ids: &'a [Uuid],
    pub active_room_effects: &'a HashMap<Uuid, Vec<ActiveChatRoomEffect>>,
    pub collapsed_sections: &'a HashSet<RoomSection>,
    pub selected_room_id: Option<Uuid>,
    pub selected_bumped_join_room_id: Option<Uuid>,
    pub room_jump_active: bool,
    pub room_section_prefix_armed: bool,
    pub current_user_id: Uuid,
    pub feeds_available: bool,
    pub feeds_selected: bool,
    pub feeds_unread_count: i64,
    pub news_selected: bool,
    pub news_unread_count: i64,
    pub notifications_selected: bool,
    pub notifications_unread_count: i64,
    pub voice_selected: bool,
    pub voice_participant_count: usize,
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
    pub image_modal: Option<ImageModalView<'a>>,
    pub rows_cache: &'a mut ChatRowsCache,
    pub usernames: &'a UsernameLookup<'a>,
    pub countries: &'a HashMap<Uuid, String>,
    pub friend_user_ids: &'a HashSet<Uuid>,
    pub afk_user_ids: &'a HashSet<Uuid>,
    pub message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub inline_images: &'a HashMap<Uuid, InlineImagePreview>,
    pub current_user_id: Uuid,
    pub show_flag_fallback: bool,
    pub selected_message_id: Option<Uuid>,
    pub selected_image_message: bool,
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
    pub chat_badges: &'a HashMap<Uuid, String>,
    pub profile_award_badges: &'a HashMap<Uuid, String>,
    pub keep_composer_focused: bool,
    /// Cell that, when present, receives the composer block rect so mouse
    /// hit-testing in `app::input` can detect double-clicks into the bar.
    pub composer_rect_slot: Option<&'a std::cell::Cell<Option<Rect>>>,
    /// Cell that, when present, receives this frame's chat-scroll hit
    /// layout (with `content` set to the painted text area, not the
    /// bordered frame).
    pub(crate) chat_hit_slot: Option<&'a std::cell::Cell<Option<ChatHitLayout>>>,
}

pub fn draw_embedded_room_chat(
    frame: &mut Frame,
    area: Rect,
    view: EmbeddedRoomChatView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let total_composer_lines = chat_composer_lines_for_height(view.composer, composer_text_width)
        .max(composer_placeholder_lines(
            &ComposerBlockView {
                composer: view.composer,
                composing: view.composing,
                selected_message: view.selected_message_id.is_some(),
                selected_image_message: view.selected_image_message,
                selected_news_message: false,
                reaction_picker_active: view.reaction_picker_active,
                reply_author: view.reply_author,
                is_editing: view.is_editing,
                mention_active: view.mention_active,
                mention_matches: view.mention_matches,
                mention_selected: view.mention_selected,
                keep_composer_focused: view.keep_composer_focused,
            },
            composer_text_width,
        ));
    let composer_height = total_composer_lines.min(4) as u16 + 2;
    let (messages_area, composer_area) = split_chat_and_composer(area, composer_height);

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
            afk_user_ids: view.afk_user_ids,
            show_flag_fallback: view.show_flag_fallback,
            usernames: view.usernames,
            countries: view.countries,
            friend_user_ids: view.friend_user_ids,
            bonsai_glyphs: view.bonsai_glyphs,
            chat_badges: view.chat_badges,
            profile_award_badges: view.profile_award_badges,
            bot_username_color_active: false,
            message_reactions: view.message_reactions,
            inline_images: view.inline_images,
        },
    );
    let visible = visible_chat_rows(
        view.rows_cache,
        view.selected_message_id,
        view.highlighted_message_id,
        height,
    );
    let chat_hits = visible.hits;
    let lines = if visible.lines.is_empty() {
        vec![Line::from(Span::styled(
            "No messages yet",
            Style::default().fg(theme::TEXT_DIM()),
        ))]
    } else {
        visible.lines
    };

    frame.render_widget(messages_block, messages_area);
    frame.render_widget(Paragraph::new(lines), messages_text_area);
    if let (Some(slot), false, false) = (
        view.chat_hit_slot,
        view.overlay.is_some(),
        view.image_modal.is_some(),
    ) {
        slot.set(Some(ChatHitLayout {
            content: messages_text_area,
            rows: chat_hits,
        }));
    }
    if let Some(overlay) = view.overlay {
        draw_overlay(frame, messages_text_area, overlay);
    }
    if let Some(image_modal) = view.image_modal {
        draw_image_modal(frame, messages_text_area, image_modal, terminal_images);
    }

    draw_composer_block(
        frame,
        composer_area,
        &ComposerBlockView {
            composer: view.composer,
            composing: view.composing,
            selected_message: view.selected_message_id.is_some(),
            selected_image_message: view.selected_image_message,
            selected_news_message: false,
            reaction_picker_active: view.reaction_picker_active,
            reply_author: view.reply_author,
            is_editing: view.is_editing,
            mention_active: view.mention_active,
            mention_matches: view.mention_matches,
            mention_selected: view.mention_selected,
            keep_composer_focused: view.keep_composer_focused,
        },
    );
    if let Some(slot) = view.composer_rect_slot {
        slot.set(Some(composer_area));
    }
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

fn room_section_key_prefix(section: RoomSection, active: bool) -> String {
    if active {
        format!("[{}] ", section.shortcut() as char)
    } else {
        String::new()
    }
}

fn strip_room_section_header_prefix(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        if let Some(rest) = trimmed
            .strip_prefix("+ ")
            .or_else(|| trimmed.strip_prefix("- "))
        {
            text = rest;
            continue;
        }
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 4
            && bytes[0] == b'['
            && bytes[2] == b']'
            && bytes[3].is_ascii_whitespace()
        {
            text = &trimmed[4..];
            continue;
        }
        return trimmed;
    }
}

fn chat_selection_mode(view: &ChatRenderInput<'_>, area: Rect) -> ChatSelectionMode {
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    if view.notifications_selected
        || view.voice_selected
        || view.discover_selected
        || view.feeds_selected
    {
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
                composer_placeholder_lines(
                    &ComposerBlockView {
                        composer: view.composer,
                        composing: view.composing,
                        selected_message: view.selected_message_id.is_some(),
                        selected_image_message: view.selected_image_message,
                        selected_news_message: view.selected_news_message,
                        reaction_picker_active: view.reaction_picker_active,
                        reply_author: view.reply_author,
                        is_editing: view.is_editing,
                        mention_active: view.mention_active,
                        mention_matches: view.mention_matches,
                        mention_selected: view.mention_selected,
                        keep_composer_focused: view.keep_composer_focused,
                    },
                    composer_text_width,
                ),
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
        room_last_message_at: view.room_last_message_at,
        favorite_room_ids: view.favorite_room_ids,
        active_room_effects: view.active_room_effects,
        collapsed_sections: view.collapsed_sections,
        selected_room_id: view.selected_room_id,
        selected_bumped_join_room_id: view.selected_bumped_join_room_id,
        room_jump_active: view.room_jump_active,
        room_section_prefix_armed: view.room_section_prefix_armed,
        current_user_id: view.current_user_id,
        feeds_available: view.feeds_view.has_feeds,
        feeds_selected: view.feeds_selected,
        feeds_unread_count: view.feeds_unread_count,
        news_selected: view.news_selected,
        news_unread_count: view.news_unread_count,
        notifications_selected: view.notifications_selected,
        notifications_unread_count: view.notifications_unread_count,
        voice_selected: view.voice_selected,
        voice_participant_count: view.voice_participant_count,
        discover_selected: view.discover_selected,
        showcase_selected: view.showcase_selected,
        showcase_unread_count: view.showcase_unread_count,
        work_selected: view.work_selected,
        work_unread_count: view.work_unread_count,
    }
}

pub(crate) fn home_title_room_label(view: &ChatRenderInput<'_>) -> Option<String> {
    if view.feeds_selected {
        return Some("rss".to_string());
    }
    if view.news_selected {
        return Some("news".to_string());
    }
    if view.notifications_selected {
        return Some("mentions".to_string());
    }
    if view.voice_selected {
        return Some("voice".to_string());
    }
    if view.discover_selected {
        return Some("browse rooms".to_string());
    }
    if view.showcase_selected {
        return Some("showcase".to_string());
    }
    if view.work_selected {
        return Some("work".to_string());
    }

    let room_id = view.selected_room_id?;
    let (room, _) = view
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == room_id)?;
    Some(room_display_label(
        room,
        view.usernames,
        view.current_user_id,
    ))
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
            && !view.voice_selected
            && !view.discover_selected
            && !view.showcase_selected
            && !view.work_selected
            && view.selected_room_id == Some(room_id)
    };

    push_row(section_divider("Core"), None, false);
    let core_order = ["lounge", "announcements", "suggestions", "bugs"];
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

    let voice_line = {
        let prefix = room_jump_prefix(
            view.room_jump_active.then(|| jump_keys.next()).flatten(),
            view.room_jump_active,
            view.voice_selected,
        );
        let style = if view.voice_selected {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        Line::from(Span::styled(format!("{prefix}voice"), style))
    };
    push_row(voice_line, Some(RoomSlot::Voice), view.voice_selected);

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
                format!("{prefix}rss ({})", view.feeds_unread_count)
            } else {
                format!("{prefix}rss")
            };
            Line::from(Span::styled(label, style))
        };
        push_row(feeds_line, Some(RoomSlot::Feeds), view.feeds_selected);
    }

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
    dm_rooms.sort_by(|(a_room, _), (b_room, _)| {
        compare_dm_rooms_for_nav(
            a_room,
            b_room,
            view.current_user_id,
            view.usernames,
            view.unread_counts,
            view.room_last_message_at,
        )
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

    let inner = room_rail_inner_area(rooms_area);
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
    if let Some(slot) = room_rows.hit_slots.get(row_index).copied().flatten() {
        return Some(slot);
    }

    let clicked_line = room_rows
        .lines
        .get(row_index)
        .map(line_text)
        .unwrap_or_default();
    let clicked_line = strip_room_section_header_prefix(clicked_line.trim());
    let search_start = if clicked_line == "channels" {
        row_index + 1
    } else if clicked_line.is_empty()
        && room_rows
            .lines
            .get(row_index + 1)
            .map(line_text)
            .is_some_and(|line| strip_room_section_header_prefix(line.trim()) == "channels")
    {
        row_index + 2
    } else {
        return None;
    };

    room_rows
        .lines
        .iter()
        .zip(room_rows.hit_slots.iter())
        .skip(search_start)
        .take_while(|(line, _)| !line_text(line).trim().is_empty())
        .find_map(|(_, slot)| *slot)
}

/// If the click at `(x, y)` landed on a collapsible section header in the
/// room rail, return that section. Used to toggle collapse on header click.
/// Checked before `room_list_hit_test` so header clicks toggle rather than
/// select a room.
pub(crate) fn room_list_section_hit_test(
    rooms_area: Rect,
    view: &ChatRoomListView<'_>,
    x: u16,
    y: u16,
) -> Option<RoomSection> {
    if view.chat_rooms.is_empty() {
        return None;
    }

    let inner = room_rail_inner_area(rooms_area);
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
    // Header rows carry no slot; strip display affordances back to the section
    // label so clicks keep working while keyboard hints are visible.
    if room_rows
        .hit_slots
        .get(row_index)
        .copied()
        .flatten()
        .is_some()
    {
        return None;
    }
    let text = room_rows.lines.get(row_index).map(line_text)?;
    let label = strip_room_section_header_prefix(text.trim());
    RoomSection::from_label(label)
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
    let inner = room_rail_inner_area(area);

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

fn room_rail_inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(1),
    }
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Builds the cozy rail rows. Active rows are tagged with a sentinel `▌` span
/// at index 0 so the renderer can paint a one-column accent bar in the gutter.
/// That sentinel is stripped before final paint.
fn build_cozy_room_rail_rows(view: &ChatRoomListView<'_>, width: u16) -> RoomListRows {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut hit_slots: Vec<Option<RoomSlot>> = Vec::new();
    let mut selected_row_index = None;
    let inner_width = width.saturating_sub(3) as usize; // 2 left gutter + 1 right margin
    let mut order = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: view.chat_rooms,
        user_id: view.current_user_id,
        usernames: view.usernames,
        unread_counts: view.unread_counts,
        room_last_message_at: view.room_last_message_at,
        feeds_available: view.feeds_available,
        favorite_room_ids: view.favorite_room_ids,
        collapsed_sections: view.collapsed_sections,
    });
    let bumped_slots = bumped_join_room_slots(view.active_room_effects);
    let mut promoted_order = bumped_slots.clone();
    promoted_order.extend(order);
    order = promoted_order;
    let jump_targets: HashMap<RoomSlot, u8> = order
        .iter()
        .copied()
        .zip(ROOM_JUMP_KEYS.iter().copied())
        .collect();

    let blank = || Line::raw("");
    // Collapsible-section header: a leading `+`/`-` toggle drawn in
    // TEXT_BRIGHT so it stays legible against every theme background, then
    // the faint italic label. Clicking anywhere on this row toggles it.
    let collapsed_set = view.collapsed_sections;
    let section_header = |section: RoomSection| -> Line<'static> {
        let collapsed = collapsed_set.contains(&section);
        let toggle = if collapsed { "+ " } else { "- " };
        let mut spans = Vec::new();
        if view.room_section_prefix_armed {
            spans.push(Span::styled(
                room_section_key_prefix(section, true),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        spans.extend([
            Span::styled(
                toggle,
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                section.label().to_string(),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]);
        Line::from(spans)
    };
    let effect_section_header = |label: &'static str| -> Line<'static> {
        Line::from(Span::styled(
            label,
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ))
    };

    let item_row = |label: String,
                    slot: RoomSlot,
                    unread: i64,
                    active: bool,
                    jump_key: Option<u8>,
                    effects: &[ActiveChatRoomEffect]|
     -> Line<'static> {
        let key_prefix = if view.room_jump_active {
            jump_key
                .map(|key| format!("{} ", key as char))
                .unwrap_or_else(|| "  ".to_string())
        } else {
            String::new()
        };
        let effect_suffix = room_effect_suffix(effects);
        let label = format!("{label}{effect_suffix}");
        let key_width = UnicodeWidthStr::width(key_prefix.as_str());
        let label_max = inner_width.saturating_sub(key_width + 4);
        let display_label = if UnicodeWidthStr::width(label.as_str()) > label_max && label_max > 1 {
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
        let pad = inner_width.saturating_sub(used + UnicodeWidthStr::width(unread_str.as_str()));
        let mut spans = Vec::new();
        if active {
            spans.push(Span::raw("▌"));
        }
        let name_color = if active {
            theme::AMBER()
        } else if matches!(slot, RoomSlot::BumpedJoin(_)) {
            theme::TEXT()
        } else if has_room_effect(effects, "pinned_vibe") {
            theme::AMBER_GLOW()
        } else if unread > 0 {
            theme::TEXT()
        } else {
            theme::TEXT_DIM()
        };
        let name_modifier = if active || has_room_effect(effects, "pinned_vibe") {
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
            let effects = room_slot_effects(view, slot);
            push_row(
                item_row(
                    label,
                    slot,
                    unread,
                    active,
                    jump_targets.get(&slot).copied(),
                    effects,
                ),
                Some(slot),
                active,
            );
        };

    // `order` already excludes collapsed sections' rooms, so `favorite_slots`
    // is empty when Favorites is collapsed. `favorite_ids` is derived from the
    // raw favorite list instead — collapse must not change which rooms count
    // as favorites for the Core/Channels/DM exclusions below.
    let favorite_slots: Vec<RoomSlot> = view
        .favorite_room_ids
        .iter()
        .copied()
        .map(RoomSlot::Room)
        .filter(|slot| order.contains(slot))
        .collect();
    let favorite_ids: std::collections::HashSet<Uuid> = view
        .favorite_room_ids
        .iter()
        .copied()
        .filter(|id| {
            view.chat_rooms
                .iter()
                .any(|(r, _)| r.id == *id && is_chat_list_room(r))
        })
        .collect();
    if !bumped_slots.is_empty() {
        push_row(effect_section_header("bumped"), None, false);
        for slot in bumped_slots.iter().copied() {
            push_slot(slot, &mut push_row);
        }
        push_row(blank(), None, false);
    }

    if !favorite_ids.is_empty() {
        push_row(section_header(RoomSection::Favorites), None, false);
        for slot in favorite_slots {
            push_slot(slot, &mut push_row);
        }
        push_row(blank(), None, false);
    }

    let core_order = ["lounge", "announcements", "suggestions", "bugs"];
    let core_collapsed = collapsed_set.contains(&RoomSection::Core);
    push_row(section_header(RoomSection::Core), None, false);
    if !core_collapsed {
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
        push_slot(RoomSlot::Voice, &mut push_row);
        push_slot(RoomSlot::News, &mut push_row);
        if view.feeds_available {
            push_slot(RoomSlot::Feeds, &mut push_row);
        }
    }

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
        push_row(section_header(RoomSection::Channels), None, false);
        if !collapsed_set.contains(&RoomSection::Channels) {
            for (room, _) in channels {
                push_slot(RoomSlot::Room(room.id), &mut push_row);
            }
        }
    }

    let mut dms: Vec<&(ChatRoom, Vec<ChatMessage>)> = view
        .chat_rooms
        .iter()
        .filter(|(r, _)| is_chat_list_room(r) && r.kind == "dm" && !favorite_ids.contains(&r.id))
        .collect();
    dms.sort_by(|(a_room, _), (b_room, _)| {
        compare_dm_rooms_for_nav(
            a_room,
            b_room,
            view.current_user_id,
            view.usernames,
            view.unread_counts,
            view.room_last_message_at,
        )
    });
    if !dms.is_empty() {
        push_row(blank(), None, false);
        push_row(section_header(RoomSection::Dms), None, false);
        if !collapsed_set.contains(&RoomSection::Dms) {
            for (room, _) in dms {
                push_slot(RoomSlot::Room(room.id), &mut push_row);
            }
        }
    }

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
        RoomSlot::BumpedJoin(room_id) => {
            let label = view
                .active_room_effects
                .get(&room_id)
                .and_then(|effects| effects.first())
                .and_then(|effect| effect.room_slug.as_deref())
                .map(|slug| format!("join #{slug}"))
                .unwrap_or_else(|| "join room".to_string());
            (label, 0)
        }
        RoomSlot::Feeds => ("rss".to_string(), view.feeds_unread_count),
        RoomSlot::News => ("news".to_string(), view.news_unread_count),
        RoomSlot::Notifications => ("mentions".to_string(), view.notifications_unread_count),
        RoomSlot::Voice => ("voice".to_string(), view.voice_participant_count as i64),
        RoomSlot::Discover => ("+ browse rooms".to_string(), 0),
        RoomSlot::Showcase => ("showcase".to_string(), view.showcase_unread_count),
        RoomSlot::Work => ("work".to_string(), view.work_unread_count),
    }
}

fn bumped_join_room_slots(
    active_room_effects: &HashMap<Uuid, Vec<ActiveChatRoomEffect>>,
) -> Vec<RoomSlot> {
    let mut rooms = active_room_effects
        .iter()
        .filter_map(|(room_id, effects)| {
            let first = effects.first()?;
            (has_room_effect(effects, "room_bump")
                && first.room_kind == "topic"
                && first.room_visibility == "public"
                && !first.room_permanent
                && first
                    .room_slug
                    .as_deref()
                    .is_some_and(|slug| !slug.is_empty()))
            .then_some((
                first.room_slug.clone().unwrap_or_default(),
                RoomSlot::BumpedJoin(*room_id),
            ))
        })
        .collect::<Vec<_>>();
    rooms.sort_by(|(a, _), (b, _)| a.cmp(b));
    rooms.into_iter().map(|(_, slot)| slot).collect()
}

fn room_slot_effects<'a>(
    view: &'a ChatRoomListView<'_>,
    slot: RoomSlot,
) -> &'a [ActiveChatRoomEffect] {
    match slot {
        RoomSlot::Room(room_id) => view
            .active_room_effects
            .get(&room_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        RoomSlot::BumpedJoin(_) => &[],
        _ => &[],
    }
}

fn has_room_effect(effects: &[ActiveChatRoomEffect], effect_kind: &str) -> bool {
    effects
        .iter()
        .any(|effect| effect.effect_kind == effect_kind)
}

fn room_effect_suffix(effects: &[ActiveChatRoomEffect]) -> String {
    if let Some(vibe) = effects
        .iter()
        .find(|effect| effect.effect_kind == "pinned_vibe")
        .and_then(|effect| effect.vibe.as_deref())
    {
        format!(" {vibe}")
    } else {
        String::new()
    }
}

fn room_display_label(
    room: &ChatRoom,
    usernames: &UsernameLookup<'_>,
    current_user_id: Uuid,
) -> String {
    if room.kind == "dm" {
        return dm_display_label(room, usernames, current_user_id);
    }
    let base_label = room
        .slug
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| room.kind.clone());
    if room.visibility == "private" {
        format!("🔒 {}", base_label)
    } else {
        base_label
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
        Line::from(vec![key("[ ]/z"), hint("     sort/fold")]),
        Line::from(vec![key("ctrl+/"), hint("    find room")]),
    ]
}

fn cozy_slot_selected(view: &ChatRoomListView<'_>, slot: RoomSlot) -> bool {
    is_selected_slot(
        slot,
        SelectedRoomSlotState {
            selected_room_id: view.selected_room_id,
            selected_bumped_join_room_id: view.selected_bumped_join_room_id,
            feeds_selected: view.feeds_selected,
            news_selected: view.news_selected,
            notifications_selected: view.notifications_selected,
            voice_selected: view.voice_selected,
            discover_selected: view.discover_selected,
            showcase_selected: view.showcase_selected,
            work_selected: view.work_selected,
        },
    )
}

fn dm_display_label(
    room: &ChatRoom,
    usernames: &UsernameLookup<'_>,
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
pub fn draw_chat_center(
    frame: &mut Frame,
    area: Rect,
    view: ChatRenderInput<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if view.chat_rooms.is_empty() {
        let empty = Paragraph::new("No chat rooms yet.")
            .style(Style::default().fg(theme::TEXT_DIM()))
            .centered();
        frame.render_widget(empty, area);
        return;
    }

    let selection_mode = chat_selection_mode(&view, area);
    let (messages_area, composer_area) =
        split_chat_and_composer(area, selection_mode.composer_height());

    draw_selected_content(frame, messages_area, composer_area, view, terminal_images);
}

fn draw_selected_content(
    frame: &mut Frame,
    messages_area: Rect,
    composer_area: Rect,
    view: ChatRenderInput<'_>,
    terminal_images: &mut TerminalImageFrame,
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
    } else if view.voice_selected {
        crate::app::voice::ui::draw_voice_room(frame, messages_area, &view.voice_view);
    } else if view.discover_selected {
        super::discover::ui::draw_discover_list(frame, messages_area, &view.discover_view);
    } else if view.showcase_selected {
        super::showcase::ui::draw_showcase_list(frame, messages_area, &view.showcase_view);
    } else if view.work_selected {
        super::work::ui::draw_work_list(frame, messages_area, &view.work_view);
    } else if news_selected {
        super::news::ui::draw_article_list(frame, messages_area, &view.news_view);
    } else {
        let selected_room = if view.selected_bumped_join_room_id.is_some() {
            None
        } else {
            selected_room_id
                .and_then(|id| view.chat_rooms.iter().find(|(room, _)| room.id == id))
                .filter(|(room, _)| is_chat_list_room(room))
                .or_else(|| {
                    view.chat_rooms
                        .iter()
                        .find(|(room, _)| is_chat_list_room(room))
                })
        };

        let mut chat_hits: Option<Vec<ChatRowHit>> = None;
        let (poll_area, message_render_area) =
            split_poll_and_messages(messages_area, view.active_poll);
        if let Some(poll) = view.active_poll
            && let Some(poll_area) = poll_area
        {
            draw_poll_strip(frame, poll_area, poll);
        }
        let mut selected_room_effects: &[ActiveChatRoomEffect] = &[];
        let message_lines: Vec<Line> = if let Some((room, messages)) = selected_room {
            let active_effects = view
                .active_room_effects
                .get(&room.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            selected_room_effects = active_effects;
            let height = message_render_area.height.max(1) as usize;
            let width = message_render_area.width.max(1) as usize;

            ensure_chat_rows_cache(
                view.rows_cache,
                messages.iter().collect(),
                width,
                ChatRowsContext {
                    current_user_id,
                    afk_user_ids: view.afk_user_ids,
                    show_flag_fallback: view.show_flag_fallback,
                    usernames: view.usernames,
                    countries: view.countries,
                    friend_user_ids: view.friend_user_ids,
                    bonsai_glyphs: view.bonsai_glyphs,
                    chat_badges: view.chat_badges,
                    profile_award_badges: view.profile_award_badges,
                    bot_username_color_active: view.bot_username_color_active,
                    message_reactions: view.message_reactions,
                    inline_images: view.inline_images,
                },
            );
            let visible = visible_chat_rows(
                view.rows_cache,
                view.selected_message_id,
                view.highlighted_message_id,
                height,
            );
            chat_hits = Some(visible.hits);

            if visible.lines.is_empty() {
                vec![Line::from(Span::styled(
                    "No messages yet",
                    Style::default().fg(theme::TEXT_DIM()),
                ))]
            } else {
                visible.lines
            }
        } else if let Some(room_id) = view.selected_bumped_join_room_id {
            let label = view
                .active_room_effects
                .get(&room_id)
                .and_then(|effects| effects.first())
                .and_then(|effect| effect.room_slug.as_deref())
                .map(|slug| format!("join #{slug}"))
                .unwrap_or_else(|| "join room".to_string());
            vec![Line::from(Span::styled(
                label,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ))]
        } else {
            vec![Line::from(Span::styled(
                "Select a room.",
                Style::default().fg(theme::TEXT_DIM()),
            ))]
        };

        let messages_paragraph = Paragraph::new(message_lines);
        frame.render_widget(messages_paragraph, message_render_area);
        draw_room_page_effects(frame, message_render_area, selected_room_effects);
        if let (Some(slot), Some(hits)) = (view.chat_hit_slot, chat_hits)
            && view.overlay.is_none()
            && view.image_modal.is_none()
        {
            slot.set(Some(ChatHitLayout {
                content: message_render_area,
                rows: hits,
            }));
        }
        if let Some(overlay) = view.overlay {
            draw_overlay(frame, message_render_area, overlay);
        }
        if let Some(image_modal) = view.image_modal {
            draw_image_modal(frame, message_render_area, image_modal, terminal_images);
        }
    }

    if feeds_selected {
        if view.feeds_processing {
            let hint_block = Block::default()
                .title(" Processing URL... ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::AMBER()));
            let hint_text = Paragraph::new(Line::from(Span::styled(
                " Sharing RSS entry to news · Esc cancel",
                Style::default().fg(theme::TEXT_DIM()),
            )))
            .block(hint_block);
            frame.render_widget(hint_text, composer_area);
        } else {
            let hint_block = Block::default()
                .title(" RSS ")
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
    } else if view.voice_selected {
        crate::app::voice::ui::draw_voice_controls(frame, composer_area, &view.voice_view);
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
                selected_image_message: view.selected_image_message,
                selected_news_message: view.selected_news_message,
                reaction_picker_active: view.reaction_picker_active,
                reply_author: view.reply_author,
                is_editing: view.is_editing,
                mention_active: view.mention_active,
                mention_matches: view.mention_matches,
                mention_selected: view.mention_selected,
                keep_composer_focused: view.keep_composer_focused,
            },
        );
        if let Some(slot) = view.composer_rect_slot {
            slot.set(Some(composer_area));
        }
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use late_core::models::chat_room::ChatRoom;
    use std::{
        collections::{HashMap, HashSet},
        sync::OnceLock,
    };

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
    fn author_badge_suffix_keeps_badges_compact() {
        assert_eq!(
            format_author_badge_suffix(&["mod", "dev"], None, None),
            " mod dev"
        );
        assert_eq!(
            format_author_badge_suffix(&["mod"], Some("🐱"), Some("bonsai")),
            " mod bonsai 🐱"
        );
        assert_eq!(format_author_badge_suffix(&[], Some("🐱"), None), " 🐱");
        assert_eq!(
            format_author_badge_suffix(&[], None, Some("bonsai")),
            " bonsai"
        );
        assert_eq!(format_author_badge_suffix(&[], None, None), "");
    }

    #[test]
    fn chat_composer_layout_keeps_one_blank_row_gap() {
        let area = Rect::new(0, 0, 80, 20);
        let (messages_area, composer_area) = split_chat_and_composer(area, 3);

        assert_eq!(
            composer_area.y,
            messages_area.y + messages_area.height + CHAT_COMPOSER_GAP_HEIGHT
        );
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
        let chat_badges = HashMap::new();
        let friend_user_ids = HashSet::new();
        let afk_user_ids = HashSet::new();
        let message_reactions = HashMap::new();
        let inline_images = HashMap::new();
        let profile_award_badges = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);

        let messages = vec![&message];
        let ctx = ChatRowsContext {
            current_user_id: user_id,
            afk_user_ids: &afk_user_ids,
            show_flag_fallback: false,
            usernames: &username_lookup,
            countries: &countries,
            friend_user_ids: &friend_user_ids,
            bonsai_glyphs: &bonsai_glyphs,
            chat_badges: &chat_badges,
            profile_award_badges: &profile_award_badges,
            bot_username_color_active: false,
            message_reactions: &message_reactions,
            inline_images: &inline_images,
        };

        theme::set_current_by_id("late");
        let late_fingerprint = chat_rows_fingerprint(&messages, &ctx, 80);
        theme::set_current_by_id("contrast");
        let contrast_fingerprint = chat_rows_fingerprint(&messages, &ctx, 80);

        assert_ne!(late_fingerprint, contrast_fingerprint);
    }

    #[test]
    fn chat_rows_fingerprint_changes_when_author_goes_afk() {
        let room_id = Uuid::from_u128(1);
        let current_user_id = Uuid::from_u128(2);
        let author_id = Uuid::from_u128(3);
        let message = ChatMessage {
            id: Uuid::from_u128(4),
            created: Utc::now(),
            updated: Utc::now(),
            pinned: false,
            reply_to_message_id: None,
            room_id,
            user_id: author_id,
            body: "hello".to_string(),
        };
        let usernames = HashMap::from([
            (current_user_id, "alice".to_string()),
            (author_id, "bob".to_string()),
        ]);
        let countries = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let friend_user_ids = HashSet::new();
        let message_reactions = HashMap::new();
        let inline_images = HashMap::new();
        let profile_award_badges = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let messages = vec![&message];
        let active_afk_user_ids = HashSet::from([author_id]);
        let inactive_afk_user_ids = HashSet::new();

        let active_ctx = ChatRowsContext {
            current_user_id,
            afk_user_ids: &active_afk_user_ids,
            show_flag_fallback: false,
            usernames: &username_lookup,
            countries: &countries,
            friend_user_ids: &friend_user_ids,
            bonsai_glyphs: &bonsai_glyphs,
            chat_badges: &chat_badges,
            profile_award_badges: &profile_award_badges,
            bot_username_color_active: false,
            message_reactions: &message_reactions,
            inline_images: &inline_images,
        };
        let inactive_ctx = ChatRowsContext {
            current_user_id,
            afk_user_ids: &inactive_afk_user_ids,
            show_flag_fallback: false,
            usernames: &username_lookup,
            countries: &countries,
            friend_user_ids: &friend_user_ids,
            bonsai_glyphs: &bonsai_glyphs,
            chat_badges: &chat_badges,
            profile_award_badges: &profile_award_badges,
            bot_username_color_active: false,
            message_reactions: &message_reactions,
            inline_images: &inline_images,
        };

        assert_ne!(
            chat_rows_fingerprint(&messages, &active_ctx, 80),
            chat_rows_fingerprint(&messages, &inactive_ctx, 80)
        );
    }

    fn composer_view<'a>(textarea: &'a TextArea<'static>) -> ComposerBlockView<'a> {
        ComposerBlockView {
            composer: textarea,
            composing: true,
            selected_message: false,
            selected_image_message: false,
            selected_news_message: false,
            reaction_picker_active: false,
            reply_author: None,
            is_editing: false,
            mention_active: false,
            mention_matches: &[],
            mention_selected: 0,
            keep_composer_focused: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn chat_view<'a>(
        rows_cache: &'a mut ChatRowsCache,
        rooms: &'a [(ChatRoom, Vec<ChatMessage>)],
        selected_room_id: Option<Uuid>,
        usernames: &'a UsernameLookup<'a>,
        countries: &'a HashMap<Uuid, String>,
        message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
        unread_counts: &'a HashMap<Uuid, i64>,
        bonsai_glyphs: &'a HashMap<Uuid, String>,
        chat_badges: &'a HashMap<Uuid, String>,
        profile_award_badges: &'a HashMap<Uuid, String>,
        composer: &'a TextArea<'static>,
        news_composer: &'a TextArea<'static>,
    ) -> ChatRenderInput<'a> {
        static INLINE_IMAGES: OnceLock<HashMap<Uuid, InlineImagePreview>> = OnceLock::new();
        static FRIEND_USER_IDS: OnceLock<HashSet<Uuid>> = OnceLock::new();
        static AFK_USER_IDS: OnceLock<HashSet<Uuid>> = OnceLock::new();
        static VOICE_SNAPSHOT: OnceLock<crate::app::voice::svc::VoiceSnapshot> = OnceLock::new();
        static COLLAPSED_SECTIONS: OnceLock<HashSet<RoomSection>> = OnceLock::new();
        static ACTIVE_ROOM_EFFECTS: OnceLock<HashMap<Uuid, Vec<ActiveChatRoomEffect>>> =
            OnceLock::new();
        static ROOM_LAST_MESSAGE_AT: OnceLock<HashMap<Uuid, Option<DateTime<Utc>>>> =
            OnceLock::new();

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
            image_modal: None,
            usernames,
            countries,
            friend_user_ids: FRIEND_USER_IDS.get_or_init(HashSet::new),
            message_reactions,
            inline_images: INLINE_IMAGES.get_or_init(HashMap::new),
            unread_counts,
            room_last_message_at: ROOM_LAST_MESSAGE_AT.get_or_init(HashMap::new),
            favorite_room_ids: &[],
            active_room_effects: ACTIVE_ROOM_EFFECTS.get_or_init(HashMap::new),
            active_poll: None,
            collapsed_sections: COLLAPSED_SECTIONS.get_or_init(HashSet::new),
            selected_room_id,
            selected_bumped_join_room_id: None,
            room_jump_active: false,
            room_section_prefix_armed: false,
            selected_message_id: None,
            selected_image_message: false,
            selected_news_message: false,
            reaction_picker_active: false,
            highlighted_message_id: None,
            composer,
            composing: false,
            current_user_id: Uuid::nil(),
            afk_user_ids: AFK_USER_IDS.get_or_init(HashSet::new),
            show_flag_fallback: false,
            cursor_visible: false,
            mention_matches: &[],
            mention_selected: 0,
            mention_active: false,
            reply_author: None,
            is_editing: false,
            bonsai_glyphs,
            chat_badges,
            profile_award_badges,
            bot_username_color_active: false,
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
            voice_selected: false,
            voice_participant_count: 0,
            voice_view: crate::app::voice::ui::VoiceRoomView {
                snapshot: VOICE_SNAPSHOT.get_or_init(Default::default),
                current_user_id: Uuid::nil(),
                paired_cli_supports_voice: false,
                browser_listen_url: "http://localhost:3000/voice",
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
            keep_composer_focused: false,
            composer_rect_slot: None,
            chat_hit_slot: None,
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
    fn composer_title_with_keep_composer_focused_drops_alt_s_copy() {
        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.keep_composer_focused = true;
        let full = composer_title(&view, 100);
        assert!(
            full.contains("send & stay"),
            "expected 'send & stay' copy, got {full:?}"
        );
        assert!(
            !full.contains("Alt+S"),
            "expected Alt+S to be removed, got {full:?}"
        );

        view.reply_author = Some("alice");
        let reply = composer_title(&view, 100);
        assert!(
            reply.contains("send & stay"),
            "expected reply copy to mention 'send & stay', got {reply:?}"
        );
        assert!(!reply.contains("Alt+S"));

        view.reply_author = None;
        view.is_editing = true;
        let edit = composer_title(&view, 100);
        assert!(
            edit.contains("save & stay"),
            "expected edit copy to mention 'save & stay', got {edit:?}"
        );
        assert!(!edit.contains("Alt+S"));
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

        let visible = visible_chat_rows(&cache, Some(message_id), Some(message_id), 4);
        assert_eq!(
            visible.lines.len(),
            visible.hits.len(),
            "visible_chat_rows must return lines and hits of identical length"
        );
        assert!(
            visible
                .lines
                .iter()
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
        let lines = reaction_picker_placeholder_lines(Style::default(), usize::MAX);
        assert_eq!(lines.len(), 1);

        let rendered: String = lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(
            rendered,
            "1 👍  2 🧡  3 😂  4 👀  5 🔥  6 🙌  7 🚀  8 🤔  9 💩  0 👋  f list"
        );
    }

    #[test]
    fn reaction_picker_placeholder_wraps_at_narrow_width() {
        let lines = reaction_picker_placeholder_lines(Style::default(), 48);
        assert_eq!(lines.len(), 2);
        let rendered: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();

        assert_eq!(
            rendered,
            vec![
                "1 👍  2 🧡  3 😂  4 👀  5 🔥  6 🙌  7 🚀  8 🤔",
                "9 💩  0 👋  f list",
            ]
        );
    }

    #[test]
    fn chat_composer_placeholder_counts_wrapped_reaction_picker_lines() {
        let ta = TextArea::default();
        let lines = chat_composer_placeholder_lines(&ta, false, true, 48);
        assert_eq!(lines, 2);
    }

    #[test]
    fn reaction_picker_placeholder_keeps_zero_choice_at_mid_width() {
        let lines = reaction_picker_placeholder_lines(Style::default(), 50);
        let rendered: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();

        assert!(
            rendered.contains("0 👋"),
            "zero reaction choice missing from {rendered:?}",
        );
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
            row_1.contains("1 👍   2 🧡"),
            "reaction choices should preserve two separator spaces plus wide emoji padding: {row_1:?}",
        );
        assert!(
            row_1.contains("8 🤔"),
            "extended reaction choices missing from {row_1:?}",
        );
        assert!(
            row_1.contains("9 💩"),
            "ninth reaction choice missing from {row_1:?}",
        );
        assert!(
            row_1.contains("0 👋"),
            "zero reaction choice missing from {row_1:?}",
        );
        assert!(
            row_1.contains("f list"),
            "reaction owner hint missing from {row_1:?}",
        );
        assert!(
            !row_1.contains("0 👋  f list"),
            "reaction owner hint should not collapse below two separator spaces plus wide emoji padding: {row_1:?}",
        );
        assert!(
            row_1.contains("0 👋   f list"),
            "reaction owner hint should preserve two separator spaces plus wide emoji padding: {row_1:?}",
        );
    }

    #[test]
    fn empty_composer_placeholder_is_dim_while_composing() {
        use ratatui::{Terminal, backend::TestBackend};

        let ta = TextArea::default();
        let view = composer_view(&ta);
        let placeholder = empty_composer_placeholder(&view, 20);
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

        let expected =
            "Type a message · j/k select · Ctrl+] icon picker · or just ask @bot about anything";
        let width = expected.chars().count() as u16;
        let placeholder = empty_composer_placeholder(&view, width as usize);
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

        let expected =
            "f react · r reply · e edit · d delete · p profile · c copy · Enter view/copy link";
        let width = expected.chars().count() as u16;
        let placeholder = empty_composer_placeholder(&view, width as usize);
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
    fn empty_composer_placeholder_contextualizes_selected_image_message() {
        use ratatui::{Terminal, backend::TestBackend};

        let ta = TextArea::default();
        let mut view = composer_view(&ta);
        view.composing = false;
        view.selected_message = true;
        view.selected_image_message = true;

        let expected =
            "f react · r reply · e edit · d delete · p profile · c copy · Enter view image";
        let width = expected.chars().count() as u16;
        let placeholder = empty_composer_placeholder(&view, width as usize);
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
    fn room_list_rows_display_lounge() {
        let lounge = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("lounge".to_string()),
            permanent: true,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rooms = vec![(lounge.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let composer = TextArea::default();
        let profile_award_badges = HashMap::new();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            Some(lounge.id),
            &username_lookup,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &chat_badges,
            &composer,
            &profile_award_badges,
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
    }

    #[test]
    fn room_list_rows_keep_directory_surfaces_out_of_home() {
        let rooms = Vec::new();
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let composer = TextArea::default();
        let profile_award_badges = HashMap::new();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            None,
            &username_lookup,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &chat_badges,
            &composer,
            &profile_award_badges,
            &news_composer,
        );

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));
        let hit_slots: Vec<_> = room_rows.hit_slots.into_iter().flatten().collect();

        assert_eq!(
            hit_slots,
            vec![
                RoomSlot::Notifications,
                RoomSlot::Voice,
                RoomSlot::News,
                RoomSlot::Discover,
            ]
        );
    }

    #[test]
    fn cozy_room_rail_places_voice_news_and_feeds_below_mentions_with_jump_keys() {
        let lounge = ChatRoom {
            id: Uuid::from_u128(1),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("lounge".to_string()),
            permanent: true,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rust = ChatRoom {
            id: Uuid::from_u128(2),
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
        let rooms = vec![(lounge.clone(), Vec::new()), (rust.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let composer = TextArea::default();
        let profile_award_badges = HashMap::new();
        let news_composer = TextArea::default();
        let mut view = chat_view(
            &mut rows_cache,
            &rooms,
            None,
            &username_lookup,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &chat_badges,
            &composer,
            &profile_award_badges,
            &news_composer,
        );
        view.feeds_view.has_feeds = true;
        view.room_jump_active = true;

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_cozy_room_rail_rows(&room_list_view, 40);
        let keyed_slots: Vec<_> = room_rows
            .lines
            .iter()
            .zip(room_rows.hit_slots.iter())
            .filter_map(|(line, slot)| slot.map(|slot| (slot, line_text(line))))
            .collect();

        assert_eq!(
            &keyed_slots[..6],
            &[
                (RoomSlot::Room(lounge.id), "a lounge".to_string()),
                (RoomSlot::Notifications, "s mentions".to_string()),
                (RoomSlot::Voice, "d voice".to_string()),
                (RoomSlot::News, "f news".to_string()),
                (RoomSlot::Feeds, "g rss".to_string()),
                (RoomSlot::Room(rust.id), "h rust".to_string()),
            ]
        );
    }

    #[test]
    fn cozy_room_rail_shows_section_keys_when_fold_prefix_is_armed() {
        let lounge = ChatRoom {
            id: Uuid::from_u128(1),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("lounge".to_string()),
            permanent: true,
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        };
        let rust = ChatRoom {
            id: Uuid::from_u128(2),
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
        let dm = ChatRoom {
            id: Uuid::from_u128(3),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "dm".to_string(),
            visibility: "private".to_string(),
            auto_join: false,
            slug: None,
            permanent: false,
            language_code: None,
            dm_user_a: Some(Uuid::nil()),
            dm_user_b: Some(Uuid::from_u128(4)),
        };
        let rooms = vec![
            (lounge.clone(), Vec::new()),
            (rust.clone(), Vec::new()),
            (dm, Vec::new()),
        ];
        let favorite_room_ids = vec![lounge.id];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let composer = TextArea::default();
        let profile_award_badges = HashMap::new();
        let news_composer = TextArea::default();
        let mut view = chat_view(
            &mut rows_cache,
            &rooms,
            None,
            &username_lookup,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &chat_badges,
            &composer,
            &profile_award_badges,
            &news_composer,
        );
        view.favorite_room_ids = &favorite_room_ids;
        view.room_section_prefix_armed = true;

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_cozy_room_rail_rows(&room_list_view, 40);
        let rendered = room_rows.lines.iter().map(line_text).collect::<Vec<_>>();

        for expected in [
            "[f] - favorites",
            "[o] - core",
            "[c] - channels",
            "[d] - dms",
        ] {
            assert!(
                rendered.iter().any(|line| line == expected),
                "expected {expected:?} in {rendered:?}"
            );
        }
    }

    #[test]
    fn room_section_header_parser_ignores_fold_key_hints() {
        assert_eq!(strip_room_section_header_prefix("[o] - core"), "core");
        assert_eq!(strip_room_section_header_prefix("- [o] core"), "core");
        assert_eq!(strip_room_section_header_prefix("+ dms"), "dms");
    }

    #[test]
    fn room_list_rows_skip_game_rooms() {
        let lounge = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("lounge".to_string()),
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
        let rooms = vec![(lounge.clone(), Vec::new()), (game.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let composer = TextArea::default();
        let profile_award_badges = HashMap::new();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            Some(lounge.id),
            &username_lookup,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &chat_badges,
            &composer,
            &profile_award_badges,
            &news_composer,
        );

        let room_list_view = room_list_view_from_render_input(&view);
        let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));

        assert!(!room_rows.hit_slots.contains(&Some(RoomSlot::Room(game.id))));
    }

    #[test]
    fn room_list_hit_test_maps_public_room_row_to_room_slot() {
        let lounge = ChatRoom {
            id: Uuid::now_v7(),
            created: Utc::now(),
            updated: Utc::now(),
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            slug: Some("lounge".to_string()),
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
        let rooms = vec![(lounge.clone(), Vec::new()), (rust.clone(), Vec::new())];
        let mut rows_cache = ChatRowsCache::default();
        let usernames = HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);
        let countries = HashMap::new();
        let message_reactions = HashMap::new();
        let unread_counts = HashMap::new();
        let bonsai_glyphs = HashMap::new();
        let chat_badges = HashMap::new();
        let composer = TextArea::default();
        let profile_award_badges = HashMap::new();
        let news_composer = TextArea::default();
        let view = chat_view(
            &mut rows_cache,
            &rooms,
            Some(lounge.id),
            &username_lookup,
            &countries,
            &message_reactions,
            &unread_counts,
            &bonsai_glyphs,
            &chat_badges,
            &composer,
            &profile_award_badges,
            &news_composer,
        );

        let area = Rect::new(1, 1, 74, 30);
        let rooms_area = room_list_area(area, chat_selection_mode(&view, area));
        let room_list_view = room_list_view_from_render_input(&view);
        let inner = room_rail_inner_area(rooms_area);
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

    // ── Mouse hit-test (author header segments) ──────────────────

    #[test]
    fn header_segments_bare_username_only() {
        let (prefix, segs) =
            build_author_prefix_and_segments(false, "alice", &[], None, None, None, None);
        assert_eq!(prefix, "alice");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].target, HeaderTarget::Profile);
        // column 0 is pad, prefix begins at 1.
        assert_eq!(segs[0].start_col, 1);
        assert_eq!(segs[0].end_col, 1 + 5); // "alice"
    }

    #[test]
    fn build_author_prefix_matches_legacy_formatter_across_combinations() {
        // The legacy `format_author_badge_suffix` is kept under #[cfg(test)]
        // precisely to pin this byte-identity invariant: whatever pieces
        // the production builder emits must concatenate to exactly the
        // same prefix string the legacy `format!(...)` block produced.
        let assert_matches =
            |is_friend: bool, author: &str, sp: &[&str], cb: Option<&str>, bg: Option<&str>| {
                let suffix = format_author_badge_suffix(sp, cb, bg);
                let legacy = if is_friend {
                    format!("{FRIEND_BADGE} {author}{suffix}")
                } else {
                    format!("{author}{suffix}")
                };
                let (built, _) =
                    build_author_prefix_and_segments(is_friend, author, sp, cb, bg, None, None);
                assert_eq!(
                    built, legacy,
                    "case {is_friend} {author:?} {sp:?} {cb:?} {bg:?}"
                );
            };
        assert_matches(false, "alice", &[], None, None);
        assert_matches(true, "alice", &[], None, None);
        assert_matches(false, "alice", &["mod", "dev"], None, None);
        assert_matches(false, "alice", &[], Some("🐱"), None);
        assert_matches(false, "alice", &[], None, Some("🌱"));
        assert_matches(false, "alice", &[], Some("🐱"), Some("🌱"));
        assert_matches(true, "alice", &["mod"], Some("🐱"), Some("🌱"));
    }

    #[test]
    fn header_segments_full_label_orders_special_bonsai_store() {
        // alice ★ + author + " mod bonsai 🐱"
        // (special "mod", bonsai "bonsai", store "🐱")
        let (prefix, segs) = build_author_prefix_and_segments(
            true,
            "alice",
            &["mod"],
            Some("🐱"),
            Some("bonsai"),
            None,
            None,
        );
        // Sanity: the legacy formatter produces the same suffix shape.
        let legacy = format!(
            "{FRIEND_BADGE} alice{}",
            format_author_badge_suffix(&["mod"], Some("🐱"), Some("bonsai"))
        );
        assert_eq!(prefix, legacy);

        // Profile-classified segments: friend badge, author, "mod", "bonsai".
        let profiles: Vec<_> = segs
            .iter()
            .filter(|s| s.target == HeaderTarget::Profile)
            .collect();
        assert_eq!(profiles.len(), 4);

        // Exactly one StoreBadge segment, sitting after "bonsai".
        let stores: Vec<_> = segs
            .iter()
            .filter(|s| s.target == HeaderTarget::StoreBadge)
            .collect();
        assert_eq!(stores.len(), 1);
        let store = stores[0];
        // The store segment's start col must equal the prefix-relative
        // offset of the chat-badge emoji (column 0 is the pad cell).
        let expected_store_offset = 1
            + UnicodeWidthStr::width(FRIEND_BADGE) as u16
            + 1
            + UnicodeWidthStr::width("alice") as u16
            + 1
            + UnicodeWidthStr::width("mod") as u16
            + UnicodeWidthStr::width(AUTHOR_BADGE_SEPARATOR) as u16
            + UnicodeWidthStr::width("bonsai") as u16
            + UnicodeWidthStr::width(AUTHOR_BADGE_SEPARATOR) as u16;
        assert_eq!(store.start_col, expected_store_offset);
        assert_eq!(
            store.end_col,
            expected_store_offset + UnicodeWidthStr::width("🐱") as u16
        );
    }

    #[test]
    fn header_segments_skip_empty_badges() {
        // Empty special/store/bonsai entries should be dropped — they
        // would render as zero-width but a hit-test range of (col, col)
        // would never match anything, so don't emit them.
        let (_prefix, segs) = build_author_prefix_and_segments(
            false,
            "alice",
            &["", "mod"],
            Some(""),
            Some(""),
            None,
            None,
        );
        // 1 author + 1 special "mod" = 2 segments. No store, no bonsai.
        assert_eq!(segs.len(), 2);
        assert!(segs.iter().all(|s| s.target == HeaderTarget::Profile));
        assert!(segs.iter().any(|s| s.end_col - s.start_col == 3)); // "mod"
    }

    #[test]
    fn header_segments_bonsai_then_store_without_specials() {
        let (_prefix, segs) =
            build_author_prefix_and_segments(false, "bob", &[], Some("🐱"), Some("🌱"), None, None);
        // author (Profile), bonsai (Profile), store (StoreBadge).
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].target, HeaderTarget::Profile);
        assert_eq!(segs[1].target, HeaderTarget::Profile);
        assert_eq!(segs[2].target, HeaderTarget::StoreBadge);
        // Bonsai and store are separated by `AUTHOR_BADGE_SEPARATOR`, so their
        // ranges must not abut.
        assert!(segs[2].start_col > segs[1].end_col);
    }

    #[test]
    fn header_segments_split_chat_flag_from_regular_badge() {
        let chat_badges = [
            (HeaderTarget::StoreBadge, "🐱"),
            (HeaderTarget::StoreFlag, "US"),
        ];
        let (prefix, segs) = build_author_prefix_and_segments_with_chat_badges(
            false,
            "bob",
            &[],
            &chat_badges,
            None,
            None,
            None,
        );
        assert_eq!(prefix, "bob 🐱 US");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].target, HeaderTarget::Profile);
        assert_eq!(segs[1].target, HeaderTarget::StoreBadge);
        assert_eq!(segs[2].target, HeaderTarget::StoreFlag);
    }

    #[test]
    fn header_prefix_orders_all_badge_classes() {
        let chat_badges = [
            (HeaderTarget::StoreBadge, "badge"),
            (HeaderTarget::StoreFlag, "flag"),
        ];
        let (prefix, _segs) = build_author_prefix_and_segments_with_chat_badges(
            false,
            "alice",
            &["mod", "developer", "artist"],
            &chat_badges,
            Some("bonsai"),
            Some("AW1"),
            Some("brb"),
        );

        assert_eq!(
            prefix,
            "alice mod developer artist bonsai AW1 badge flag brb"
        );
    }

    #[test]
    fn chat_badge_display_parts_put_store_badge_before_flag() {
        let parts = chat_badge_display_parts("🇺🇸 🐱", false);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, HeaderTarget::StoreBadge);
        assert_eq!(parts[0].1, "🐱");
        assert_eq!(parts[1].0, HeaderTarget::StoreFlag);
        assert_eq!(parts[1].1, "🇺🇸");

        let fallback_parts = chat_badge_display_parts("🇺🇸 🐱", true);
        assert_eq!(fallback_parts[0].0, HeaderTarget::StoreBadge);
        assert_eq!(fallback_parts[0].1, "🐱");
        assert_eq!(fallback_parts[1].0, HeaderTarget::StoreFlag);
        assert_eq!(fallback_parts[1].1, "US");
    }

    #[test]
    fn visible_chat_rows_pads_top_with_none_hits() {
        // Three rows of content into a viewport of height 5 ⇒ two
        // leading padding rows whose hit kind must be `None`.
        let message_id = Uuid::now_v7();
        let cache = ChatRowsCache {
            all_rows: vec![
                Line::from(Span::raw("alice")),
                Line::from(Span::raw("hello")),
                Line::from(Span::raw("world")),
            ],
            row_message: vec![Some(message_id), Some(message_id), Some(message_id)],
            row_kind: vec![RowKindLite::Header, RowKindLite::Body, RowKindLite::Body],
            header_segments: {
                let mut m = HashMap::new();
                m.insert(
                    message_id,
                    vec![HeaderSegment {
                        start_col: 1,
                        end_col: 6,
                        target: HeaderTarget::Profile,
                    }],
                );
                m
            },
            ..Default::default()
        };

        let visible = visible_chat_rows(&cache, None, None, 5);
        assert_eq!(visible.lines.len(), 5);
        assert_eq!(visible.hits.len(), 5);
        // Top two are padding.
        assert!(matches!(visible.hits[0].kind, ChatRowKind::None));
        assert!(visible.hits[0].message_id.is_none());
        assert!(matches!(visible.hits[1].kind, ChatRowKind::None));
        // Then header, body, body.
        assert!(matches!(visible.hits[2].kind, ChatRowKind::Header(_)));
        assert_eq!(visible.hits[2].message_id, Some(message_id));
        assert!(matches!(visible.hits[3].kind, ChatRowKind::Body));
        assert!(matches!(visible.hits[4].kind, ChatRowKind::Body));
    }
}
