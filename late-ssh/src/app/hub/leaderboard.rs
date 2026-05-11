use late_core::models::leaderboard::{HighScoreEntry, LeaderboardData, RankedEntry};
use ratatui::{
    Frame,
    layout::{Constraint, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::common::theme;

pub fn draw(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    let layout = ratatui::layout::Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(12),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("monthly UTC", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(
                " boards refresh every 30s",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[1],
    );

    draw_boards(frame, layout[2], data, user_id);
}

fn draw_boards(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    if area.width >= 88 && area.height >= 20 {
        let columns =
            ratatui::layout::Layout::horizontal([Constraint::Percentage(44), Constraint::Min(48)])
                .split(area);
        let left = ratatui::layout::Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(columns[0]);
        let right = ratatui::layout::Layout::vertical([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(columns[1]);

        draw_ranked_board(
            frame,
            left[0],
            "Top Chips",
            "chips",
            &data.monthly_chip_earners,
            user_id,
            "No chip earnings yet this month",
            &["positive chip gains this UTC month"],
        );
        draw_ranked_board(
            frame,
            left[1],
            "Arcade Wins",
            "pts",
            &data.arcade_champions,
            user_id,
            "No daily puzzle wins yet this month",
            &[
                "daily puzzle wins, weighted by difficulty",
                "1 easy/draw-1, 3 medium, 5 hard/draw-3",
            ],
        );
        draw_score_board(
            frame,
            right[0],
            "Tetris",
            &data.monthly_tetris_high_scores,
            high_scores_for(data, "Tetris"),
            user_id,
        );
        draw_score_board(
            frame,
            right[1],
            "2048",
            &data.monthly_2048_high_scores,
            high_scores_for(data, "2048"),
            user_id,
        );
        draw_score_board(
            frame,
            right[2],
            "Snake",
            &data.monthly_snake_high_scores,
            high_scores_for(data, "Snake"),
            user_id,
        );
    } else {
        let rows = ratatui::layout::Layout::vertical([
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
        ])
        .split(area);
        draw_ranked_board(
            frame,
            rows[0],
            "Top Chips",
            "chips",
            &data.monthly_chip_earners,
            user_id,
            "No chip earnings yet this month",
            &["positive chip gains this UTC month"],
        );
        draw_ranked_board(
            frame,
            rows[1],
            "Arcade Wins",
            "pts",
            &data.arcade_champions,
            user_id,
            "No daily puzzle wins yet this month",
            &[
                "daily puzzle wins, weighted by difficulty",
                "1 easy/draw-1, 3 medium, 5 hard/draw-3",
            ],
        );
        draw_score_board(
            frame,
            rows[2],
            "Tetris",
            &data.monthly_tetris_high_scores,
            high_scores_for(data, "Tetris"),
            user_id,
        );
        draw_score_board(
            frame,
            rows[3],
            "2048",
            &data.monthly_2048_high_scores,
            high_scores_for(data, "2048"),
            user_id,
        );
        draw_score_board(
            frame,
            rows[4],
            "Snake",
            &data.monthly_snake_high_scores,
            high_scores_for(data, "Snake"),
            user_id,
        );
    }
}

fn high_scores_for<'a>(data: &'a LeaderboardData, game: &str) -> Vec<&'a HighScoreEntry> {
    data.high_scores
        .iter()
        .filter(|entry| entry.game == game)
        .collect()
}

fn draw_ranked_board(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    unit: &str,
    entries: &[RankedEntry],
    user_id: Uuid,
    empty: &str,
    hints: &[&str],
) {
    let block = panel_block(title);
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    push_hint_lines(&mut lines, hints, inner.width);
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            empty.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        let current_index = entries.iter().position(|entry| entry.user_id == user_id);
        for row in board_rows(
            entries.len(),
            current_index,
            visible_entry_count(inner.height.saturating_sub(lines.len() as u16)),
            10,
            3,
        ) {
            match row {
                BoardRow::Entry(index) => {
                    let entry = &entries[index];
                    lines.push(ranked_line(
                        entry.rank,
                        &entry.username,
                        entry.value,
                        unit,
                        entry.user_id == user_id,
                        inner.width,
                    ));
                }
                BoardRow::AroundYou => lines.push(around_you_line(inner.width)),
            }
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_score_board(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    monthly: &[HighScoreEntry],
    all_time: Vec<&HighScoreEntry>,
    user_id: Uuid,
) {
    let block = panel_block(title);
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);

    let columns = ratatui::layout::Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(inner);
    draw_score_list(
        frame,
        columns[0],
        "monthly",
        monthly.iter().collect(),
        user_id,
        "No monthly scores",
        &["best score this UTC month"],
    );
    draw_score_list(
        frame,
        columns[1],
        "all-time",
        all_time,
        user_id,
        "No all-time scores",
        &["personal bests"],
    );
}

fn draw_score_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    entries: Vec<&HighScoreEntry>,
    user_id: Uuid,
    empty: &str,
    hints: &[&str],
) {
    if area.height == 0 {
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    ))];
    push_hint_lines(&mut lines, hints, area.width);
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            empty.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        let current_index = entries.iter().position(|entry| entry.user_id == user_id);
        for row in board_rows(
            entries.len(),
            current_index,
            visible_entry_count(area.height.saturating_sub(lines.len() as u16)),
            3,
            3,
        ) {
            match row {
                BoardRow::Entry(index) => {
                    let entry = entries[index];
                    lines.push(score_line(
                        entry.rank,
                        &entry.username,
                        i64::from(entry.score),
                        entry.user_id == user_id,
                        area.width,
                    ));
                }
                BoardRow::AroundYou => lines.push(around_you_line(area.width)),
            }
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn visible_entry_count(height: u16) -> usize {
    usize::from(height).clamp(1, 10)
}

fn push_hint_lines(lines: &mut Vec<Line<'static>>, hints: &[&str], width: u16) {
    if width < 12 {
        return;
    }
    for hint in hints {
        lines.push(Line::from(Span::styled(
            truncate(hint, width as usize),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoardRow {
    Entry(usize),
    AroundYou,
}

fn board_rows(
    entry_count: usize,
    current_index: Option<usize>,
    row_budget: usize,
    top_limit: usize,
    around_limit: usize,
) -> Vec<BoardRow> {
    if entry_count == 0 || row_budget == 0 {
        return Vec::new();
    }

    let top_without_around = row_budget.min(top_limit).min(entry_count);
    let Some(current_index) = current_index else {
        return entry_rows(top_without_around);
    };
    if current_index < top_without_around {
        return entry_rows(top_without_around);
    }
    if row_budget < 3 {
        return vec![BoardRow::Entry(current_index)];
    }

    let around_count = row_budget
        .saturating_sub(2)
        .min(around_limit)
        .min(entry_count)
        .max(1);
    let top_count = row_budget
        .saturating_sub(around_count + 1)
        .min(top_limit)
        .min(entry_count);
    let mut rows = entry_rows(top_count);
    rows.push(BoardRow::AroundYou);

    let (start, end) = centered_window(current_index, entry_count, around_count);
    rows.extend(
        (start..end)
            .filter(|index| *index >= top_count)
            .map(BoardRow::Entry),
    );
    rows.truncate(row_budget);
    rows
}

fn entry_rows(count: usize) -> Vec<BoardRow> {
    (0..count).map(BoardRow::Entry).collect()
}

fn centered_window(center: usize, len: usize, count: usize) -> (usize, usize) {
    let count = count.min(len);
    let half = count / 2;
    let mut start = center.saturating_sub(half);
    if start + count > len {
        start = len.saturating_sub(count);
    }
    (start, start + count)
}

fn around_you_line(width: u16) -> Line<'static> {
    let side = if width > 18 { "-- " } else { "" };
    Line::from(vec![
        Span::styled(side, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            "around you",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(side, Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn ranked_line(
    rank: i64,
    username: &str,
    value: i64,
    unit: &str,
    is_current_user: bool,
    width: u16,
) -> Line<'static> {
    let rank_style = if is_current_user || rank == 1 {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if is_current_user {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD)
    } else if rank == 1 {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let value_style = if is_current_user || rank == 1 {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::SUCCESS())
    };
    let reserved = 18 + unit.len();
    let max_name = (width as usize).saturating_sub(reserved).max(4);
    let name = truncate(username, max_name);

    Line::from(vec![
        Span::styled(format!("#{rank:<3}"), rank_style),
        Span::styled(name, name_style),
        Span::raw(" "),
        Span::styled(format!("{value} {unit}"), value_style),
    ])
}

fn score_line(
    rank: i64,
    username: &str,
    score: i64,
    is_current_user: bool,
    width: u16,
) -> Line<'static> {
    let rank_style = if is_current_user || rank == 1 {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if is_current_user {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let reserved = 10usize;
    let max_name = (width as usize).saturating_sub(reserved).max(3);
    Line::from(vec![
        Span::styled(format!("#{rank} "), rank_style),
        Span::styled(truncate(username, max_name), name_style),
        Span::raw(" "),
        Span::styled(
            score.to_string(),
            if is_current_user {
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::SUCCESS())
            },
        ),
    ])
}

fn panel_block(title: &str) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .title_style(
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars && max_chars > 1 {
        out.pop();
        out.push('~');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{BoardRow, board_rows};

    #[test]
    fn board_rows_uses_plain_top_rows_when_current_user_is_visible() {
        assert_eq!(
            board_rows(20, Some(2), 6, 10, 3),
            vec![
                BoardRow::Entry(0),
                BoardRow::Entry(1),
                BoardRow::Entry(2),
                BoardRow::Entry(3),
                BoardRow::Entry(4),
                BoardRow::Entry(5),
            ]
        );
    }

    #[test]
    fn board_rows_adds_around_you_window_for_deep_rank() {
        assert_eq!(
            board_rows(100, Some(49), 6, 10, 3),
            vec![
                BoardRow::Entry(0),
                BoardRow::Entry(1),
                BoardRow::AroundYou,
                BoardRow::Entry(48),
                BoardRow::Entry(49),
                BoardRow::Entry(50),
            ]
        );
    }

    #[test]
    fn board_rows_keeps_current_user_visible_at_bottom_edge() {
        assert_eq!(
            board_rows(50, Some(49), 6, 10, 3),
            vec![
                BoardRow::Entry(0),
                BoardRow::Entry(1),
                BoardRow::AroundYou,
                BoardRow::Entry(47),
                BoardRow::Entry(48),
                BoardRow::Entry(49),
            ]
        );
    }
}
