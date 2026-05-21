use late_core::models::chips::difficulty_bonus;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

pub fn draw(frame: &mut Frame, area: Rect) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // summary
        Constraint::Length(1), // breathing
        Constraint::Min(0),    // body
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Guide")), sections[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "How chips, monthly boards, and score boards work.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[3]);
    frame.render_widget(Paragraph::new(chip_lines()), columns[0]);
    frame.render_widget(Paragraph::new(leaderboard_lines()), columns[1]);
}

fn chip_lines() -> Vec<Line<'static>> {
    vec![
        section_heading("Earn Chips"),
        text("  New accounts start with 1,000 chips."),
        text("  Daily puzzle wins pay once per daily board:"),
        payout("easy", difficulty_bonus("easy")),
        payout("medium", difficulty_bonus("medium")),
        payout("hard", difficulty_bonus("hard")),
        text("  Solitaire draw-1 pays easy; draw-3 pays hard."),
        text(&format!(
            "  Bonsai watering pays {} chips once per day.",
            crate::app::bonsai::svc::WATER_CHIP_BONUS
        )),
        spacer(),
        section_heading("Rooms"),
        text("  Blackjack and Poker move chips through bets and pots."),
        text("  Table losses restore you to the 100-chip floor."),
        text("  Tic-Tac-Toe is for activity, not chip payout."),
        spacer(),
        section_heading("Top Chips"),
        text("  Monthly Top Chips counts positive earnings only."),
        text("  Spending chips does not lower your monthly rank."),
        text("  Floor restores are excluded from the board."),
    ]
}

fn leaderboard_lines() -> Vec<Line<'static>> {
    vec![
        section_heading("Arcade Wins"),
        text("  Counts daily Sudoku, Nonogram, Solitaire, Minesweeper."),
        text("  Each completed daily adds monthly points:"),
        points("easy / draw-1", 1),
        points("medium", 3),
        points("hard / draw-3", 5),
        text("  More hard dailies across more games wins the board."),
        spacer(),
        section_heading("Score Games"),
        text("  Tetris, 2048, and Snake use final run score."),
        text("  Monthly boards use score events this month."),
        text("  All-time boards use each user's saved best score."),
        spacer(),
        section_heading("Timing"),
        text("  Monthly boards reset on the 1st, UTC."),
        text("  All-time score boards persist."),
        text("  Hub refreshes from the server about every 30 seconds."),
    ]
}

fn payout(label: &str, chips: i64) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(format!("{label:<8}"), Style::default().fg(theme::TEXT())),
        Span::styled(
            format!("{chips:>4} chips"),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn points(label: &str, value: i64) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(format!("{label:<14}"), Style::default().fg(theme::TEXT())),
        Span::styled(
            format!("{value} pts"),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn text(value: &str) -> Line<'static> {
    Line::from(Span::styled(
        value.to_string(),
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

fn spacer() -> Line<'static> {
    Line::from("")
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
