use std::{cmp::Reverse, time::Duration};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::chat::ui::{DashboardChatView, draw_dashboard_chat_card},
    app::common::{
        primitives::{format_duration_mmss, genre_label},
        theme,
    },
    app::rooms::{
        blackjack::settings::BlackjackTableSettings,
        blackjack::state::BlackjackSnapshot,
        svc::{GameKind, RoomListItem, RoomsSnapshot},
    },
    app::vote::svc::{Genre, VoteCount},
    app::vote::ui::{VoteCardView, draw_vote_card},
};
use late_core::models::{
    article::ArticleFeedItem, chat_message::ChatMessage, leaderboard::DailyGame,
};

// `draw_dashboard` receives the content pane after the outer app border and the
// fixed 24-column sidebar are removed. A 77-column terminal yields 51 columns
// of dashboard content, which is where we want to hide voting and keep screen 1
// usable for chat.
const DASHBOARD_HIDE_VOTE_AT_WIDTH: u16 = 51;
// A 65-column terminal yields 39 columns of dashboard content, which is where
// we drop the top stream/music card entirely so chat can use the reclaimed
// vertical space.
const DASHBOARD_HIDE_STREAM_AT_WIDTH: u16 = 39;
// Below this many rows the fixed 5-row stream card plus chat card no longer
// fit cleanly, so we collapse to chat-only rather than render clipped blocks.
const DASHBOARD_MIN_FULL_HEIGHT: u16 = 16;
const BLACKJACK_GRID_MIN_WIDTH: u16 = 52;
const BLACKJACK_GRID_COLUMNS: usize = 3;
const BLACKJACK_GRID_TEXT_ROWS: u16 = 2;
const BLACKJACK_GRID_HEIGHT: u16 = BLACKJACK_GRID_TEXT_ROWS + 1; // + bottom rule
const BLACKJACK_GRID_HEIGHT_WITH_TOP: u16 = BLACKJACK_GRID_HEIGHT + 1; // + top rule
// Keep the top pinned-rule variant for roomy panes so tight layouts keep the
// pin in the bottom rule and preserve the old scan order.
const BLACKJACK_GRID_TOP_RULE_MIN_HEIGHT: u16 = 10;
pub(crate) const DASHBOARD_DAILY_CYCLE_SECONDS: u64 = 60;
/// 1 minute per wire headline. The wire is meant as a slow ambient feed —
/// you might glance at the dashboard every few minutes and see something new
/// without it churning into noise.
pub(crate) const WIRE_NEWS_CYCLE_SECONDS: u64 = 60;
pub(crate) const WIRE_NEWS_MAX_ITEMS: usize = 5;
const AUDIO_BUTTON_PREFIX: &str = "No audio? ";
const CLI_BUTTON_TEXT: &str = "[B] CLI";
const PAIR_BUTTON_TEXT: &str = "[P] web";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DashboardDailyStatus {
    pub game: DailyGame,
    pub completed_today: bool,
    pub launch_key: char,
    pub streak: u32,
}

pub struct DashboardRenderInput<'a> {
    pub now_playing: Option<&'a str>,
    pub vote_counts: &'a VoteCount,
    pub current_genre: Genre,
    pub next_switch_in: Duration,
    pub my_vote: Option<Genre>,
    pub show_header: bool,
    /// When `Some`, the user has 2+ favorites pinned and we render a
    /// quick-switch strip directly above the chat card. Each entry is
    /// `(room_id, label, is_active, unread_count)`. `None` hides the strip.
    pub favorites_strip: Option<&'a [(uuid::Uuid, String, bool, i64)]>,
    /// Pinned chat messages visible to this user; the newest pin is embedded
    /// into the dashboard box rule when the box row is visible.
    pub pinned_messages: &'a [ChatMessage],
    pub rooms_snapshot: &'a RoomsSnapshot,
    pub blackjack_snapshots: &'a std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    pub blackjack_prefix_armed: bool,
    pub daily_statuses: &'a [DashboardDailyStatus],
    /// Latest articles for the wire box, freshest first. Up to
    /// `WIRE_NEWS_MAX_ITEMS` are rotated; extras are ignored.
    pub wire_news_articles: &'a [ArticleFeedItem],
    pub dashboard_cycle_secs: u64,
    pub chat_view: DashboardChatView<'a>,
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    let stream_props = StreamCardProps {
        now_playing: view.now_playing.unwrap_or("Waiting for stream..."),
        current_genre: view.current_genre,
        leading_genre: view.vote_counts.winner_or(view.current_genre),
        next_switch_in: view.next_switch_in,
    };
    if !view.show_header
        || area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH
        || area.height < DASHBOARD_MIN_FULL_HEIGHT
    {
        draw_blackjack_and_chat_section(frame, area, view);
        return;
    }

    let sections = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);

    if area.width <= DASHBOARD_HIDE_VOTE_AT_WIDTH {
        draw_stream_card(frame, sections[0], &stream_props);
    } else {
        let top = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0]);
        draw_stream_card(frame, top[0], &stream_props);
        draw_vote_card(
            frame,
            top[1],
            &VoteCardView {
                vote_counts: view.vote_counts,
                my_vote: view.my_vote,
            },
        );
    }

    draw_blackjack_and_chat_section(frame, sections[1], view);
}

fn draw_blackjack_and_chat_section(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    let rooms = dashboard_blackjack_rooms(view.rooms_snapshot, view.blackjack_snapshots);
    let Some(grid_height) = blackjack_grid_height(area) else {
        draw_chat_section(frame, area, view.favorites_strip, view.chat_view);
        return;
    };

    // Two layouts ordered by available height:
    //   1. Roomy: pinned msg embedded in a TOP rule above the grid (own row,
    //      tied to columns by `┌┬┬┐` junctions)
    //   2. Standard: grid only, pinned msg falls into the BOTTOM rule
    let pinned_present = !view.pinned_messages.is_empty();
    let has_room_for_top_rule = area.height >= BLACKJACK_GRID_TOP_RULE_MIN_HEIGHT;

    if pinned_present && has_room_for_top_rule {
        let split = Layout::vertical([
            Constraint::Length(BLACKJACK_GRID_HEIGHT_WITH_TOP),
            Constraint::Fill(1),
        ])
        .split(area);
        draw_blackjack_grid(
            frame,
            split[0],
            BlackjackGridView {
                rooms: &rooms,
                snapshots: view.blackjack_snapshots,
                prefix_armed: view.blackjack_prefix_armed,
                top_rule_pinned: view.pinned_messages.first(),
                bottom_rule_pinned: None,
                daily_statuses: view.daily_statuses,
                wire_news_articles: view.wire_news_articles,
                cycle_secs: view.dashboard_cycle_secs,
            },
        );
        draw_chat_section(frame, split[1], view.favorites_strip, view.chat_view);
        return;
    }

    let split =
        Layout::vertical([Constraint::Length(grid_height), Constraint::Fill(1)]).split(area);
    draw_blackjack_grid(
        frame,
        split[0],
        BlackjackGridView {
            rooms: &rooms,
            snapshots: view.blackjack_snapshots,
            prefix_armed: view.blackjack_prefix_armed,
            top_rule_pinned: None,
            bottom_rule_pinned: view.pinned_messages.first(),
            daily_statuses: view.daily_statuses,
            wire_news_articles: view.wire_news_articles,
            cycle_secs: view.dashboard_cycle_secs,
        },
    );
    draw_chat_section(frame, split[1], view.favorites_strip, view.chat_view);
}

fn blackjack_grid_height(area: Rect) -> Option<u16> {
    if area.width < BLACKJACK_GRID_MIN_WIDTH || area.height < BLACKJACK_GRID_HEIGHT {
        return None;
    }

    Some(BLACKJACK_GRID_HEIGHT)
}

fn dashboard_blackjack_rooms<'a>(
    snapshot: &'a RoomsSnapshot,
    snapshots: &std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
) -> Vec<&'a RoomListItem> {
    let mut rooms: Vec<&RoomListItem> = snapshot
        .rooms
        .iter()
        .filter(|room| matches!(room.game_kind, GameKind::Blackjack))
        .collect();
    rooms.sort_by_key(|room| {
        let snapshot = snapshots.get(&room.id);
        let occupied = snapshot
            .map(|snap| {
                snap.seats
                    .iter()
                    .filter(|seat| seat.user_id.is_some())
                    .count()
            })
            .unwrap_or(0);
        (
            Reverse(occupied),
            Reverse(blackjack_phase_priority(snapshot)),
        )
    });
    rooms.truncate(BLACKJACK_GRID_COLUMNS);
    rooms
}

fn blackjack_phase_priority(snapshot: Option<&BlackjackSnapshot>) -> u8 {
    use crate::app::rooms::blackjack::state::Phase;
    match snapshot.map(|snap| snap.phase) {
        Some(Phase::PlayerTurn | Phase::DealerTurn) => 2,
        Some(Phase::Betting) => 1,
        _ => 0,
    }
}

struct BlackjackGridView<'a> {
    rooms: &'a [&'a RoomListItem],
    snapshots: &'a std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    prefix_armed: bool,
    top_rule_pinned: Option<&'a ChatMessage>,
    bottom_rule_pinned: Option<&'a ChatMessage>,
    daily_statuses: &'a [DashboardDailyStatus],
    wire_news_articles: &'a [ArticleFeedItem],
    cycle_secs: u64,
}

fn draw_blackjack_grid(frame: &mut Frame, area: Rect, view: BlackjackGridView<'_>) {
    let has_top_rule = view.top_rule_pinned.is_some();
    let required_height = if has_top_rule {
        BLACKJACK_GRID_HEIGHT_WITH_TOP
    } else {
        BLACKJACK_GRID_HEIGHT
    };
    if area.height < required_height {
        return;
    }

    let mut constraints: Vec<Constraint> = Vec::with_capacity(3);
    if has_top_rule {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(BLACKJACK_GRID_TEXT_ROWS));
    constraints.push(Constraint::Length(1));
    let chunks = Layout::vertical(constraints).split(area);
    let (top_rule_area, text_area, bottom_rule_area) = if has_top_rule {
        (Some(chunks[0]), chunks[1], chunks[2])
    } else {
        (None, chunks[0], chunks[1])
    };

    // 7-track horizontal layout: vert | col1 | vert | col2 | vert | col3 | vert
    let cols = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(text_area);

    let border_style = Style::default().fg(theme::BORDER_DIM());
    for &vert_idx in &[0usize, 2, 4, 6] {
        let lines = vec![Line::from("│"), Line::from("│")];
        frame.render_widget(Paragraph::new(lines).style(border_style), cols[vert_idx]);
    }

    let slot_areas = [cols[1], cols[3], cols[5]];
    let loading = view.rooms.is_empty();
    for (slot, area) in slot_areas
        .iter()
        .copied()
        .enumerate()
        .take(BLACKJACK_GRID_COLUMNS)
    {
        // 1-char left/right padding inside each column.
        let padded = if area.width >= 4 {
            Rect {
                x: area.x + 1,
                width: area.width - 2,
                ..area
            }
        } else {
            area
        };
        match slot {
            0 => draw_blackjack_slot(
                frame,
                padded,
                slot,
                view.rooms.first().copied(),
                view.snapshots,
                view.prefix_armed,
                loading,
            ),
            1 => draw_dailies_slot(
                frame,
                padded,
                view.daily_statuses,
                view.prefix_armed,
                view.cycle_secs,
            ),
            2 => draw_wire_slot(
                frame,
                padded,
                view.wire_news_articles,
                view.prefix_armed,
                view.cycle_secs,
            ),
            _ => {}
        }
    }

    if let Some(top_area) = top_rule_area
        && let Some(msg) = view.top_rule_pinned
    {
        draw_pin_strip(frame, top_area, msg, view.prefix_armed);
    }

    let bottom_junctions = [
        (cols[0].x.saturating_sub(bottom_rule_area.x) as usize, '└'),
        (cols[2].x.saturating_sub(bottom_rule_area.x) as usize, '┴'),
        (cols[4].x.saturating_sub(bottom_rule_area.x) as usize, '┴'),
        (cols[6].x.saturating_sub(bottom_rule_area.x) as usize, '┘'),
    ];
    draw_blackjack_rule(
        frame,
        bottom_rule_area,
        &bottom_junctions,
        view.bottom_rule_pinned,
        view.prefix_armed,
    );
}

/// Renders one horizontal row of the grid chrome (top OR bottom). Junction
/// chars are positioned at column splits; if `newest_pinned` is `Some`, the
/// message is centered between rule chars in the row, with junctions
/// preserved wherever the message doesn't cover them.
fn draw_blackjack_rule(
    frame: &mut Frame,
    area: Rect,
    junctions: &[(usize, char)],
    newest_pinned: Option<&ChatMessage>,
    prefix_armed: bool,
) {
    let total_w = area.width as usize;
    if total_w == 0 {
        return;
    }

    let make_rule_char = |i: usize| -> char {
        junctions
            .iter()
            .find_map(|(off, ch)| if *off == i { Some(*ch) } else { None })
            .unwrap_or('─')
    };

    let border_style = Style::default().fg(theme::BORDER_DIM());

    let render_plain_rule = |frame: &mut Frame| {
        let rule: String = (0..total_w).map(make_rule_char).collect();
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(rule, border_style))),
            area,
        );
    };

    let Some(msg) = newest_pinned else {
        render_plain_rule(frame);
        return;
    };

    let first_line = msg.body.split('\n').next().unwrap_or("").trim();
    if first_line.is_empty() {
        render_plain_rule(frame);
        return;
    }

    // Centered "  <body>  " between rule chars. Outer-pad spaces on each side
    // keep the message from butting against the rule; junctions are preserved
    // wherever the message doesn't cover them.
    let outer_pad = 2usize;
    let chrome_w = outer_pad * 2;
    let min_pad = 3usize;
    let min_body = 3usize;
    let key_tag = dashboard_key_tag(3, prefix_armed, true);
    let key_text = key_tag.content.as_ref();
    let key_w = UnicodeWidthStr::width(key_text);
    let key_gap = 1usize;
    let min_required = min_pad * 2 + chrome_w + min_body + key_gap + key_w;
    if total_w < min_required {
        render_plain_rule(frame);
        return;
    }

    let max_body = total_w - min_pad * 2 - chrome_w - key_gap - key_w;
    let body = truncate(first_line, max_body);
    let body_w = UnicodeWidthStr::width(body.as_str());
    let middle_w = chrome_w + body_w;
    let left_rule_w = (total_w - middle_w - key_gap - key_w) / 2;
    let right_rule_w = total_w - left_rule_w - middle_w - key_gap - key_w;

    let left: String = (0..left_rule_w).map(make_rule_char).collect();
    let right: String = (left_rule_w + middle_w..left_rule_w + middle_w + right_rule_w)
        .map(make_rule_char)
        .collect();
    let outer_space: String = " ".repeat(outer_pad);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, border_style),
            Span::raw(outer_space.clone()),
            Span::styled(
                body,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(outer_space),
            Span::styled(right, border_style),
            Span::raw(" ".repeat(key_gap)),
            key_tag,
        ])),
        area,
    );
}

/// Renders the pinned-message strip that floats above the grid as its own
/// detached element. Plain `─` rule chars (no column junctions) and an amber
/// tint mark it as a pin rather than box chrome.
fn draw_pin_strip(frame: &mut Frame, area: Rect, msg: &ChatMessage, prefix_armed: bool) {
    let total_w = area.width as usize;
    if total_w == 0 {
        return;
    }

    let rule_style = Style::default().fg(theme::AMBER_DIM());

    let render_plain_rule = |frame: &mut Frame| {
        let rule: String = "─".repeat(total_w);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(rule, rule_style))),
            area,
        );
    };

    let first_line = msg.body.split('\n').next().unwrap_or("").trim();
    if first_line.is_empty() {
        render_plain_rule(frame);
        return;
    }

    let outer_pad = 2usize;
    let chrome_w = outer_pad * 2;
    let min_pad = 3usize;
    let min_body = 3usize;
    let key_tag = dashboard_key_tag(3, prefix_armed, true);
    let key_text = key_tag.content.as_ref();
    let key_w = UnicodeWidthStr::width(key_text);
    let key_gap = 1usize;
    let min_required = min_pad * 2 + chrome_w + min_body + key_gap + key_w;
    if total_w < min_required {
        render_plain_rule(frame);
        return;
    }

    let max_body = total_w - min_pad * 2 - chrome_w - key_gap - key_w;
    let body = truncate(first_line, max_body);
    let body_w = UnicodeWidthStr::width(body.as_str());
    let middle_w = chrome_w + body_w;
    let left_rule_w = (total_w - middle_w - key_gap - key_w) / 2;
    let right_rule_w = total_w - left_rule_w - middle_w - key_gap - key_w;
    let outer_space: String = " ".repeat(outer_pad);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("─".repeat(left_rule_w), rule_style),
            Span::raw(outer_space.clone()),
            Span::styled(
                body,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(outer_space),
            Span::styled("─".repeat(right_rule_w), rule_style),
            Span::raw(" ".repeat(key_gap)),
            key_tag,
        ])),
        area,
    );
}

fn draw_dailies_slot(
    frame: &mut Frame,
    area: Rect,
    statuses: &[DashboardDailyStatus],
    prefix_armed: bool,
    cycle_secs: u64,
) {
    if area.width < 10 || area.height < BLACKJACK_GRID_TEXT_ROWS {
        return;
    }

    let key_tag = dashboard_key_tag(1, prefix_armed, true);
    let inner_width = area.width as usize;

    if statuses.is_empty() {
        let line1 = line_with_key(
            "dailies loading",
            key_tag,
            inner_width,
            Style::default().fg(theme::TEXT_FAINT()),
        );
        let line2 = Line::from(Span::styled(
            "streak 0 · [3] arcade",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        frame.render_widget(Paragraph::new(vec![line1, line2]), area);
        return;
    }

    let unfinished: Vec<DashboardDailyStatus> = statuses
        .iter()
        .copied()
        .filter(|status| !status.completed_today)
        .collect();

    let Some(status) = unfinished
        .get(((cycle_secs / DASHBOARD_DAILY_CYCLE_SECONDS) as usize) % unfinished.len().max(1))
        .copied()
    else {
        let streak = statuses
            .iter()
            .map(|status| status.streak)
            .max()
            .unwrap_or(0);
        let line1 = line_with_key(
            &format!("All dailies ✓ · streak {streak}"),
            key_tag,
            inner_width,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        );
        let line2 = Line::from(Span::styled(
            "today complete",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        frame.render_widget(Paragraph::new(vec![line1, line2]), area);
        return;
    };

    let line1 = line_with_key(
        &format!("{} · today", daily_game_name(status.game)),
        key_tag,
        inner_width,
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    );
    let line2 = Line::from(Span::styled(
        truncate(
            &format!("streak {} · [{}] play", status.streak, status.launch_key),
            inner_width,
        ),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    frame.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn draw_wire_slot(
    frame: &mut Frame,
    area: Rect,
    articles: &[ArticleFeedItem],
    prefix_armed: bool,
    cycle_secs: u64,
) {
    if area.width < 10 || area.height < BLACKJACK_GRID_TEXT_ROWS {
        return;
    }

    let inner_width = area.width as usize;
    let has_articles = wire_current_article(articles, cycle_secs).is_some();
    let key_tag = dashboard_key_tag(2, prefix_armed, has_articles);

    let label_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);

    let Some(item) = wire_current_article(articles, cycle_secs) else {
        let line1 = line_with_key("wire · news", key_tag, inner_width, label_style);
        let line2 = Line::from(Span::styled(
            "no headlines yet",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        frame.render_widget(Paragraph::new(vec![line1, line2]), area);
        return;
    };

    let total = articles.len().min(WIRE_NEWS_MAX_ITEMS);
    let idx = ((cycle_secs / WIRE_NEWS_CYCLE_SECONDS) as usize) % total.max(1);
    let label = format!("wire · news {}/{}", idx + 1, total);
    let line1 = line_with_key(&label, key_tag, inner_width, label_style);
    let line2 = Line::from(Span::styled(
        truncate(item.article.title.as_str(), inner_width),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(vec![line1, line2]), area);
}

pub(crate) fn wire_current_article(
    articles: &[ArticleFeedItem],
    cycle_secs: u64,
) -> Option<&ArticleFeedItem> {
    let pool = &articles[..articles.len().min(WIRE_NEWS_MAX_ITEMS)];
    if pool.is_empty() {
        return None;
    }
    let idx = ((cycle_secs / WIRE_NEWS_CYCLE_SECONDS) as usize) % pool.len();
    pool.get(idx)
}

fn dashboard_key_tag(slot: usize, prefix_armed: bool, enabled: bool) -> Span<'static> {
    let key_char = match slot {
        0 => '1',
        1 => '2',
        2 => '3',
        3 => '4',
        _ => '?',
    };
    let style = if prefix_armed {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else if enabled {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    Span::styled(format!("[b+{key_char}]"), style)
}

fn line_with_key(
    label: &str,
    key_tag: Span<'static>,
    inner_width: usize,
    label_style: Style,
) -> Line<'static> {
    let key_text = key_tag.content.as_ref();
    let key_w = UnicodeWidthStr::width(key_text);
    let label_budget = inner_width.saturating_sub(key_w + 1).max(1);
    let truncated = truncate(label, label_budget);
    let label_w = UnicodeWidthStr::width(truncated.as_str());
    let pad_w = inner_width.saturating_sub(label_w + key_w).max(1);
    Line::from(vec![
        Span::styled(truncated, label_style),
        Span::raw(" ".repeat(pad_w)),
        key_tag,
    ])
}

fn daily_game_name(game: DailyGame) -> &'static str {
    match game {
        DailyGame::Sudoku => "Sudoku",
        DailyGame::Nonogram => "Nonogram",
        DailyGame::Solitaire => "Solitaire",
        DailyGame::Minesweeper => "Minesweeper",
    }
}

fn draw_blackjack_slot(
    frame: &mut Frame,
    area: Rect,
    slot: usize,
    room: Option<&RoomListItem>,
    snapshots: &std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    prefix_armed: bool,
    loading: bool,
) {
    if area.width < 10 || area.height < BLACKJACK_GRID_TEXT_ROWS {
        return;
    }

    let key_char = match slot {
        0 => '1',
        1 => '2',
        2 => '3',
        _ => '?',
    };
    let key_style = if prefix_armed {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else if room.is_some() {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let key_tag = Span::styled(format!("[b+{key_char}]"), key_style);

    let inner_width = area.width as usize;

    let Some(room) = room else {
        let label = if loading { "loading…" } else { "open slot" };
        let line1 = line_with_key(
            label,
            key_tag,
            inner_width,
            Style::default().fg(theme::TEXT_FAINT()),
        );
        frame.render_widget(Paragraph::new(vec![line1, Line::from("")]), area);
        return;
    };

    let snapshot = snapshots.get(&room.id);
    let max_seats: usize = snapshot.map(|s| s.seats.len()).unwrap_or(4);
    let occupied: Option<usize> =
        snapshot.map(|s| s.seats.iter().filter(|seat| seat.user_id.is_some()).count());

    let mut seat_text = String::new();
    for i in 0..max_seats {
        let filled = occupied.map(|o| i < o).unwrap_or(false);
        seat_text.push(if filled { '●' } else { '○' });
    }
    let count_label = match occupied {
        Some(n) => format!("{}/{}", n, max_seats),
        None => format!("?/{}", max_seats),
    };

    // Line 1: "○○○○ 0/4 <name>                       [b+1]"
    // Seats / count / name flush left, key tag pushed to the slot's right edge.
    let seats_w = UnicodeWidthStr::width(seat_text.as_str());
    let count_w = UnicodeWidthStr::width(count_label.as_str());
    let key_w = UnicodeWidthStr::width(format!("[b+{key_char}]").as_str());
    // Reserve room for: seats + " " + count + " " + name + " " + key
    let fixed_w = seats_w + 1 + count_w + 1 + 1 + key_w;
    let name_budget = inner_width.saturating_sub(fixed_w).max(2);
    let truncated_name = truncate(&room.display_name, name_budget);
    let name_w = UnicodeWidthStr::width(truncated_name.as_str());
    let pad_w = inner_width
        .saturating_sub(seats_w + 1 + count_w + 1 + name_w + key_w)
        .max(1);
    let line1 = Line::from(vec![
        Span::styled(seat_text, Style::default().fg(theme::AMBER())),
        Span::raw(" "),
        Span::styled(count_label, Style::default().fg(theme::TEXT_DIM())),
        Span::raw(" "),
        Span::styled(
            truncated_name,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(pad_w)),
        key_tag,
    ]);

    let settings = BlackjackTableSettings::from_json(&room.settings);
    let pace_label = settings.pace.label();
    let stake_label = settings.stake_label();
    let phase_label = blackjack_phase_label(snapshot);
    let phase_color = match snapshot.map(|s| s.phase) {
        Some(crate::app::rooms::blackjack::state::Phase::PlayerTurn)
        | Some(crate::app::rooms::blackjack::state::Phase::DealerTurn) => theme::AMBER_GLOW(),
        Some(_) => theme::TEXT(),
        None => theme::TEXT_FAINT(),
    };
    let sep = || Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT()));
    let line2 = Line::from(vec![
        Span::styled(
            pace_label,
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        sep(),
        Span::styled(stake_label, Style::default().fg(theme::TEXT_DIM())),
        sep(),
        Span::styled(phase_label, Style::default().fg(phase_color)),
    ]);

    frame.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn blackjack_phase_label(snapshot: Option<&BlackjackSnapshot>) -> String {
    use crate::app::rooms::blackjack::state::Phase;
    let Some(snap) = snapshot else {
        return "idle".to_string();
    };
    match snap.phase {
        Phase::Betting => match snap.betting_countdown_secs {
            Some(secs) => format!("betting · {secs}s"),
            None => "betting".to_string(),
        },
        Phase::BetPending => "bet pending".to_string(),
        Phase::PlayerTurn => "player turn".to_string(),
        Phase::DealerTurn => "dealer".to_string(),
        Phase::Settling => "settling".to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(crate) fn favorites_strip_hit_test(
    area: Rect,
    show_header: bool,
    pins: &[(uuid::Uuid, String, bool, i64)],
    x: u16,
    y: u16,
) -> Option<uuid::Uuid> {
    let strip_area = favorites_strip_area(area, show_header, pins)?;
    if y != strip_area.y || x < strip_area.x || x >= strip_area.right() {
        return None;
    }

    let mut cursor_x = strip_area.x + 1;
    for (idx, (room_id, label, _, unread)) in pins.iter().enumerate() {
        if idx > 0 {
            cursor_x = cursor_x.saturating_add(1);
        }
        let slot = if idx < 9 {
            format!("{}:", idx + 1)
        } else {
            String::new()
        };
        let unread_suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        let pill = format!(" {slot}{label}{unread_suffix} ");
        let width = UnicodeWidthStr::width(pill.as_str()) as u16;
        let end_x = cursor_x.saturating_add(width);
        if x >= cursor_x && x < end_x {
            return Some(*room_id);
        }
        cursor_x = end_x;
    }
    None
}

pub(crate) fn cli_install_button_hit_test(area: Rect, show_header: bool, x: u16, y: u16) -> bool {
    let Some(button_area) = cli_install_button_area(area, show_header) else {
        return false;
    };
    y == button_area.y && x >= button_area.x && x < button_area.right()
}

pub(crate) fn browser_pair_button_hit_test(area: Rect, show_header: bool, x: u16, y: u16) -> bool {
    let Some(button_area) = browser_pair_button_area(area, show_header) else {
        return false;
    };
    y == button_area.y && x >= button_area.x && x < button_area.right()
}

/// Draws the chat card with the optional favorites pill strip above it when
/// there is room for a useful chat card below.
fn draw_chat_section(
    frame: &mut Frame,
    area: Rect,
    favorites_strip: Option<&[(uuid::Uuid, String, bool, i64)]>,
    chat_view: DashboardChatView<'_>,
) {
    let mut remaining = area;

    if let Some(pins) = favorites_strip
        && pins.len() >= 2
        && remaining.height >= 6
    {
        let split = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(remaining);
        draw_favorites_strip(frame, split[0], pins);
        remaining = split[1];
    }

    draw_dashboard_chat_card(frame, remaining, chat_view);
}

fn favorites_strip_area(
    area: Rect,
    show_header: bool,
    pins: &[(uuid::Uuid, String, bool, i64)],
) -> Option<Rect> {
    if pins.len() < 2 {
        return None;
    }

    let chat_area = if show_header {
        if area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH || area.height < DASHBOARD_MIN_FULL_HEIGHT {
            area
        } else {
            Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area)[1]
        }
    } else {
        area
    };

    if chat_area.height < 6 {
        return None;
    }

    Some(Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(chat_area)[0])
}

fn draw_favorites_strip(frame: &mut Frame, area: Rect, pins: &[(uuid::Uuid, String, bool, i64)]) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (idx, (_, label, active, unread)) in pins.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        // Slot hint doubles as the `g<digit>` target; only 1..9 are reachable
        // via the prefix, so pins beyond nine render without a number.
        let slot = if idx < 9 {
            format!("{}:", idx + 1)
        } else {
            String::new()
        };
        let style = if *active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        let unread_suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        spans.push(Span::styled(
            format!(" {slot}{label}{unread_suffix} "),
            style,
        ));
    }
    spans.push(Span::styled(
        "   [] cycle · , last · g_ jump · ` game",
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub struct StreamCardProps<'a> {
    pub now_playing: &'a str,
    pub current_genre: Genre,
    pub leading_genre: Genre,
    pub next_switch_in: Duration,
}

fn draw_stream_card(frame: &mut Frame, area: Rect, props: &StreamCardProps<'_>) {
    let block = Block::default()
        .title(" Stream ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Playing: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(props.now_playing, Style::default().fg(theme::TEXT_BRIGHT())),
        ]),
        Line::from(vec![
            Span::styled("Vibe: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                genre_label(props.current_genre),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Next: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                genre_label(props.leading_genre),
                Style::default().fg(theme::AMBER_DIM()),
            ),
            Span::styled("  Switch in ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format_duration_mmss(props.next_switch_in),
                Style::default().fg(theme::TEXT()),
            ),
        ]),
        Line::from(vec![
            Span::styled(AUDIO_BUTTON_PREFIX, Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                CLI_BUTTON_TEXT,
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                PAIR_BUTTON_TEXT,
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::BORDER_ACTIVE())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn cli_install_button_area(area: Rect, show_header: bool) -> Option<Rect> {
    audio_button_area(area, show_header, 0)
}

fn browser_pair_button_area(area: Rect, show_header: bool) -> Option<Rect> {
    audio_button_area(area, show_header, 1)
}

fn audio_button_area(area: Rect, show_header: bool, button_index: usize) -> Option<Rect> {
    let inner = stream_text_area(area, show_header)?;
    let prefix_width = UnicodeWidthStr::width(AUDIO_BUTTON_PREFIX) as u16;
    let cli_width = UnicodeWidthStr::width(CLI_BUTTON_TEXT) as u16;
    let gap_width = 2u16;
    let pair_width = UnicodeWidthStr::width(PAIR_BUTTON_TEXT) as u16;
    let (offset, width) = match button_index {
        0 => (prefix_width, cli_width),
        1 => (
            prefix_width
                .saturating_add(cli_width)
                .saturating_add(gap_width),
            pair_width,
        ),
        _ => return None,
    };
    Some(Rect::new(
        inner.x.saturating_add(offset),
        inner.y.saturating_add(2),
        width,
        1,
    ))
}

fn stream_text_area(area: Rect, show_header: bool) -> Option<Rect> {
    if !show_header
        || area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH
        || area.height < DASHBOARD_MIN_FULL_HEIGHT
    {
        return None;
    }

    let sections = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);
    let stream_area = if area.width <= DASHBOARD_HIDE_VOTE_AT_WIDTH {
        sections[0]
    } else {
        Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0])[0]
    };
    let inner = Block::default().borders(Borders::ALL).inner(stream_area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };
    Some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chat::ui::ChatRowsCache;
    use late_core::models::leaderboard::BadgeTier;
    use ratatui::{Terminal, backend::TestBackend};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn render_dashboard(width: u16) -> Vec<String> {
        render_dashboard_with_size(width, 20)
    }

    fn render_dashboard_with_size(width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let inline_images: HashMap<Uuid, Vec<ratatui::text::Line<'static>>> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, width, height);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: None,
                        pinned_messages: &[],
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        daily_statuses: &[],
                        wire_news_articles: &[],
                        dashboard_cycle_secs: 0,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            selected_news_message: false,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                            inline_images: &inline_images,
                        },
                    },
                );
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut out = String::new();
                for x in 0..width {
                    out.push_str(buffer[(x, y)].symbol());
                }
                out
            })
            .collect()
    }

    fn render_dashboard_section(
        width: u16,
        height: u16,
        pinned_messages: &[ChatMessage],
        daily_statuses: &[DashboardDailyStatus],
        wire_news_articles: &[ArticleFeedItem],
        dashboard_cycle_secs: u64,
    ) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let inline_images: HashMap<Uuid, Vec<ratatui::text::Line<'static>>> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();

        terminal
            .draw(|frame| {
                draw_blackjack_and_chat_section(
                    frame,
                    Rect::new(0, 0, width, height),
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: None,
                        pinned_messages,
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        daily_statuses,
                        wire_news_articles,
                        dashboard_cycle_secs,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            selected_news_message: false,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                            inline_images: &inline_images,
                        },
                    },
                );
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut out = String::new();
                for x in 0..width {
                    out.push_str(buffer[(x, y)].symbol());
                }
                out
            })
            .collect()
    }

    fn test_pinned_message(body: &str) -> ChatMessage {
        ChatMessage {
            id: Uuid::now_v7(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            pinned: true,
            reply_to_message_id: None,
            room_id: Uuid::nil(),
            user_id: Uuid::nil(),
            body: body.to_string(),
        }
    }

    fn test_article(title: &str, url: &str) -> ArticleFeedItem {
        ArticleFeedItem {
            article: late_core::models::article::Article {
                id: Uuid::now_v7(),
                created: chrono::Utc::now(),
                updated: chrono::Utc::now(),
                user_id: Uuid::nil(),
                url: url.to_string(),
                title: title.to_string(),
                summary: String::new(),
                ascii_art: String::new(),
            },
            author_username: String::new(),
        }
    }

    fn unfinished_daily_statuses() -> [DashboardDailyStatus; 4] {
        [
            DashboardDailyStatus {
                game: DailyGame::Sudoku,
                completed_today: false,
                launch_key: 's',
                streak: 3,
            },
            DashboardDailyStatus {
                game: DailyGame::Nonogram,
                completed_today: false,
                launch_key: 'n',
                streak: 3,
            },
            DashboardDailyStatus {
                game: DailyGame::Solitaire,
                completed_today: false,
                launch_key: 'o',
                streak: 3,
            },
            DashboardDailyStatus {
                game: DailyGame::Minesweeper,
                completed_today: false,
                launch_key: 'm',
                streak: 3,
            },
        ]
    }

    #[test]
    fn dashboard_hides_vote_card_at_77_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH);
        assert!(!lines.join("\n").contains("Vote Next"));
        assert_eq!(lines[0].chars().filter(|c| *c == '┌').count(), 1);
    }

    #[test]
    fn dashboard_keeps_vote_card_above_77_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH + 1);
        assert!(lines.join("\n").contains("Vote Next"));
        assert_eq!(lines[0].chars().filter(|c| *c == '┌').count(), 2);
    }

    #[test]
    fn dashboard_still_renders_at_77_column_terminal_content_width() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH);
        assert!(!lines.join("\n").contains("Dashboard view too small."));
        assert!(lines.join("\n").contains("Stream"));
        assert!(lines.join("\n").contains("[B] CLI"));
        assert!(lines.join("\n").contains("[P] web"));
        assert!(lines.join("\n").contains("No messages yet."));
    }

    #[test]
    fn dashboard_hides_top_stream_card_at_65_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_STREAM_AT_WIDTH);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Dashboard view too small."));
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_collapses_to_chat_when_height_below_minimum() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT - 1);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_keeps_full_layout_at_minimum_height() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Stream"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_blackjack_grid_renders_room_slots_when_tall_enough() {
        let lines = render_dashboard_with_size(100, 36);
        let rendered = lines.join("\n");
        assert!(rendered.contains("loading…"));
        assert!(rendered.contains("b+1"));
        assert!(rendered.contains("b+2"));
        assert!(rendered.contains("b+3"));
        assert!(!rendered.contains("b+4"));
    }

    #[test]
    fn dashboard_pinned_floats_above_grid_when_roomy() {
        let pinned = vec![test_pinned_message("Pin this above the grid")];
        let lines = render_dashboard_section(100, 10, &pinned, &[], &[], 0);
        let pinned_y = lines
            .iter()
            .position(|line| line.contains("Pin this above the grid"))
            .expect("pinned strip");
        let grid_y = lines
            .iter()
            .position(|line| line.contains("loading…"))
            .expect("grid row");

        assert!(pinned_y < grid_y);
        assert!(lines[pinned_y].contains("[b+4]"));
        // Pin strip is detached from the grid: plain `─` rule, no column junctions.
        assert!(lines[pinned_y].contains('─'));
        assert!(!lines[pinned_y].contains('┬'));
        assert!(!lines[pinned_y].contains('┌'));
        assert!(!lines[pinned_y].contains('┐'));
    }

    #[test]
    fn dashboard_pinned_falls_back_into_grid_rule_at_nine_rows() {
        let pinned = vec![test_pinned_message("Pin in the rule")];
        let lines = render_dashboard_section(100, 9, &pinned, &[], &[], 0);
        let pinned_y = lines
            .iter()
            .position(|line| line.contains("Pin in the rule"))
            .expect("pinned rule");
        let grid_y = lines
            .iter()
            .position(|line| line.contains("loading…"))
            .expect("grid row");

        assert!(pinned_y > grid_y);
        assert!(lines[pinned_y].contains("[b+4]"));
    }

    #[test]
    fn dashboard_dailies_slot_cycles_unfinished_games() {
        let statuses = unfinished_daily_statuses();

        let rendered_zero = render_dashboard_section(100, 9, &[], &statuses, &[], 0).join("\n");
        let rendered_second =
            render_dashboard_section(100, 9, &[], &statuses, &[], DASHBOARD_DAILY_CYCLE_SECONDS)
                .join("\n");
        let rendered_third = render_dashboard_section(
            100,
            9,
            &[],
            &statuses,
            &[],
            DASHBOARD_DAILY_CYCLE_SECONDS * 2,
        )
        .join("\n");

        assert!(rendered_zero.contains("Sudoku"));
        assert!(rendered_second.contains("Nonogram"));
        assert!(rendered_third.contains("Solitaire"));
    }

    #[test]
    fn dashboard_wire_slot_rotates_through_news_articles() {
        let articles = vec![
            test_article("First story", "https://a.example"),
            test_article("Second story", "https://b.example"),
        ];

        let rendered_first = render_dashboard_section(100, 9, &[], &[], &articles, 0).join("\n");
        let rendered_second =
            render_dashboard_section(100, 9, &[], &[], &articles, WIRE_NEWS_CYCLE_SECONDS)
                .join("\n");

        assert!(rendered_first.contains("First story"));
        assert!(rendered_first.contains("news 1/2"));
        assert!(rendered_second.contains("Second story"));
        assert!(rendered_second.contains("news 2/2"));
    }

    #[test]
    fn dashboard_wire_slot_shows_placeholder_when_no_articles() {
        let rendered = render_dashboard_section(100, 9, &[], &[], &[], 0).join("\n");
        assert!(rendered.contains("no headlines yet"));
    }

    #[test]
    fn dashboard_grid_width_gate_remains_but_chat_height_gate_is_gone() {
        let too_narrow =
            render_dashboard_section(BLACKJACK_GRID_MIN_WIDTH - 1, 20, &[], &[], &[], 0).join("\n");
        let tight_height = render_dashboard_section(
            BLACKJACK_GRID_MIN_WIDTH,
            BLACKJACK_GRID_HEIGHT,
            &[],
            &[],
            &[],
            0,
        )
        .join("\n");

        assert!(!too_narrow.contains("loading…"));
        assert!(tight_height.contains("loading…"));
    }

    #[test]
    fn dashboard_hides_stream_and_vote_when_header_setting_disabled() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let inline_images: HashMap<Uuid, Vec<ratatui::text::Line<'static>>> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 100, 20);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: false,
                        favorites_strip: None,
                        pinned_messages: &[],
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        daily_statuses: &[],
                        wire_news_articles: &[],
                        dashboard_cycle_secs: 0,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            selected_news_message: false,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                            inline_images: &inline_images,
                        },
                    },
                );
            })
            .expect("draw");

        let rendered = (0..20)
            .map(|y| {
                let mut out = String::new();
                for x in 0..100 {
                    out.push_str(terminal.backend().buffer()[(x, y)].symbol());
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_favorites_strip_renders_unread_counts() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let inline_images: HashMap<Uuid, Vec<ratatui::text::Line<'static>>> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();
        let rust_room = Uuid::now_v7();
        let go_room = Uuid::now_v7();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 100, 20);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: Some(&[
                            (rust_room, "#rust".to_string(), true, 3),
                            (go_room, "#go".to_string(), false, 0),
                        ]),
                        pinned_messages: &[],
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        daily_statuses: &[],
                        wire_news_articles: &[],
                        dashboard_cycle_secs: 0,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            selected_news_message: false,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                            inline_images: &inline_images,
                        },
                    },
                );
            })
            .expect("draw");

        let rendered = (0..20)
            .map(|y| {
                let mut out = String::new();
                for x in 0..100 {
                    out.push_str(terminal.backend().buffer()[(x, y)].symbol());
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("#rust (3)"));
        assert!(rendered.contains("#go"));
    }

    #[test]
    fn favorites_strip_hit_test_returns_clicked_room() {
        let rust_room = Uuid::now_v7();
        let go_room = Uuid::now_v7();
        let pins = vec![
            (rust_room, "#rust".to_string(), true, 3),
            (go_room, "#go".to_string(), false, 0),
        ];
        let area = Rect::new(1, 1, 74, 30);

        assert_eq!(
            favorites_strip_hit_test(area, true, &pins, 10, 6),
            Some(rust_room)
        );
        assert_eq!(
            favorites_strip_hit_test(area, true, &pins, 18, 6),
            Some(go_room)
        );
        assert_eq!(favorites_strip_hit_test(area, true, &pins, 40, 6), None);
    }

    #[test]
    fn favorites_strip_hit_test_returns_none_when_strip_hidden() {
        let room = Uuid::now_v7();
        let pins = vec![(room, "#rust".to_string(), true, 0)];

        assert_eq!(
            favorites_strip_hit_test(Rect::new(1, 1, 74, 30), true, &pins, 5, 7),
            None
        );
        assert_eq!(
            favorites_strip_hit_test(
                Rect::new(1, 1, 74, 5),
                false,
                &[
                    (room, "#rust".to_string(), true, 0),
                    (Uuid::now_v7(), "#go".to_string(), false, 0)
                ],
                5,
                1
            ),
            None
        );
    }

    #[test]
    fn dashboard_audio_buttons_hit_test_separately() {
        let area = Rect::new(0, 0, 100, 20);

        assert!(cli_install_button_hit_test(area, true, 12, 3));
        assert!(!cli_install_button_hit_test(area, true, 21, 3));
        assert!(browser_pair_button_hit_test(area, true, 21, 3));
        assert!(!browser_pair_button_hit_test(area, true, 12, 3));
    }
}
