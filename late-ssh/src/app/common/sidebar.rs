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
    pub vote: VoteCardView<'a>,
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

    const TIME_HEIGHT: u16 = 1;
    const RULE_HEIGHT: u16 = 1;
    const VISUALIZER_HEIGHT: u16 = 6;
    const NOW_PLAYING_HEIGHT: u16 = 9;
    const ACTIVE_TABLES_HEIGHT: u16 = 6;
    const BONSAI_MIN_HEIGHT: u16 = 3;

    let fixed_without_active = TIME_HEIGHT
        + RULE_HEIGHT
        + VISUALIZER_HEIGHT
        + RULE_HEIGHT
        + NOW_PLAYING_HEIGHT
        + RULE_HEIGHT;
    let active_tables_budget = ACTIVE_TABLES_HEIGHT + RULE_HEIGHT;
    let show_active_tables =
        fixed_without_active + active_tables_budget + BONSAI_MIN_HEIGHT <= area.height;

    // Vertical real estate, top to bottom. Active tables are lower priority
    // than bonsai: hide them before squeezing the tree below its visible size.
    let mut constraints = vec![
        Constraint::Length(TIME_HEIGHT),        // time
        Constraint::Length(RULE_HEIGHT),        // ── rule
        Constraint::Length(VISUALIZER_HEIGHT),  // visualizer
        Constraint::Length(RULE_HEIGHT),        // ── rule
        Constraint::Length(NOW_PLAYING_HEIGHT), // now playing + vote
        Constraint::Length(RULE_HEIGHT),        // ── rule
    ];
    if show_active_tables {
        constraints.push(Constraint::Length(ACTIVE_TABLES_HEIGHT)); // active tables
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
    }
    constraints.push(Constraint::Fill(1)); // bonsai

    let layout = Layout::vertical(constraints).split(area);

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

    draw_now_playing_block(
        frame,
        inset(layout[4]),
        props.now_playing,
        props.paired_client,
        &props.vote,
    );

    draw_horizontal_rule(frame, inset(layout[5]));

    let mut bonsai_idx = 6;
    if show_active_tables {
        draw_active_tables(frame, inset(layout[6]), props.top_rooms);
        draw_horizontal_rule(frame, inset(layout[7]));
        bonsai_idx = 8;
    }
    crate::app::bonsai::ui::draw_bonsai_inline(
        frame,
        inset(layout[bonsai_idx]),
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

fn draw_now_playing_block(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
    vote: &VoteCardView<'_>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // artist
        Constraint::Length(1), // progress
        Constraint::Length(1), // pair status
        Constraint::Length(1), // controls
        Constraint::Length(1), // now/next vibe
        Constraint::Fill(1),   // vote rows
    ])
    .split(area);

    let (title, artist) = match now_playing {
        Some(np) => (
            truncate_chars(&np.track.title, area.width as usize),
            truncate_chars(
                np.track.artist.as_deref().unwrap_or("unknown"),
                area.width as usize,
            ),
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
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            artist,
            Style::default().fg(theme::TEXT_DIM()),
        ))),
        rows[1],
    );

    if let Some(np) = now_playing {
        let elapsed = np.started_at.elapsed().as_secs();
        if let Some(dur) = np.track.duration_seconds {
            draw_progress_line(frame, rows[2], elapsed, dur);
        } else {
            draw_elapsed_line(frame, rows[2], elapsed);
        }
    }

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
            truncate_chars(&pair_text, rows[3].width as usize),
            Style::default().fg(theme::TEXT_FAINT()),
        ))),
        rows[3],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "-/=",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" vol  ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(
                "m",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" mute", Style::default().fg(theme::TEXT_FAINT())),
        ])),
        rows[4],
    );

    draw_vibe_line(frame, rows[5], vote);
    crate::app::vote::ui::draw_vote_inline(frame, rows[6], vote);
}

fn draw_vibe_line(frame: &mut Frame, area: Rect, vote: &VoteCardView<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(Paragraph::new(vote_vibe_line(vote)), area);
}

fn vote_vibe_line(vote: &VoteCardView<'_>) -> Line<'static> {
    let next = vote.vote_counts.winner_or(vote.current_genre);
    let current =
        crate::app::common::primitives::genre_label(vote.current_genre).to_ascii_lowercase();
    let next = crate::app::common::primitives::genre_label(next).to_ascii_lowercase();
    let ends = compact_vote_duration(vote.ends_in);

    Line::from(vec![
        Span::styled(current, Style::default().fg(theme::SUCCESS())),
        Span::styled(" > ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(next, Style::default().fg(theme::AMBER())),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(ends, Style::default().fg(theme::AMBER_DIM())),
    ])
}

fn compact_vote_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs == 0 {
        return "now".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let minutes = secs.div_ceil(60);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    let mins = minutes % 60;
    if mins == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h{mins:02}")
    }
}

fn draw_progress_line(frame: &mut Frame, area: Rect, elapsed_secs: u64, duration_secs: u64) {
    if area.width == 0 || duration_secs == 0 {
        return;
    }
    let elapsed = elapsed_secs.min(duration_secs);
    let elapsed_str = format!("{}:{:02}", elapsed / 60, elapsed % 60);
    let total_str = format!("{}:{:02}", duration_secs / 60, duration_secs % 60);
    let time_w = elapsed_str.len() + total_str.len() + 2;
    let bar_w = (area.width as usize).saturating_sub(time_w);
    if bar_w == 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                elapsed_str,
                Style::default().fg(theme::AMBER()),
            ))),
            area,
        );
        return;
    }

    let progress = (elapsed as f64 / duration_secs as f64).clamp(0.0, 1.0);
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
        area,
    );
}

fn draw_elapsed_line(frame: &mut Frame, area: Rect, elapsed_secs: u64) {
    if area.width == 0 {
        return;
    }
    let elapsed = format!("{}:{:02}", elapsed_secs / 60, elapsed_secs % 60);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(elapsed, Style::default().fg(theme::AMBER())),
            Span::styled(" live", Style::default().fg(theme::TEXT_FAINT())),
        ])),
        area,
    );
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
    use crate::app::vote::svc::{Genre, VoteCount};
    use std::time::Duration;

    #[test]
    fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
        let clock = sidebar_clock_text(None);
        assert!(clock.starts_with("UTC "));
    }

    #[test]
    fn compact_vote_duration_rounds_remaining_minutes_up() {
        assert_eq!(compact_vote_duration(Duration::from_secs(0)), "now");
        assert_eq!(compact_vote_duration(Duration::from_secs(42)), "42s");
        assert_eq!(compact_vote_duration(Duration::from_secs(61)), "2m");
        assert_eq!(compact_vote_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(compact_vote_duration(Duration::from_secs(3661)), "1h02");
    }

    #[test]
    fn vote_vibe_line_includes_vote_end_time() {
        let counts = VoteCount {
            lofi: 1,
            ambient: 3,
            classic: 0,
            jazz: 0,
        };
        let view = VoteCardView {
            vote_counts: &counts,
            current_genre: Genre::Lofi,
            my_vote: None,
            ends_in: Duration::from_secs(9 * 60),
        };

        let text: String = vote_vibe_line(&view)
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(text, "lofi > ambient · 9m");
    }
}
