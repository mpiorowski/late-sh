//! Profile Badges tab.
//!
//! There is no achievements/badges system yet. This module defines the badge
//! shape and renders a placeholder grid so the layout is ready for a real
//! source to populate later (each badge will carry a glyph, name, earned date,
//! and what it was awarded for). Users are expected to accumulate many of
//! these over time, hence the dedicated tab.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::common::theme;

/// A single earned badge. Populated by a future achievements system.
#[derive(Clone, Debug)]
pub(crate) struct Badge {
    pub glyph: String,
    pub name: String,
    /// `YYYY-MM-DD` the badge was earned.
    pub earned: String,
    /// What the badge was awarded for.
    pub description: String,
}

/// Badges for a user. No backing system yet, so always empty for now.
pub(crate) fn badges_for(_user_id: Uuid) -> Vec<Badge> {
    Vec::new()
}

const CELL_W: usize = 24;

pub(crate) fn draw(frame: &mut Frame, area: Rect, badges: &[Badge], scroll: u16) {
    if area.width < 14 || area.height < 2 {
        return;
    }
    if badges.is_empty() {
        draw_placeholder(frame, area);
        return;
    }
    draw_grid(frame, area, badges, scroll);
}

/// Future state: a scrollable grid of earned badges, newest first.
fn draw_grid(frame: &mut Frame, area: Rect, badges: &[Badge], scroll: u16) {
    let cols = (area.width as usize / CELL_W).max(1);
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut lines: Vec<Line> = Vec::new();
    for row in badges.chunks(cols) {
        let mut name_spans = Vec::new();
        let mut date_spans = Vec::new();
        let mut desc_spans = Vec::new();
        for badge in row {
            name_spans.push(Span::styled(
                pad(&format!("{} {}", badge.glyph, badge.name), CELL_W),
                accent,
            ));
            date_spans.push(Span::styled(
                pad(&format!("  {}", badge.earned), CELL_W),
                dim,
            ));
            desc_spans.push(Span::styled(
                pad(&format!("  {}", badge.description), CELL_W),
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

/// Current state: empty shelf of dim placeholder slots. In a short strip (the
/// dashboard) it is literally two rows of slots; given more room (the Badges
/// tab) it adds the "this will fill up" caption.
fn draw_placeholder(frame: &mut Frame, area: Rect) {
    let slot = Style::default().fg(theme::BORDER());
    let cols = (area.width as usize / 8).clamp(3, 8);
    let height = area.height as usize;
    let show_caption = height >= 5;
    let slot_rows = if show_caption { 2 } else { height.max(1) };

    let mut content: Vec<Line> = Vec::new();
    for _ in 0..slot_rows {
        let mut spans = Vec::new();
        for _ in 0..cols {
            spans.push(Span::styled("⬡", slot));
            spans.push(Span::raw("     "));
        }
        content.push(Line::from(spans).centered());
    }

    if show_caption {
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
                "achievements you earn will appear here, with the date and what they were for",
                Style::default().fg(theme::TEXT_DIM()),
            ))
            .centered(),
        );
    }

    let top_pad = height.saturating_sub(content.len()) / 2;
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
