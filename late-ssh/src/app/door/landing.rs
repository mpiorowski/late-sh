//! Shared line builders for the door-game landing pages (Lateania, Rebels,
//! NetHack). Keeping the section/stat/action/hint styling in one place stops the
//! three landings from drifting apart, as they had. The rules these encode:
//! amber-bold is for headings only, hint keys are bright-bold, and the action
//! label is full-bright. Per-game flavor (logos, art, glyphs, quotes) stays in
//! each game's own render module.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::common::theme;

/// A landing section heading. Amber-bold is reserved for these.
pub fn heading(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

/// A `label  value` stat row. `pad` is the label column width; each landing sizes
/// it to its own longest label so the value column lines up.
pub fn stat(label: &str, value: &str, pad: usize) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{label:<pad$}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

/// A launch/action row: `marker key  label`, with the marker and key tinted by
/// `color` (e.g. green to go, red to destroy) and the label at full brightness.
pub fn action(marker: &str, key: &str, label: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(color)),
        Span::styled(
            format!("{key:<8}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT())),
    ])
}

/// A `key  label` hint row. `pad` sizes the key column to the landing's longest
/// key so the labels line up.
pub fn hint(key: &str, label: &str, pad: usize) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{key:<pad$}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}
