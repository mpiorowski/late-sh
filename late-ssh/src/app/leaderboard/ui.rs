//! The Leaderboards page: a board list on the left, the selected board's
//! monthly and all-time standings on the right. One renderer serves every
//! board; per-board facts (title, hint, windows) come from `state::Board`.

use late_core::models::leaderboard::{LeaderboardData, RankedEntry};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::common::{primitives::hint_line, theme};

use super::state::{Board, LeaderboardPageState};

const RAIL_WIDTH: u16 = 24;

pub(crate) struct LeaderboardPageView<'a> {
    pub state: &'a LeaderboardPageState,
    pub data: &'a LeaderboardData,
    pub user_id: Uuid,
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &LeaderboardPageView<'_>) {
    if area.height < 8 || area.width < 48 {
        frame.render_widget(Paragraph::new("Terminal too small for Leaderboards"), area);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Min(0),    // body
        Constraint::Length(1), // footer
    ])
    .split(area);
    let columns = Layout::horizontal([
        Constraint::Length(RAIL_WIDTH),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(rows[1]);

    draw_rail(frame, columns[0], view.state);
    draw_detail(frame, columns[2], view);
    frame.render_widget(
        Paragraph::new(hint_line(&[
            ("j/k", "select board"),
            ("Tab", "next page"),
        ])),
        rows[2],
    );
}

fn draw_rail(frame: &mut Frame, area: Rect, state: &LeaderboardPageState) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_line = 0usize;
    for (index, board) in state.boards().iter().copied().enumerate() {
        if let Some(header) = group_header(state.boards(), index) {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(section_heading(header));
        }
        let selected = index == state.selected_index();
        if selected {
            selected_line = lines.len();
        }
        let style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        lines.push(Line::from(vec![
            Span::styled(if selected { " > " } else { "   " }, style),
            Span::styled(board.title(), style),
        ]));
    }

    // Keep the selection visible on short terminals without recentering on
    // every keypress: scroll only once it would leave the viewport.
    let visible = area.height as usize;
    let scroll = if visible >= lines.len() {
        0
    } else {
        selected_line
            .saturating_sub(visible.saturating_sub(2))
            .min(lines.len().saturating_sub(visible))
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll as u16, 0)),
        area,
    );
}

/// The rail groups the roster boards under one header each; the bespoke
/// boards lead ungrouped.
fn group_header(boards: &[Board], index: usize) -> Option<&'static str> {
    let first_daily = boards
        .iter()
        .position(|board| matches!(board, Board::Daily(_)));
    let first_score = boards
        .iter()
        .position(|board| matches!(board, Board::Score(_)));
    if Some(index) == first_daily {
        Some("Daily Wins")
    } else if Some(index) == first_score {
        Some("High Scores")
    } else if index == 0 {
        Some("Boards")
    } else {
        None
    }
}

fn draw_detail(frame: &mut Frame, area: Rect, view: &LeaderboardPageView<'_>) {
    let board = view.state.selected_board();
    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing room
        Constraint::Min(0),    // standings
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading(board.title())), rows[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(board.hint(), Style::default().fg(theme::TEXT_DIM())),
        ])),
        rows[1],
    );

    let monthly = board.monthly(view.data);
    match board.all_time(view.data) {
        Some(all_time) => {
            let columns = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(2),
                Constraint::Percentage(50),
            ])
            .split(rows[3]);
            draw_window(frame, columns[0], "monthly", monthly, board, view.user_id);
            draw_window(frame, columns[2], "all-time", all_time, board, view.user_id);
        }
        None => {
            draw_window(frame, rows[3], "this month", monthly, board, view.user_id);
        }
    }
}

fn draw_window(
    frame: &mut Frame,
    area: Rect,
    heading: &'static str,
    entries: &[RankedEntry],
    board: Board,
    user_id: Uuid,
) {
    if area.height < 3 || area.width < 12 {
        return;
    }
    let mut lines: Vec<Line<'static>> = vec![section_heading(heading)];

    if entries.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(empty_copy(board), Style::default().fg(theme::TEXT_FAINT())),
        ]));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Leader rows fill the space; when the viewer ranks below the visible
    // leaders their own row replaces the last two rows as an ellipsis tail.
    let capacity = (area.height as usize).saturating_sub(1);
    let own_row = entries.iter().find(|entry| entry.user_id == user_id);
    let own_visible = own_row
        .map(|own| {
            entries
                .iter()
                .take(capacity)
                .any(|entry| entry.user_id == own.user_id)
        })
        .unwrap_or(true);
    let leader_rows = if own_visible {
        capacity
    } else {
        capacity.saturating_sub(2)
    };

    let width = area.width as usize;
    for entry in entries.iter().take(leader_rows) {
        lines.push(entry_line(entry, board, entry.user_id == user_id, width));
    }
    if !own_visible && let Some(own) = own_row {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("…", Style::default().fg(theme::TEXT_FAINT())),
        ]));
        lines.push(entry_line(own, board, true, width));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn entry_line(entry: &RankedEntry, board: Board, own: bool, width: usize) -> Line<'static> {
    let rank_style = if entry.rank <= 3 {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if own {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };

    let rank = format!("  #{:<3}", entry.rank);
    let mut value = format_value(entry.value);
    let label = board.value_label();
    if !label.is_empty() {
        value.push(' ');
        value.push_str(label);
    }

    // Right-align the value; give whatever remains to the username.
    let name_budget = width.saturating_sub(rank.chars().count() + value.chars().count() + 3);
    let name = truncate(&entry.username, name_budget);
    let pad = width
        .saturating_sub(rank.chars().count() + name.chars().count() + value.chars().count() + 1);
    Line::from(vec![
        Span::styled(rank, rank_style),
        Span::styled(name, name_style),
        Span::raw(" ".repeat(pad.max(1))),
        Span::styled(value, Style::default().fg(theme::TEXT_BRIGHT())),
    ])
}

fn empty_copy(board: Board) -> &'static str {
    match board {
        Board::TopChips => "no chip earnings yet this month",
        Board::ArcadeWins => "no daily puzzle wins yet this month",
        Board::Daily(_) => "no wins yet, be the first",
        Board::Score(_) => "no scores yet, be the first",
    }
}

fn format_value(value: i64) -> String {
    let raw = value.to_string();
    let (sign, digits) = raw.strip_prefix('-').map_or(("", raw.as_str()), |rest| ("-", rest));
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + sign.len());
    out.push_str(sign);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn truncate(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return String::new();
    }
    let mut out: String = value.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}
