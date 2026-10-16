//! The small pieces every panel of the Lobby modal is built from: a padded
//! cell, a selection marker, a section rule, the two span styles that make a
//! key look like a key, and the centring maths.
//!
//! They live apart from the panels so that every panel of the modal draws
//! with the same pieces instead of one panel reaching sideways into another.

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::common::theme;

pub(super) fn col(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < width {
        return format!("{text:<width$}");
    }
    let mut out: String = chars.into_iter().take(width.saturating_sub(2)).collect();
    out.push('…');
    out.push(' ');
    out
}

pub(super) fn marker_span(selected: bool) -> Span<'static> {
    if selected {
        Span::styled(
            "► ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  ")
    }
}

pub(super) fn section_line(width: usize, label: &str) -> Line<'static> {
    let used = 3 + label.chars().count() + 1;
    let trail = width.saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled("── ".to_string(), Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ),
        Span::raw(" "),
        Span::styled("─".repeat(trail), Style::default().fg(theme::BORDER_DIM())),
    ])
}

pub(super) fn empty_line(message: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {message}"),
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

pub(super) fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

pub(super) fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
}

pub(super) fn gap() -> Span<'static> {
    Span::raw("   ")
}

pub(super) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

#[cfg(test)]
#[path = "modal_widgets_test.rs"]
mod modal_widgets_test;
