use late_core::models::leaderboard::{
    HighScoreEntry, LeaderboardData, LeaderboardEntry, RankedEntry,
};
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
        );
        draw_streak_board(
            frame,
            left[1],
            "Daily Streaks",
            &data.streak_leaders,
            user_id,
            "No daily streaks yet",
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
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
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
        );
        draw_streak_board(
            frame,
            rows[1],
            "Daily Streaks",
            &data.streak_leaders,
            user_id,
            "No daily streaks yet",
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
) {
    let block = panel_block(title);
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            empty.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        let visible_entries = visible_entry_count(inner.height);
        for entry in entries.iter().take(visible_entries) {
            lines.push(ranked_line(
                entry.rank,
                &entry.username,
                entry.value,
                unit,
                entry.user_id == user_id,
                inner.width,
            ));
        }
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.user_id == user_id && entry.rank > visible_entries as i64)
            .filter(|_| lines.len() + 2 <= inner.height as usize)
        {
            lines.push(Line::from(""));
            lines.push(ranked_line(
                entry.rank,
                &entry.username,
                entry.value,
                unit,
                true,
                inner.width,
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_streak_board(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    entries: &[LeaderboardEntry],
    user_id: Uuid,
    empty: &str,
) {
    let block = panel_block(title);
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            empty.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        let visible_entries = visible_entry_count(inner.height);
        for (index, entry) in entries.iter().take(visible_entries).enumerate() {
            lines.push(ranked_line(
                (index + 1) as i64,
                &entry.username,
                i64::from(entry.count),
                "days",
                entry.user_id == user_id,
                inner.width,
            ));
        }
        if let Some((index, entry)) = entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.user_id == user_id)
            .filter(|(index, _)| *index >= visible_entries)
            .filter(|_| lines.len() + 2 <= inner.height as usize)
        {
            lines.push(Line::from(""));
            lines.push(ranked_line(
                (index + 1) as i64,
                &entry.username,
                i64::from(entry.count),
                "days",
                true,
                inner.width,
            ));
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
    );
    draw_score_list(
        frame,
        columns[1],
        "all-time",
        all_time.into_iter().take(3).collect(),
        user_id,
        "No all-time scores",
    );
}

fn draw_score_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    entries: Vec<&HighScoreEntry>,
    user_id: Uuid,
    empty: &str,
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
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            empty.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        let visible_entries = visible_entry_count(area.height.saturating_sub(1)).min(3);
        for entry in entries.into_iter().take(visible_entries) {
            lines.push(score_line(
                entry.rank,
                &entry.username,
                i64::from(entry.score),
                entry.user_id == user_id,
                area.width,
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn visible_entry_count(height: u16) -> usize {
    usize::from(height).clamp(1, 10)
}

fn ranked_line(
    rank: i64,
    username: &str,
    value: i64,
    unit: &str,
    is_current_user: bool,
    width: u16,
) -> Line<'static> {
    let rank_style = if rank == 1 {
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
    let value_style = if rank == 1 {
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
    let rank_style = if rank == 1 {
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
        Span::styled(score.to_string(), Style::default().fg(theme::SUCCESS())),
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
