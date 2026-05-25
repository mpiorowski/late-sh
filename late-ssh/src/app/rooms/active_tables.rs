use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{common::theme, dashboard::ui::DashboardRoomCard};

/// Compact multiplayer room summary for lounge surfaces. Shows recent seat joins
/// as one-line jump targets.
pub fn draw_active_tables(
    frame: &mut Frame,
    area: Rect,
    rooms: &[DashboardRoomCard],
    usernames: &HashMap<Uuid, String>,
) {
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
                "recent joins",
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
                "waiting for seat joins",
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
            Paragraph::new(active_table_line(idx, card, usernames, row.width as usize)),
            row,
        );
    }
}

fn active_table_line(
    idx: usize,
    card: &DashboardRoomCard,
    usernames: &HashMap<Uuid, String>,
    width: usize,
) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }

    let hint = format!("b{}", idx + 1);
    let hint_w = hint.chars().count();
    let actor = card
        .recent_join_user_id
        .and_then(|user_id| usernames.get(&user_id))
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| "joined".to_string());
    let actor = truncate_chars(&actor, 12);
    let actor_w = actor.chars().count();
    let status = active_table_status_spans(card, width.saturating_sub(hint_w + actor_w + 3));
    let status_w = span_width(&status);
    let right_w = if status_w > 0 { status_w + 1 } else { 0 };
    let prefix_w = hint_w + 1 + actor_w + 1;
    let label_budget = width.saturating_sub(prefix_w + right_w).max(1);
    let label = if label_budget < 10 {
        truncate_chars(card.game_label, label_budget)
    } else {
        truncate_chars(
            &format!("{} · {}", card.game_label, card.room.display_name),
            label_budget,
        )
    };

    let used_w = prefix_w + label.chars().count() + right_w;
    let gap_w = width.saturating_sub(used_w);

    let mut spans = vec![
        Span::styled(
            hint,
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(actor, Style::default().fg(theme::TEXT_DIM())),
        Span::raw(" "),
        Span::styled(
            label,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if gap_w > 0 {
        spans.push(Span::raw(" ".repeat(gap_w)));
    }
    if status_w > 0 {
        spans.push(Span::raw(" "));
    }
    spans.extend(status);
    Line::from(spans)
}

fn active_table_status_spans(card: &DashboardRoomCard, width: usize) -> Vec<Span<'static>> {
    if width < 3 {
        return Vec::new();
    }

    let occupied = card.occupied_seats.unwrap_or(0);
    let total = card.total_seats;
    let label = format!("{occupied}/{total}");
    if width < 12 {
        return vec![Span::styled(label, Style::default().fg(theme::AMBER()))];
    }

    let mut spans = seat_dot_spans(occupied, total);
    let dot_w = span_width(&spans);
    if width.saturating_sub(dot_w + 1) >= label.chars().count() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label, Style::default().fg(theme::TEXT_DIM())));
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
