use late_core::models::user::{AudioSource, RadioSlots, RadioStation};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::common::theme;

use super::state::StationsModalState;

const MODAL_WIDTH: u16 = 76;
/// Rows the chrome takes around the list: border (2), breathing (1),
/// pinned row (1), breathing (1), list heading (1), breathing (1), footer (1).
const CHROME_ROWS: u16 = 8;
const LABEL_COLUMN: usize = 12;
const PROVIDER_COLUMN: usize = 13;

/// One catalogue row as the modal paints it.
pub(crate) struct StationRow {
    pub station: RadioStation,
    /// Live `Artist - Title` from the provider's feed, `None` while absent.
    pub now_playing: Option<String>,
}

pub(crate) struct StationsView<'a> {
    pub rows: &'a [StationRow],
    pub current: RadioStation,
    pub slots: RadioSlots,
    pub source: AudioSource,
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &StationsModalState,
    view: &StationsView<'_>,
) {
    let wanted_height = CHROME_ROWS + view.rows.len().max(1) as u16;
    let popup = centered_rect(
        area,
        MODAL_WIDTH.min(area.width),
        wanted_height.min(area.height),
    );
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Stations ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 6 || inner.width < 40 {
        frame.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing
        Constraint::Length(1), // pinned slots
        Constraint::Length(1), // breathing
        Constraint::Length(1), // list heading / filter
        Constraint::Length(1), // breathing
        Constraint::Min(1),    // list
        Constraint::Length(1), // footer
    ])
    .split(inner);

    frame.render_widget(Paragraph::new(pinned_line(view.slots)), layout[1]);
    frame.render_widget(
        Paragraph::new(heading_line(state.filter_query(), state.filter_active())),
        layout[3],
    );
    draw_list(frame, layout[5], state, view);
    frame.render_widget(
        Paragraph::new(footer_line(state.filter_active())),
        layout[6],
    );
}

/// `  pinned   v1 chillsynth   v2 nightride   v3 —   v4 classical`
fn pinned_line(slots: RadioSlots) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "  pinned   ",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    )];
    for (index, slot) in slots.iter().enumerate() {
        spans.push(Span::styled(
            format!("v{}", index + 1),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        match slot {
            Some(station) => spans.push(Span::styled(
                station.label().to_string(),
                Style::default().fg(theme::TEXT()),
            )),
            None => spans.push(Span::styled(
                "—".to_string(),
                Style::default().fg(theme::TEXT_FAINT()),
            )),
        }
        spans.push(Span::raw("   "));
    }
    Line::from(spans)
}

fn heading_line(query: &str, filter_active: bool) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    if query.is_empty() && !filter_active {
        return Line::from(vec![
            Span::styled("  ── ", dim),
            Span::styled("All stations", accent),
            Span::styled(" ──", dim),
        ]);
    }
    let mut spans = vec![
        Span::styled("  ── ", dim),
        Span::styled("Filter ", accent),
        Span::styled("/", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(query.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
    ];
    if filter_active {
        spans.push(Span::styled("▏", Style::default().fg(theme::AMBER_GLOW())));
    }
    spans.push(Span::styled(" ──", dim));
    Line::from(spans)
}

fn draw_list(frame: &mut Frame, area: Rect, state: &StationsModalState, view: &StationsView<'_>) {
    if area.height == 0 {
        return;
    }
    if view.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "    no station matches",
                Style::default().fg(theme::TEXT_FAINT()),
            ))),
            area,
        );
        return;
    }
    let height = area.height as usize;
    // Keep the cursor in view: scroll so the selected row is inside the window.
    let first = state
        .selected()
        .saturating_sub(height.saturating_sub(1))
        .min(view.rows.len().saturating_sub(height));
    let lines: Vec<Line<'static>> = view
        .rows
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, row)| station_line(row, view, index == state.selected(), area.width as usize))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// `▸ ● chillsynth   nightride     Artist - Title                    v1`
fn station_line(
    row: &StationRow,
    view: &StationsView<'_>,
    highlighted: bool,
    width: usize,
) -> Line<'static> {
    let station = row.station;
    let is_current = station == view.current;
    let listening = is_current && view.source == AudioSource::Radio;
    let slot = view.slots.position_of(station);

    let cursor = if highlighted { "▸ " } else { "  " };
    let (glyph, glyph_style) = if listening {
        ("●", Style::default().fg(theme::AMBER_GLOW()))
    } else {
        ("○", Style::default().fg(theme::BORDER_DIM()))
    };
    let label_style = if highlighted {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else if is_current {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let key_text = slot
        .map(|index| format!("v{}", index + 1))
        .unwrap_or_default();
    let key_width = 3;
    let fixed = 2 + 2 + LABEL_COLUMN + PROVIDER_COLUMN + key_width + 1;
    let track_budget = width.saturating_sub(fixed);
    let (track_text, track_style) = match &row.now_playing {
        Some(track) => (
            truncate_to_width(track, track_budget),
            Style::default().fg(if highlighted {
                theme::TEXT()
            } else {
                theme::TEXT_DIM()
            }),
        ),
        None => (
            truncate_to_width("no signal", track_budget),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ),
    };
    let pad = track_budget.saturating_sub(track_text.width());

    Line::from(vec![
        Span::styled(cursor.to_string(), Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(glyph.to_string(), glyph_style),
        Span::raw(" "),
        Span::styled(pad_right(station.label(), LABEL_COLUMN), label_style),
        Span::styled(
            pad_right(station.provider().label(), PROVIDER_COLUMN),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(track_text, track_style),
        Span::raw(" ".repeat(pad + 1)),
        Span::styled(
            format!("{key_text:>key_width$}"),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn footer_line(filter_active: bool) -> Line<'static> {
    let key = Style::default().fg(theme::AMBER_DIM());
    let label = Style::default().fg(theme::TEXT_DIM());
    if filter_active {
        return Line::from(vec![
            Span::raw("  "),
            Span::styled("type", key),
            Span::styled(" to filter  ", label),
            Span::styled("↵", key),
            Span::styled(" keep  ", label),
            Span::styled("Esc", key),
            Span::styled(" clear", label),
        ]);
    }
    Line::from(vec![
        Span::raw("  "),
        Span::styled("↑↓", key),
        Span::styled(" move  ", label),
        Span::styled("↵", key),
        Span::styled(" listen  ", label),
        Span::styled("1-4", key),
        Span::styled(" pin  ", label),
        Span::styled("0", key),
        Span::styled(" unpin  ", label),
        Span::styled("/", key),
        Span::styled(" filter  ", label),
        Span::styled("Esc", key),
        Span::styled(" close", label),
    ])
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn pad_right(text: &str, width: usize) -> String {
    let text = truncate_to_width(text, width.saturating_sub(1));
    let pad = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(pad))
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".repeat(width.min(1));
    }
    let mut out = String::new();
    for ch in text.chars() {
        let next = format!("{out}{ch}");
        if next.width() > width - 1 {
            break;
        }
        out = next;
    }
    format!("{out}…")
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
