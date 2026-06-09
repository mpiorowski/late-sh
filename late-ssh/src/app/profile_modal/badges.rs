//! Earned-award badges for the profile modal.
//!
//! Profile awards are stored permanently. The overview shows a short preview;
//! the compact Badges tab shows the scrollable award shelf.

use late_core::models::profile_award::ProfileAward;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

pub(crate) const PREVIEW_LIMIT: usize = 6;
const CELL_W: usize = 28;

pub(crate) fn preview_lines(awards: &[ProfileAward]) -> Vec<Line<'static>> {
    if awards.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let badge_style = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut spans = Vec::new();
    for award in awards.iter().take(PREVIEW_LIMIT) {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("[{} {}]", award.badge(), award.month_label()),
            badge_style,
        ));
    }

    let remaining = awards.len().saturating_sub(PREVIEW_LIMIT);
    if remaining > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!("+{remaining} more"), dim));
    }

    lines.push(Line::from(spans));
    lines
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, awards: &[ProfileAward], scroll: u16) {
    if area.width < 14 || area.height < 4 {
        return;
    }
    if awards.is_empty() {
        draw_placeholder(frame, area);
        return;
    }
    draw_grid(frame, area, awards, scroll);
}

fn draw_grid(frame: &mut Frame, area: Rect, awards: &[ProfileAward], scroll: u16) {
    let cols = (area.width as usize / CELL_W).max(1);
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut lines: Vec<Line> = Vec::new();
    for row in awards.chunks(cols) {
        let mut name_spans = Vec::new();
        let mut date_spans = Vec::new();
        let mut desc_spans = Vec::new();
        for award in row {
            name_spans.push(Span::styled(
                pad(&format!("{} {}", award.badge(), award.label()), CELL_W),
                accent,
            ));
            date_spans.push(Span::styled(
                pad(&format!("  {}", award.month_label()), CELL_W),
                dim,
            ));
            desc_spans.push(Span::styled(
                pad(&format!("  {}", award.description()), CELL_W),
                dim,
            ));
        }
        lines.push(Line::from(name_spans));
        lines.push(Line::from(date_spans));
        lines.push(Line::from(desc_spans));
        lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
}

fn draw_placeholder(frame: &mut Frame, area: Rect) {
    let slot = Style::default().fg(theme::BORDER());
    let cols = (area.width as usize / 8).clamp(3, 6);
    let rows = 2usize;

    let mut content: Vec<Line> = Vec::new();
    for _ in 0..rows {
        let mut spans = Vec::new();
        for _ in 0..cols {
            spans.push(Span::styled("⬡", slot));
            spans.push(Span::raw("     "));
        }
        content.push(Line::from(spans).centered());
        content.push(Line::from(""));
    }
    content.push(Line::from(""));
    content.push(
        Line::from(Span::styled(
            "No badges yet",
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::BOLD),
        ))
        .centered(),
    );
    content.push(
        Line::from(Span::styled(
            "monthly leaderboard awards will appear here",
            Style::default().fg(theme::TEXT_DIM()),
        ))
        .centered(),
    );

    let top_pad = (area.height as usize).saturating_sub(content.len()) / 2;
    let mut lines = vec![Line::from(""); top_pad];
    lines.extend(content);
    frame.render_widget(Paragraph::new(lines), area);
}

fn pad(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count >= width {
        let keep = width.saturating_sub(1);
        format!("{}…", text.chars().take(keep).collect::<String>())
    } else {
        format!("{text}{}", " ".repeat(width - count))
    }
}
