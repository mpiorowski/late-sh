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
    chat::ui::{DashboardChatView, draw_dashboard_chat_card},
    common::theme,
    rooms::{
        registry::{RoomDirectorySummary, RoomGameRegistry},
        svc::{GameKind, RoomListItem, RoomsSnapshot},
    },
};
use late_core::models::article::ArticleFeedItem;

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
    pub chat_view: DashboardChatView<'a>,
}

/// Page-1 Home surface: rituals strip, live activity, and the selected room's
/// chat. Non-general rooms bypass this and render as full chat in `render.rs`.
pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    if area.width <= 30 || area.height < 10 {
        draw_dashboard_chat_card(frame, area, view.chat_view);
        return;
    }

    let want_rituals = area.width > RITUALS_HIDE_AT_WIDTH && area.height >= 18;
    let want_activity = area.height >= ACTIVITY_HIDE_AT_HEIGHT;

    let rituals_height = if want_rituals { RITUALS_ROW_HEIGHT } else { 0 };
    let activity_height = if want_activity {
        1 + ACTIVITY_ROWS_BUDGET
    } else {
        0
    };

    let mut constraints: Vec<Constraint> = Vec::new();
    if rituals_height > 0 {
        constraints.push(Constraint::Length(rituals_height));
    }
    if activity_height > 0 {
        constraints.push(Constraint::Length(activity_height));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Fill(1));

    let chunks = Layout::vertical(constraints).split(area);
    let mut idx = 0;

    if rituals_height > 0 {
        draw_rituals_strip(
            frame,
            chunks[idx],
            view.wire_news_articles,
            view.dashboard_cycle_secs,
        );
        idx += 1;
    }
    if activity_height > 0 {
        draw_activity_banner_section(frame, chunks[idx], view.activity, view.online_count);
        idx += 1;
    }
    draw_horizontal_rule(frame, chunks[idx]);
    idx += 1;
    draw_dashboard_chat_card(frame, chunks[idx], view.chat_view);
}

const RITUALS_HIDE_AT_WIDTH: u16 = 50;
const ACTIVITY_HIDE_AT_HEIGHT: u16 = 22;
const RITUALS_ROW_HEIGHT: u16 = 5;
const ACTIVITY_ROWS_BUDGET: u16 = 4;

fn draw_rituals_strip(
    frame: &mut Frame,
    area: Rect,
    wire_news_articles: &[ArticleFeedItem],
    cycle_secs: u64,
) {
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .split(area);

    draw_box_daily_quest(frame, cols[0]);
    draw_box_shop(frame, cols[2]);
    draw_box_wire(frame, cols[4], wire_news_articles, cycle_secs);

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

fn draw_box_label(frame: &mut Frame, area: Rect, label: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ))),
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

    draw_box_label(frame, rows[0], "daily quest");
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

    draw_box_label(frame, rows[0], "shop");
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

fn draw_box_wire(frame: &mut Frame, area: Rect, articles: &[ArticleFeedItem], cycle_secs: u64) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_box_label(frame, rows[0], "the wire");
    let max = rows[1].width as usize;
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
    let visible = (rows.len() - 1).min(pool.len());
    for offset in 0..visible {
        let item = &pool[(first + offset) % pool.len()];
        let txt = truncate(item.article.title.as_str(), max);
        let style = if offset == 0 {
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(txt, style))),
            rows[offset + 1],
        );
    }
}

fn draw_activity_banner_section(
    frame: &mut Frame,
    area: Rect,
    activity: &VecDeque<ActivityEvent>,
    online_count: usize,
) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(area);
    draw_online_banner(frame, rows[0], online_count);

    let body = rows[1];
    let rows_budget = body.height.min(ACTIVITY_ROWS_BUDGET) as usize;
    let mut lines: Vec<Line<'_>> = Vec::with_capacity(rows_budget);
    for event in activity.iter().rev().take(rows_budget) {
        let elapsed = event.at.elapsed().as_secs();
        let ago = if elapsed < 60 {
            format!("{}s", elapsed)
        } else if elapsed < 3600 {
            format!("{}m", elapsed / 60)
        } else {
            format!("{}h", elapsed / 3600)
        };
        let action_w = (body.width as usize).saturating_sub(ago.len() + 22);
        let action = truncate(&event.action, action_w);
        let user = truncate(&event.username, 16);
        lines.push(Line::from(vec![
            Span::styled(format!("@{}", user), Style::default().fg(theme::TEXT())),
            Span::raw("  "),
            Span::styled(action, Style::default().fg(theme::TEXT_DIM())),
            Span::raw("  "),
            Span::styled(ago, Style::default().fg(theme::TEXT_FAINT())),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "the room is quiet",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
    }
    frame.render_widget(Paragraph::new(lines), body);
}

fn draw_online_banner(frame: &mut Frame, area: Rect, online_count: usize) {
    let count_str = format!("{online_count}");
    let consumed = 3 + 2 + 7 + count_str.chars().count() + 1;
    let trail_w = (area.width as usize).saturating_sub(consumed);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("── ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled("● ", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                "online ",
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(count_str, Style::default().fg(theme::AMBER_DIM())),
            Span::raw(" "),
            Span::styled(
                "─".repeat(trail_w),
                Style::default().fg(theme::BORDER_DIM()),
            ),
        ])),
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
