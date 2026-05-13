use std::{cmp::Reverse, collections::VecDeque};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    activity::event::ActivityEvent,
    chat::{
        news::ui::split_summary_bullets,
        ui::{DashboardChatView, draw_dashboard_chat_card},
    },
    common::theme,
    rooms::{
        registry::{RoomDirectorySummary, RoomGameRegistry},
        svc::{GameKind, RoomListItem, RoomsSnapshot},
    },
};
use late_core::models::{article::ArticleFeedItem, chat_message::ChatMessage};

/// 1 minute per wire headline. The wire is meant as a slow ambient feed:
/// glance at Home every few minutes and see something new without churn.
pub(crate) const WIRE_NEWS_CYCLE_SECONDS: u64 = 60;
pub(crate) const WIRE_NEWS_MAX_ITEMS: usize = 5;

#[derive(Clone, Debug)]
pub struct DashboardRoomCard {
    pub room: RoomListItem,
    pub game_label: &'static str,
    pub occupied_seats: Option<usize>,
    pub total_seats: usize,
    pub pace: String,
    pub stakes: String,
}

impl DashboardRoomCard {
    fn new(room: &RoomListItem, summary: RoomDirectorySummary) -> Self {
        Self {
            room: room.clone(),
            game_label: summary.game_label,
            occupied_seats: summary.occupied_seats,
            total_seats: summary.total_seats,
            pace: summary.pace,
            stakes: summary.stakes,
        }
    }
}

/// Top N multiplayer rooms by occupancy/game priority. Empty rooms are kept so
/// the right rail can advertise available tables before anyone sits.
pub fn top_dashboard_rooms(
    snapshot: &RoomsSnapshot,
    registry: &RoomGameRegistry,
    max: usize,
) -> Vec<DashboardRoomCard> {
    let mut rooms: Vec<DashboardRoomCard> = snapshot
        .rooms
        .iter()
        .map(|room| DashboardRoomCard::new(room, registry.directory_summary(room)))
        .collect();
    sort_dashboard_room_cards(&mut rooms);
    rooms.truncate(max);
    rooms
}

fn sort_dashboard_room_cards(rooms: &mut [DashboardRoomCard]) {
    rooms.sort_by_key(|room| {
        (
            Reverse(room.occupied_seats.unwrap_or(0)),
            dashboard_room_game_priority(room.room.game_kind),
            Reverse(room.total_seats),
        )
    });
}

fn dashboard_room_game_priority(kind: GameKind) -> u8 {
    match kind {
        GameKind::Poker => 0,
        GameKind::Blackjack => 1,
        GameKind::TicTacToe => 2,
    }
}

pub struct DashboardRenderInput<'a> {
    pub activity: &'a VecDeque<ActivityEvent>,
    pub online_count: usize,
    pub wire_news_articles: &'a [ArticleFeedItem],
    pub dashboard_cycle_secs: u64,
    pub show_lounge_info: bool,
    pub pinned_messages: &'a [ChatMessage],
    pub chat_view: DashboardChatView<'a>,
}

/// Page-1 Home surface: top strip (activity/quest/shop), a wide wire feed, and
/// the selected room's chat. Non-general rooms bypass this and render as full
/// chat in `render.rs`.
pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    if area.width <= 30 || area.height < 10 {
        draw_dashboard_chat_card(frame, area, view.chat_view);
        return;
    }

    let want_top =
        view.show_lounge_info && area.width > TOP_STRIP_HIDE_AT_WIDTH && area.height >= 18;
    let want_wire = view.show_lounge_info && area.height >= WIRE_HIDE_AT_HEIGHT;
    let want_pinned = !view.pinned_messages.is_empty() && area.height >= 14;

    let top_height = if want_top { TOP_STRIP_ROW_HEIGHT } else { 0 };
    let wire_height = if want_wire { WIRE_STRIP_ROW_HEIGHT } else { 0 };
    let pinned_height: u16 = if want_pinned { 1 } else { 0 };
    let want_bottom_rule = top_height > 0 || wire_height > 0 || pinned_height > 0;

    let mut constraints: Vec<Constraint> = Vec::new();
    if top_height > 0 {
        constraints.push(Constraint::Length(top_height));
    }
    if wire_height > 0 {
        constraints.push(Constraint::Length(wire_height));
    }
    if pinned_height > 0 {
        constraints.push(Constraint::Length(1)); // dim rule above pin
        constraints.push(Constraint::Length(pinned_height));
    }
    if want_bottom_rule {
        constraints.push(Constraint::Length(1)); // bottom rule (amber if pinned, dim otherwise)
    }
    constraints.push(Constraint::Fill(1));

    let chunks = Layout::vertical(constraints).split(area);
    let mut idx = 0;

    if top_height > 0 {
        draw_top_strip(frame, chunks[idx], view.activity, view.online_count);
        idx += 1;
    }
    if wire_height > 0 {
        draw_wire_strip(
            frame,
            chunks[idx],
            view.wire_news_articles,
            view.dashboard_cycle_secs,
        );
        idx += 1;
    }
    if pinned_height > 0 {
        draw_horizontal_rule(frame, chunks[idx]);
        idx += 1;
        draw_pinned_row(frame, chunks[idx], &view.pinned_messages[0]);
        idx += 1;
        draw_amber_rule(frame, chunks[idx]);
        idx += 1;
    } else if want_bottom_rule {
        draw_horizontal_rule(frame, chunks[idx]);
        idx += 1;
    }
    draw_dashboard_chat_card(frame, chunks[idx], view.chat_view);
}

const TOP_STRIP_HIDE_AT_WIDTH: u16 = 50;
const WIRE_HIDE_AT_HEIGHT: u16 = 22;
const TOP_STRIP_ROW_HEIGHT: u16 = 5;
const WIRE_STRIP_ROW_HEIGHT: u16 = 5;

fn draw_top_strip(
    frame: &mut Frame,
    area: Rect,
    activity: &VecDeque<ActivityEvent>,
    online_count: usize,
) {
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .split(area);

    draw_box_activity(frame, cols[0], activity, online_count);
    draw_box_daily_quest(frame, cols[2]);
    draw_box_shop(frame, cols[4]);

    crate::app::common::sidebar::paint_vertical_separator(
        frame,
        cols[1].x + 1,
        cols[1].y,
        cols[1].height,
    );
    crate::app::common::sidebar::paint_vertical_separator(
        frame,
        cols[3].x + 1,
        cols[3].y,
        cols[3].height,
    );
}

fn draw_box_label_with_hint(frame: &mut Frame, area: Rect, label: &str, hint: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                label.to_string(),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled(
                hint.to_string(),
                Style::default()
                    .fg(theme::BORDER_DIM())
                    .add_modifier(Modifier::ITALIC),
            ),
        ])),
        area,
    );
}

fn draw_box_daily_quest(frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_box_label_with_hint(frame, rows[0], "daily quest", "(coming soon)");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "win 3 hands",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "any table",
            Style::default().fg(theme::TEXT_DIM()),
        ))),
        rows[2],
    );

    let bar_w = (rows[3].width as usize).saturating_sub(6);
    let filled = bar_w / 3;
    let empty = bar_w.saturating_sub(filled);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("█".repeat(filled), Style::default().fg(theme::SUCCESS())),
            Span::styled("░".repeat(empty), Style::default().fg(theme::BORDER_DIM())),
            Span::styled(" 1/3", Style::default().fg(theme::TEXT_DIM())),
        ])),
        rows[3],
    );
}

fn draw_box_shop(frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_box_label_with_hint(frame, rows[0], "shop", "(coming soon)");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "golden chips",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "new this week",
            Style::default().fg(theme::TEXT_DIM()),
        ))),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("●", Style::default().fg(theme::AMBER())),
            Span::styled(" 200", Style::default().fg(theme::AMBER())),
            Span::styled("  to buy", Style::default().fg(theme::TEXT_FAINT())),
        ])),
        rows[3],
    );
}

fn draw_box_activity(
    frame: &mut Frame,
    area: Rect,
    activity: &VecDeque<ActivityEvent>,
    online_count: usize,
) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "online",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled("● ", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                online_count.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" here", Style::default().fg(theme::TEXT_DIM())),
        ])),
        rows[0],
    );

    let event_rows = [rows[1], rows[2], rows[3], rows[4]];
    let mut drawn = 0;
    for (i, event) in activity.iter().rev().take(event_rows.len()).enumerate() {
        let row = event_rows[i];
        let body_w = row.width as usize;
        let elapsed = event.at.elapsed().as_secs();
        let ago = if elapsed < 60 {
            format!("{}s", elapsed)
        } else if elapsed < 3600 {
            format!("{}m", elapsed / 60)
        } else {
            format!("{}h", elapsed / 3600)
        };
        let user = truncate(&event.username, 12);
        let user_part = format!("@{}", user);
        let action_w = body_w.saturating_sub(user_part.chars().count() + ago.chars().count() + 4);
        let action = truncate(&event.action, action_w);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(user_part, Style::default().fg(theme::TEXT())),
                Span::raw("  "),
                Span::styled(action, Style::default().fg(theme::TEXT_DIM())),
                Span::raw("  "),
                Span::styled(ago, Style::default().fg(theme::TEXT_FAINT())),
            ])),
            row,
        );
        drawn += 1;
    }
    if drawn == 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "the room is quiet",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            event_rows[0],
        );
    }
}

fn draw_wire_strip(frame: &mut Frame, area: Rect, articles: &[ArticleFeedItem], cycle_secs: u64) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let constraints: Vec<Constraint> = (0..area.height).map(|_| Constraint::Length(1)).collect();
    let rows = Layout::vertical(constraints).split(area);

    draw_wire_top_border(frame, rows[0]);
    if rows.len() < 2 {
        return;
    }

    let pool = &articles[..articles.len().min(WIRE_NEWS_MAX_ITEMS)];
    if pool.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no headlines yet",
                Style::default().fg(theme::TEXT_FAINT()),
            ))),
            rows[1],
        );
        return;
    }

    let first = ((cycle_secs / WIRE_NEWS_CYCLE_SECONDS) as usize) % pool.len();
    draw_wire_article(frame, &rows, &pool[first]);
}

fn draw_wire_top_border(frame: &mut Frame, area: Rect) {
    let label = "the wire";
    let consumed = 3 + label.chars().count() + 1;
    let trail_w = (area.width as usize).saturating_sub(consumed);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("── ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(
                label,
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw(" "),
            Span::styled(
                "─".repeat(trail_w),
                Style::default().fg(theme::BORDER_DIM()),
            ),
        ])),
        area,
    );
}

fn draw_wire_article(frame: &mut Frame, rows: &[Rect], item: &ArticleFeedItem) {
    if rows.len() < 2 {
        return;
    }
    let title_w = rows[1].width as usize;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(item.article.title.as_str(), title_w),
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::BOLD),
        ))),
        rows[1],
    );

    if rows.len() < 3 {
        return;
    }
    let bullet_rows = &rows[2..];
    let bullets = split_summary_bullets(&item.article.summary);
    for (i, row) in bullet_rows.iter().enumerate() {
        let Some(bullet) = bullets.get(i) else { break };
        let text = truncate(bullet, row.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            *row,
        );
    }
}

fn draw_pinned_row(frame: &mut Frame, area: Rect, message: &ChatMessage) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let glyph = "● ";
    let label = "pinned  ";
    let prefix_w = glyph.chars().count() + label.chars().count();
    let body_w = (area.width as usize).saturating_sub(prefix_w);
    let flat_body: String = message
        .body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let body = truncate(&flat_body, body_w);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(glyph, Style::default().fg(theme::AMBER())),
            Span::styled(
                label,
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(body, Style::default().fg(theme::TEXT())),
        ])),
        area,
    );
}

fn draw_amber_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::AMBER_DIM()),
        ))),
        area,
    );
}

fn draw_horizontal_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::BORDER_DIM()),
        ))),
        area,
    );
}

fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max - 1).collect();
    out.push('…');
    out
}
