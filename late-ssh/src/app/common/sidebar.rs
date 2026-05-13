use std::collections::VecDeque;

use chrono::Utc;
use late_core::api_types::NowPlaying;
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
use crate::app::vote::ui::VoteCardView;
use crate::session::ClientAudioState;

pub struct SidebarProps<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub visualizer: &'a Visualizer,
    pub now_playing: Option<&'a NowPlaying>,
    pub paired_client: Option<&'a ClientAudioState>,
    pub online_count: usize,
    pub bonsai: &'a BonsaiState,
    pub audio_beat: f32,
    pub connect_url: &'a str,
    pub activity: &'a VecDeque<ActivityEvent>,
    pub clock_text: &'a str,
    pub vote: Option<VoteCardView<'a>>,
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
    let visualizer = props.visualizer;
    let now_playing = props.now_playing;
    let paired_client = props.paired_client;

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
    //   9 rows stream block (now playing + vote merged)
    //   1 row  ── rule
    //   6 rows active tables (up to 3 rooms × 2 rows; empty state draws its own label)
    //   1 row  ── rule
    //   Fill   bonsai
    let layout = Layout::vertical([
        Constraint::Length(1), // time
        Constraint::Length(1), // ── rule
        Constraint::Length(6), // visualizer
        Constraint::Length(1), // ── rule
        Constraint::Length(9), // stream block
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
    visualizer.render_inline(frame, inset(layout[2]));

    draw_horizontal_rule(frame, inset(layout[3]));

    draw_stream_block(
        frame,
        inset(layout[4]),
        now_playing,
        paired_client,
        props.vote.as_ref(),
    );

    draw_horizontal_rule(frame, inset(layout[5]));

    draw_active_tables(frame, inset(layout[6]), props.top_rooms);

    draw_horizontal_rule(frame, inset(layout[7]));

    crate::app::bonsai::ui::draw_bonsai_inline(
        frame,
        inset(layout[8]),
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
        let room_hint = if idx == 0 {
            Some(active_tables_rooms_hint())
        } else {
            None
        };
        let hint_w = room_hint
            .as_ref()
            .map(|hint| hint.iter().map(|span| span.content.chars().count()).sum())
            .unwrap_or(0);
        let name_budget = inner_w.saturating_sub(hint_w + 1).max(1);
        let name = truncate_chars(&card.room.display_name, name_budget);
        let mut row = vec![Span::styled(
            name,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )];
        if let Some(hint) = room_hint {
            let pad = inner_w.saturating_sub(
                row.iter()
                    .map(|span| span.content.chars().count())
                    .sum::<usize>()
                    + hint_w,
            );
            row.push(Span::raw(" ".repeat(pad)));
            row.extend(hint);
        }
        lines.push(Line::from(row));

        lines.push(active_table_status_line(card, body.width as usize));
    }

    frame.render_widget(Paragraph::new(lines), body);
}

fn active_tables_rooms_hint() -> Vec<Span<'static>> {
    vec![
        Span::styled(
            "3",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" rooms", Style::default().fg(theme::TEXT_DIM())),
    ]
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
        Line::from(vec![key("3"), dim(" rooms")]),
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
    let visible_total = total.max(1).min(6);
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

/// Merged "what's playing + what's next" block. No border, no title bar.
/// Layout (9 rows):
///   track title
///   artist · genre        (dim)
///   <blank>
///   progress bar          (e.g. 0:40 ──●──── 3:00)
///   <blank>
///   "what's next"         (italic dim label)
///   L lofi    ███▒    1
///   A ambient  ·       0
///   C classic  ·       0
fn draw_stream_block(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
    vote: Option<&VoteCardView<'_>>,
) {
    if area.height < 4 {
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // artist
        Constraint::Length(1), // progress
        Constraint::Length(1), // gutter / pair status
        Constraint::Length(1), // "next" label
        Constraint::Fill(1),   // vote rows
    ])
    .split(area);

    let (title, artist) = match now_playing {
        Some(np) => (
            np.track.title.clone(),
            np.track
                .artist
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        None => ("waiting for stream".to_string(), String::new()),
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            artist,
            Style::default().fg(theme::TEXT_DIM()),
        ))),
        chunks[1],
    );

    // Progress
    if let Some(np) = now_playing {
        let elapsed_secs = np.started_at.elapsed().as_secs();
        let duration = np.track.duration_seconds;
        if let Some(dur) = duration {
            let elapsed = elapsed_secs.min(dur);
            let elapsed_str = format!("{}:{:02}", elapsed / 60, elapsed % 60);
            let total_str = format!("{}:{:02}", dur / 60, dur % 60);
            let time_w = elapsed_str.len() + total_str.len() + 2;
            let bar_w = (chunks[2].width as usize).saturating_sub(time_w);
            let progress = if dur > 0 {
                (elapsed as f64 / dur as f64).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dot = ((bar_w as f64 * progress) as usize).min(bar_w.saturating_sub(1));
            let bar_before = "─".repeat(dot);
            let bar_after = "─".repeat(bar_w.saturating_sub(dot + 1));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(elapsed_str, Style::default().fg(theme::AMBER())),
                    Span::raw(" "),
                    Span::styled(bar_before, Style::default().fg(theme::BORDER_DIM())),
                    Span::styled("●", Style::default().fg(theme::AMBER_GLOW())),
                    Span::styled(bar_after, Style::default().fg(theme::BORDER_DIM())),
                    Span::raw(" "),
                    Span::styled(total_str, Style::default().fg(theme::TEXT_FAINT())),
                ])),
                chunks[2],
            );
        }
    }

    // Pair status (dim, one line)
    let pair_text = match paired_client {
        Some(state) => format!(
            "{} · {}%{}",
            state.client_kind.label(),
            state.volume_percent,
            if state.muted { " · muted" } else { "" }
        ),
        None => "no pair".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pair_text,
            Style::default().fg(theme::TEXT_FAINT()),
        ))),
        chunks[3],
    );

    // "next" label
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "what's next",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ))),
        chunks[4],
    );

    // Vote rows (3, borderless, sage for your pick, dim otherwise)
    if let Some(vote) = vote {
        crate::app::vote::ui::draw_vote_inline(frame, chunks[5], vote);
    }
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
