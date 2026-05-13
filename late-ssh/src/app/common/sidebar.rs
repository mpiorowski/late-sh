use std::collections::VecDeque;

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
use crate::app::activity::event::ActivityEvent;
use crate::app::bonsai::state::BonsaiState;
use crate::app::dashboard::ui::DashboardRoomCard;
use crate::app::visualizer::Visualizer;

pub struct SidebarProps<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub visualizer: &'a Visualizer,
    pub online_count: usize,
    pub bonsai: &'a BonsaiState,
    pub audio_beat: f32,
    pub connect_url: &'a str,
    pub activity: &'a VecDeque<ActivityEvent>,
    pub clock_text: &'a str,
    /// Top multiplayer rooms — rendered as a compact "active tables" block
    /// in the right rail.
    pub top_rooms: &'a [DashboardRoomCard],
}

pub fn draw_sidebar(frame: &mut Frame, area: Rect, props: &SidebarProps<'_>) {
    draw_sidebar_new_shell(frame, area, props);
}

fn draw_sidebar_new_shell(frame: &mut Frame, area: Rect, props: &SidebarProps<'_>) {
    // Single thin separator on the LEFT edge anchors the rail; sections inside
    // breathe without their own borders. Italic dim labels mark each block.
    // Paint the separator column first so content rendering overdraws nothing.
    paint_vertical_separator(frame, area.x, area.y, area.height);

    // Shrink the working area to skip the separator column + 1 col padding.
    let area = Rect {
        x: area.x + 2,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };

    // Vertical real estate, top to bottom:
    //   1 row  time (centered)
    //   1 row  ── rule
    //   6 rows visualizer (borderless)
    //   1 row  ── rule
    //   6 rows active tables (up to 3 rooms × 2 rows; empty state draws its own label)
    //   1 row  ── rule
    //   Fill   bonsai
    let layout = Layout::vertical([
        Constraint::Length(1), // time
        Constraint::Length(1), // ── rule
        Constraint::Length(6), // visualizer
        Constraint::Length(1), // ── rule
        Constraint::Length(6), // active tables
        Constraint::Length(1), // ── rule
        Constraint::Fill(1),   // bonsai
    ])
    .split(area);

    // Inset content one column from the right so it doesn't kiss the frame.
    let inset = |r: Rect| -> Rect {
        Rect {
            x: r.x,
            y: r.y,
            width: r.width.saturating_sub(1),
            height: r.height,
        }
    };

    // Time: right-aligned in the top row.
    draw_time_top(frame, inset(layout[0]), props.clock_text);
    draw_horizontal_rule(frame, inset(layout[1]));

    // Visualizer: borderless inline render.
    props.visualizer.render_inline(frame, inset(layout[2]));

    draw_horizontal_rule(frame, inset(layout[3]));

    draw_active_tables(frame, inset(layout[4]), props.top_rooms);

    draw_horizontal_rule(frame, inset(layout[5]));

    crate::app::bonsai::ui::draw_bonsai_inline(
        frame,
        inset(layout[6]),
        props.bonsai,
        props.audio_beat,
    );
}

/// Compact active tables panel for the right rail. Shows up to 3 busy rooms,
/// 2 rows each: name, then seat dots + timer.
fn draw_active_tables(frame: &mut Frame, area: Rect, rooms: &[DashboardRoomCard]) {
    if area.width == 0 || area.height < 2 {
        return;
    }

    if rooms.is_empty() {
        let chunks = Layout::vertical([
            Constraint::Length(1), // empty-state label
            Constraint::Fill(1),   // hints
        ])
        .split(area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "multiplayer",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            chunks[0],
        );
        draw_empty_active_tables(frame, chunks[1]);
        return;
    }

    let body = area;
    let rows_per_room: u16 = 2;
    let max_rooms = ((body.height / rows_per_room) as usize).min(3);
    let visible_rooms = rooms.iter().take(max_rooms.max(1));

    let mut lines: Vec<Line<'_>> = Vec::new();
    for (idx, card) in visible_rooms.enumerate() {
        let inner_w = body.width as usize;
        let room_hint = active_tables_room_hint(idx);
        let hint_w = room_hint
            .iter()
            .map(|span| span.content.chars().count())
            .sum::<usize>();
        let name_budget = inner_w.saturating_sub(hint_w + 1).max(1);
        let name = truncate_chars(&card.room.display_name, name_budget);
        let mut row = vec![Span::styled(
            name,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )];
        let pad = inner_w.saturating_sub(
            row.iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>()
                + hint_w,
        );
        row.push(Span::raw(" ".repeat(pad)));
        row.extend(room_hint);
        lines.push(Line::from(row));

        lines.push(active_table_status_line(card, body.width as usize));
    }

    frame.render_widget(Paragraph::new(lines), body);
}

fn active_tables_room_hint(idx: usize) -> Vec<Span<'static>> {
    vec![Span::styled(
        format!("b{}", idx + 1),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )]
}

fn draw_empty_active_tables(frame: &mut Frame, area: Rect) {
    let key = |text: &str| -> Span<'static> {
        Span::styled(
            text.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        )
    };
    let dim = |text: &str| -> Span<'static> {
        Span::styled(text.to_string(), Style::default().fg(theme::TEXT_DIM()))
    };
    let faint = |text: &str| -> Span<'static> {
        Span::styled(
            text.to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )
    };

    let lines = vec![
        Line::from(faint("no active tables")),
        Line::from(vec![key("b1"), dim("/"), key("b2"), dim("/"), key("b3")]),
        Line::from(vec![key("n"), dim(" create table")]),
        Line::from(vec![key("Enter"), dim(" join")]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn active_table_status_line(card: &DashboardRoomCard, width: usize) -> Line<'static> {
    let occupied = card.occupied_seats.unwrap_or(0);
    let total = card.total_seats;
    let dots = seat_dot_spans(occupied, total);
    let dot_width = total.min(6);
    let timer = compact_timer_label(&card.pace);
    let timer_budget = width.saturating_sub(dot_width + 1);
    let timer = truncate_chars(&timer, timer_budget);

    let mut spans = dots;
    if !timer.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(timer, Style::default().fg(theme::TEXT_DIM())));
    }
    Line::from(spans)
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

/// Top-of-rail time. Centered, `◷` clock glyph in dim amber, optional timezone
/// label dimmed, time digits bold amber. Mirrors the classic sidebar clock.
fn draw_time_top(frame: &mut Frame, area: Rect, clock_text: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut parts = clock_text.rsplitn(2, ' ');
    let time = parts.next().unwrap_or(clock_text);
    let label = parts.next();

    // Native `⊙` (U+2299 circled dot operator). Reliably mono across terminals,
    // reads as a small clock face without competing with the digits.
    let mut spans: Vec<Span<'static>> =
        vec![Span::styled("⊙ ", Style::default().fg(theme::AMBER_DIM()))];
    spans.push(Span::styled(
        time.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(label) = label {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).centered(), area);
}

fn draw_horizontal_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(theme::BORDER_DIM()),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

/// Paint a thin vertical line (1 column wide) in BORDER_DIM. Used by the
/// merged shell to anchor left/right rails without wrapping them in a box.
pub fn paint_vertical_separator(frame: &mut Frame, x: u16, y: u16, height: u16) {
    let buf = frame.buffer_mut();
    for dy in 0..height {
        if let Some(cell) = buf.cell_mut((x, y + dy)) {
            cell.set_symbol("│").set_fg(theme::BORDER_DIM());
        }
    }
}

pub fn sidebar_clock_text(timezone: Option<&str>) -> String {
    crate::app::common::time::timezone_current_time(Utc::now(), timezone)
        .unwrap_or_else(|| Utc::now().format("UTC %H:%M").to_string())
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
mod tests {
    use super::*;

    #[test]
    fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
        let clock = sidebar_clock_text(None);
        assert!(clock.starts_with("UTC "));
    }
}
