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

use crate::app::common::{
    primitives::{hint_line, thousands},
    theme,
};

use super::state::{Board, LeaderboardPageState};

const RAIL_WIDTH: u16 = 24;
const WINDOW_GAP_MAX: u16 = 3;

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
        Paragraph::new(hint_line(&[("j/k", "select board"), ("Tab", "next page")])),
        rows[2],
    );
}

fn draw_rail(frame: &mut Frame, area: Rect, state: &LeaderboardPageState) {
    let (lines, selected_line) = rail_lines(state);

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
    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), area);
}

/// The board rail. The bespoke boards lead under one "Boards" header; the
/// roster boards get one header per group. Returns the built lines and the
/// index of the selected row, so the caller can keep it scrolled into view.
fn rail_lines(state: &LeaderboardPageState) -> (Vec<Line<'static>>, usize) {
    let boards = state.boards();
    let first_daily = boards
        .iter()
        .position(|board| matches!(board, Board::Daily(_)));
    let first_score = boards
        .iter()
        .position(|board| matches!(board, Board::Score(_)));

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_line = 0usize;
    for (index, board) in boards.iter().copied().enumerate() {
        let header = if Some(index) == first_daily {
            Some("Daily Wins")
        } else if Some(index) == first_score {
            Some("High Scores")
        } else if index == 0 {
            Some("Boards")
        } else {
            None
        };
        if let Some(header) = header {
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
    (lines, selected_line)
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
            let capacity = (rows[3].height as usize).saturating_sub(1);
            let columns = standings_columns(
                rows[3],
                window_natural_width("monthly", monthly, board, view.user_id, capacity),
                window_natural_width("all-time", all_time, board, view.user_id, capacity),
            );
            draw_window(frame, columns[0], "monthly", monthly, board, view.user_id);
            draw_window(frame, columns[1], "all-time", all_time, board, view.user_id);
        }
        None => {
            draw_window(frame, rows[3], "this month", monthly, board, view.user_id);
        }
    }
}

/// Keep paired windows close enough to scan as one category. When both fit at
/// their natural widths, any surplus space goes after the second window rather
/// than stretching the pair across the screen. Narrow terminals retain a
/// one-cell gutter and divide the constrained width evenly.
fn standings_columns(area: Rect, first_width: usize, second_width: usize) -> [Rect; 2] {
    if area.width == 0 {
        return [area, area];
    }

    let first_width = first_width.min(u16::MAX as usize) as u16;
    let second_width = second_width.min(u16::MAX as usize) as u16;
    let required = first_width as usize + second_width as usize + 1;
    if required <= area.width as usize {
        let gap = (area.width - first_width - second_width).min(WINDOW_GAP_MAX);
        let columns = Layout::horizontal([
            Constraint::Length(first_width),
            Constraint::Length(gap),
            Constraint::Length(second_width),
            Constraint::Min(0),
        ])
        .split(area);
        [columns[0], columns[2]]
    } else {
        let columns = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);
        [columns[0], columns[2]]
    }
}

fn window_natural_width(
    heading: &'static str,
    entries: &[RankedEntry],
    board: Board,
    user_id: Uuid,
    capacity: usize,
) -> usize {
    let heading_width = heading.chars().count() + 8;
    let content_width = if entries.is_empty() {
        empty_copy(board).chars().count() + 2
    } else {
        let (leader_rows, own_tail) = window_row_plan(entries, user_id, capacity);
        entries
            .iter()
            .take(leader_rows)
            .chain(own_tail.map(|index| &entries[index]))
            .map(|entry| entry_natural_width(entry, board))
            .max()
            .unwrap_or(0)
    };

    heading_width.max(content_width)
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
    let capacity = (area.height as usize).saturating_sub(1);
    let lines = window_lines(
        heading,
        entries,
        board,
        user_id,
        capacity,
        area.width as usize,
    );
    frame.render_widget(Paragraph::new(lines), area);
}

/// One standings window: heading, then up to `capacity` leader rows. When the
/// viewer ranks below the visible leaders, their own row replaces the last
/// two rows as an ellipsis tail.
fn window_lines(
    heading: &'static str,
    entries: &[RankedEntry],
    board: Board,
    user_id: Uuid,
    capacity: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![section_heading(heading)];

    if entries.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(empty_copy(board), Style::default().fg(theme::TEXT_FAINT())),
        ]));
        return lines;
    }

    let (leader_rows, own_tail) = window_row_plan(entries, user_id, capacity);
    let content_width = entries
        .iter()
        .take(leader_rows)
        .chain(own_tail.map(|index| &entries[index]))
        .map(|entry| entry_natural_width(entry, board))
        .max()
        .unwrap_or(width)
        .min(width);

    for entry in entries.iter().take(leader_rows) {
        lines.push(entry_line(
            entry,
            board,
            entry.user_id == user_id,
            content_width,
        ));
    }
    if let Some(index) = own_tail {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("…", Style::default().fg(theme::TEXT_FAINT())),
        ]));
        lines.push(entry_line(&entries[index], board, true, content_width));
    }

    lines
}

fn window_row_plan(
    entries: &[RankedEntry],
    user_id: Uuid,
    capacity: usize,
) -> (usize, Option<usize>) {
    let own_index = entries.iter().position(|entry| entry.user_id == user_id);
    let own_visible = own_index.is_none_or(|index| index < capacity);
    let leader_rows = if own_visible {
        capacity
    } else {
        capacity.saturating_sub(2)
    };
    let own_tail = own_index.filter(|index| *index >= capacity);
    (leader_rows, own_tail)
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
    let value = entry_value(entry, board);

    // Right-align within the window's compact content width, leaving two cells
    // between the longest visible name and its value.
    let name_budget = width.saturating_sub(rank.chars().count() + value.chars().count() + 2);
    let name = truncate(&entry.username, name_budget);
    let pad =
        width.saturating_sub(rank.chars().count() + name.chars().count() + value.chars().count());
    Line::from(vec![
        Span::styled(rank, rank_style),
        Span::styled(name, name_style),
        Span::raw(" ".repeat(pad.max(2))),
        Span::styled(value, Style::default().fg(theme::TEXT_BRIGHT())),
    ])
}

fn entry_natural_width(entry: &RankedEntry, board: Board) -> usize {
    let rank_width = format!("  #{:<3}", entry.rank).chars().count();
    let value_width = entry_value(entry, board).chars().count();

    rank_width + entry.username.chars().count() + 2 + value_width
}

fn entry_value(entry: &RankedEntry, board: Board) -> String {
    let mut value = thousands(entry.value);
    let label = board.value_label();
    if !label.is_empty() {
        value.push(' ');
        value.push_str(label);
    }
    value
}

fn empty_copy(board: Board) -> &'static str {
    match board {
        Board::TopChips => "no chip earnings yet this month",
        Board::ArcadeWins => "no daily puzzle wins yet this month",
        Board::Daily(_) => "no wins yet, be the first",
        Board::Score(_) => "no scores yet, be the first",
    }
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

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
