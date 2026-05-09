use late_core::models::leaderboard::{HighScoreEntry, LeaderboardData, RankedEntry};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::common::theme;

pub const MODAL_WIDTH: u16 = 104;
pub const MODAL_HEIGHT: u16 = 34;

pub fn draw(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Leaderboards ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .split(inner);

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

    draw_boards(frame, layout[3], data, user_id);
    draw_footer(frame, layout[4]);
}

fn draw_boards(frame: &mut Frame, area: Rect, data: &LeaderboardData, user_id: Uuid) {
    if area.width >= 88 && area.height >= 18 {
        let rows =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
        let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);
        let bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);
        draw_ranked_board(
            frame,
            top[0],
            "Top Chips",
            "chips",
            &data.monthly_chip_earners,
            user_id,
            "No chip earnings yet this month",
        );
        draw_ranked_board(
            frame,
            top[1],
            "Arcade Champion",
            "pts",
            &data.arcade_champions,
            user_id,
            "No daily puzzle wins yet this month",
        );
        draw_score_board(
            frame,
            bottom[0],
            "Tetris",
            &data.monthly_tetris_high_scores,
            user_id,
        );
        draw_score_board(
            frame,
            bottom[1],
            "2048",
            &data.monthly_2048_high_scores,
            user_id,
        );
    } else {
        let rows = Layout::vertical([
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
        draw_ranked_board(
            frame,
            rows[1],
            "Arcade Champion",
            "pts",
            &data.arcade_champions,
            user_id,
            "No daily puzzle wins yet this month",
        );
        draw_score_board(
            frame,
            rows[2],
            "Tetris",
            &data.monthly_tetris_high_scores,
            user_id,
        );
        draw_score_board(
            frame,
            rows[3],
            "2048",
            &data.monthly_2048_high_scores,
            user_id,
        );
    }
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
        for entry in entries.iter().take(10) {
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
            .find(|entry| entry.user_id == user_id && entry.rank > 10)
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

fn draw_score_board(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    entries: &[HighScoreEntry],
    user_id: Uuid,
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
            "No scores yet this month",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        for entry in entries.iter().take(10) {
            lines.push(ranked_line(
                entry.rank,
                &entry.username,
                i64::from(entry.score),
                "score",
                entry.user_id == user_id,
                inner.width,
            ));
        }
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.user_id == user_id && entry.rank > 10)
        {
            lines.push(Line::from(""));
            lines.push(ranked_line(
                entry.rank,
                &entry.username,
                i64::from(entry.score),
                "score",
                true,
                inner.width,
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
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

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars && max_chars > 1 {
        out.pop();
        out.push('~');
    }
    out
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
