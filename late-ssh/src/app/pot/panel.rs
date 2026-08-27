//! Right-sidebar Pot panel: passive, two rows, stable chrome. The rail is 24
//! columns wide, so every row is a left value and a right value with the pad
//! between them, and both are truncated rather than allowed to wrap.
//!
//! All interaction lives in the composer (`/pot`, `/pot buy N`); this panel
//! never takes input.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::{primitives::thousands, theme};

use super::state::PotView;

/// The size row and the tickets row. The sidebar's labeled separator rule
/// (`── pot ────`) is the title, so the panel spends no row on a name.
pub(crate) const POT_PANEL_HEIGHT: u16 = 2;

pub(crate) fn draw_pot_inline(frame: &mut Frame, area: Rect, view: &PotView) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Paragraph::new(pot_panel_lines(area.width, view)), area);
}

fn pot_panel_lines(width: u16, view: &PotView) -> Vec<Line<'static>> {
    if !view.open {
        // Before the first refresh, and in a process with no pot service:
        // dashes, so the panel keeps its shape instead of claiming a pot of
        // zero that nobody can buy into.
        return vec![empty_row(), empty_row()];
    }
    vec![
        // The prize, and how long is left to get into it.
        split_row(
            width,
            thousands(view.size),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
            format!("in {}", view.draws_in),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        // The field, and your place in it.
        split_row(
            width,
            format!("{} tickets", thousands(view.ticket_count)),
            Style::default().fg(theme::TEXT_DIM()),
            format!("you {}", thousands(view.my_tickets)),
            match view.my_tickets {
                0 => Style::default().fg(theme::TEXT_FAINT()),
                _ => Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            },
        ),
    ]
}

/// One row: `left` against the rail's left edge, `right` against its right,
/// at least one space between them. The right value is the smaller of the
/// two and never truncates; the left gives up the columns.
fn split_row(
    width: u16,
    left: String,
    left_style: Style,
    right: String,
    right_style: Style,
) -> Line<'static> {
    let right_w = right.chars().count();
    let left_budget = (width as usize).saturating_sub(right_w + 1);
    let left = truncate_chars(&left, left_budget);
    let pad = (width as usize)
        .saturating_sub(left.chars().count() + right_w)
        .max(1);
    Line::from(vec![
        Span::styled(left, left_style),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, right_style),
    ])
}

fn empty_row() -> Line<'static> {
    Line::from(Span::styled(
        "  ─",
        Style::default().fg(theme::BORDER_DIM()),
    ))
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "panel_test.rs"]
mod panel_test;
