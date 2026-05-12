//! Experimental Home view for the merged-shell redesign.
//!
//! Renders the cozy lounge layout: a 3-box "tonight" rituals grid up top
//! (daily quest, shop, the wire), an activity strip with a labeled banner
//! divider, and the dashboard chat filling the rest. The active multiplayer
//! rooms grid that used to live here moved to the right rail as the "active
//! tables" panel — this center pane is now about *your* rituals (quests, shop,
//! reading) rather than what other people are doing.
//!
//! Gated by `App::new_shell` (`LATE_UI_NEW_SHELL=1`). The old `draw_dashboard`
//! still handles the flag-off path until we flip the default.

use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::activity::event::ActivityEvent;
use crate::app::chat::ui::{DashboardChatView, draw_dashboard_chat_card};
use crate::app::common::theme;
use crate::app::dashboard::ui::DashboardRoomCard;

pub struct HomeNewRenderInput<'a> {
    /// Active multiplayer rooms — currently consumed only by the right rail's
    /// "active tables" panel, but kept on the input so future versions of the
    /// Home view can also reference them.
    #[allow(dead_code)]
    pub top_rooms: &'a [DashboardRoomCard],
    pub activity: &'a VecDeque<ActivityEvent>,
    pub online_count: usize,
    pub chat_view: DashboardChatView<'a>,
}

/// Below this width we drop the 3-box grid and let chat take everything.
const RITUALS_HIDE_AT_WIDTH: u16 = 50;
/// Below this height we also drop the activity strip — small terminals get
/// just the rituals strip + chat.
const ACTIVITY_HIDE_AT_HEIGHT: u16 = 22;
const RITUALS_ROW_HEIGHT: u16 = 5; // label row + 3 content rows + 1 spacer
const ACTIVITY_ROWS_BUDGET: u16 = 4;

pub fn draw_home_new_shell(frame: &mut Frame, area: Rect, view: HomeNewRenderInput<'_>) {
    if area.width <= 30 || area.height < 10 {
        // Tiny terminal: just chat, nothing else fits.
        draw_dashboard_chat_card(frame, area, view.chat_view);
        return;
    }

    let want_rituals = area.width > RITUALS_HIDE_AT_WIDTH && area.height >= 18;
    let want_activity = area.height >= ACTIVITY_HIDE_AT_HEIGHT;

    // Banner row (1 line "── activity ──") + content rows + 1 trailing rule
    // above chat. Section banner doubles as the divider, so no separate top rule.
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
    // Always a 1-row rule directly above chat so chat reads as its own block.
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Fill(1));

    let chunks = Layout::vertical(constraints).split(area);
    let mut idx = 0;

    if rituals_height > 0 {
        draw_rituals_strip(frame, chunks[idx]);
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

/// Three side-by-side ritual boxes. No borders — each box is just a quiet
/// italic label row, then text content, separated horizontally by 1-col
/// gutters. Mocked for now; daily-quest and shop are placeholders until those
/// systems land. The wire box pulls a few headlines but currently fakes them
/// until we thread real article data through.
fn draw_rituals_strip(frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(3), // gutter (space · vertical · space)
        Constraint::Fill(1),
        Constraint::Length(3), // gutter
        Constraint::Fill(1),
    ])
    .split(area);

    draw_box_daily_quest(frame, cols[0]);
    draw_box_shop(frame, cols[2]);
    draw_box_wire(frame, cols[4]);

    // Vertical dim separators centered in each 3-col gutter.
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
        Constraint::Length(1), // label
        Constraint::Length(1), // headline
        Constraint::Length(1), // detail
        Constraint::Length(1), // progress
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
    // Mock progress: 1 of 3
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

fn draw_box_wire(frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_box_label(frame, rows[0], "the wire");
    let max = rows[1].width as usize;
    // Mocked headlines until we thread real wire_news_articles in.
    let headlines = [
        "rust 1.86 ships async drop",
        "openbsd drops sysv shm",
        "github outage post-mortem",
    ];
    for (i, h) in headlines.iter().enumerate() {
        if i + 1 >= rows.len() {
            break;
        }
        let txt = truncate(h, max);
        let style = if i == 0 {
            Style::default().fg(theme::TEXT())
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(txt, style))),
            rows[i + 1],
        );
    }
}

/// "── ● online · 47 ──────────────" banner row + activity rows below.
/// Banner reads as "what's happening live now"; the green dot signals presence.
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

/// Presence banner: `── ● online · 47 ─────────────`. Green dot is the live
/// indicator; the count is the true online-user count from app state.
fn draw_online_banner(frame: &mut Frame, area: Rect, online_count: usize) {
    let count_str = format!("{online_count}");
    // Tally width: "── " (3) + "● " (2) + "online " (7) + count + " " (1) + trailing rule
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
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(theme::BORDER_DIM()),
    ));
    frame.render_widget(Paragraph::new(line), area);
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
