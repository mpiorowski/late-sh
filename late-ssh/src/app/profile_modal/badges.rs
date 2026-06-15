//! Compact earned-award preview for profile overview.
//!
//! Profile awards are stored permanently, but the overview intentionally shows
//! only a short preview so the profile still reads quickly.

use late_core::models::profile_award::ProfileAward;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;

pub(crate) const PREVIEW_LIMIT: usize = 6;

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
