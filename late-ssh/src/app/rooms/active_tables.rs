use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{common::theme, dashboard::ui::DashboardRoomCard};

/// Compact multiplayer room summary for lounge surfaces. Shows up to four hot
/// rooms and points users at the Rooms page in the header.
pub fn draw_active_tables(frame: &mut Frame, area: Rect, rooms: &[DashboardRoomCard]) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let constraints: Vec<Constraint> = (0..area.height).map(|_| Constraint::Length(1)).collect();
    let rows = Layout::vertical(constraints).split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "multiplayer",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled(
                "open [3] Rooms",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    if rows.len() < 2 {
        return;
    }

    if rooms.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no active tables",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            rows[1],
        );
        return;
    }

    for (idx, card) in rooms.iter().take(4).enumerate() {
        let Some(row) = rows.get(idx + 1).copied() else {
            break;
        };
        frame.render_widget(
            Paragraph::new(active_table_line(idx, card, row.width as usize)),
            row,
        );
    }
}

fn active_table_line(idx: usize, card: &DashboardRoomCard, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }

    let hint = format!("b{}", idx + 1);
    let hint_w = hint.chars().count();
    let mut status = active_table_status_spans(card, width.saturating_sub(hint_w + 2));
    let mut status_w = span_width(&status);
    let mut right_w = status_w + if status_w > 0 { 1 } else { 0 } + hint_w;
    if right_w + 2 > width {
        status.clear();
        status_w = 0;
        right_w = hint_w;
    }
    let name_budget = width.saturating_sub(right_w + 1).max(1);
    let name = truncate_chars(&card.room.display_name, name_budget);

    let used_w = name.chars().count() + right_w;
    let gap_w = width.saturating_sub(used_w);

    let mut spans = vec![Span::styled(
        name,
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )];
    if gap_w > 0 {
        spans.push(Span::raw(" ".repeat(gap_w)));
    }
    spans.extend(status);
    if status_w > 0 {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        hint,
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    ));
    Line::from(spans)
}

fn active_table_status_spans(card: &DashboardRoomCard, width: usize) -> Vec<Span<'static>> {
    if width < 6 {
        return Vec::new();
    }

    let occupied = card.occupied_seats.unwrap_or(0);
    let total = card.total_seats;
    if width < 16 {
        return vec![Span::styled(
            format!("{occupied}/{total}"),
            Style::default().fg(theme::AMBER()),
        )];
    }

    let mut spans = seat_dot_spans(occupied, total);
    let dot_w = span_width(&spans);
    let timer_budget = width.saturating_sub(dot_w + 1);
    if timer_budget >= 6 {
        let timer = truncate_chars(&compact_timer_label(&card.pace), timer_budget);
        spans.push(Span::raw(" "));
        spans.push(Span::styled(timer, Style::default().fg(theme::TEXT_DIM())));
    }
    spans
}

fn seat_dot_spans(occupied: usize, total: usize) -> Vec<Span<'static>> {
    let visible_total = total.clamp(1, 6);
    let visible_occupied = occupied.min(visible_total);
    let mut spans = Vec::with_capacity(visible_total);
    for idx in 0..visible_total {
        let symbol = if idx < visible_occupied { "●" } else { "○" };
        spans.push(Span::styled(symbol, Style::default().fg(theme::AMBER())));
    }
    spans
}

fn compact_timer_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        return "waiting".to_string();
    }
    label
        .replace(" action timer", " timer")
        .replace('-', " ")
        .to_string()
}

fn span_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
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
