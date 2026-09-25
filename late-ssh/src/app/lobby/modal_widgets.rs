//! The small pieces every panel of the Lobby modal is built from: a wrapped
//! blurb, a truncated cell, a selection marker, a section rule, the two span
//! styles that make a key look like a key, and the centring maths.
//!
//! They live apart from the panels because there is more than one panel now —
//! the modal draws the daily games and the house tables, `realm/modal_ui.rs`
//! draws the realm row and its overlays — and a shared widget owned by one of
//! them would have the other reaching sideways into a sibling.

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::common::theme;

pub(super) fn wrap_blurb(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub(super) fn truncate_text(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

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
