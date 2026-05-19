use std::{collections::HashMap, time::Instant};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    arcade::ui::{draw_game_frame_with_info_sidebar, info_label_value, info_tagline, key_hint},
    common::theme,
    rooms::chess::{
        state::{ChessColor, ChessGameResult, ChessPhase, State, piece_label},
        svc::{ChessClockSnapshot, ChessPiece},
    },
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    if area.height < 10 || area.width < 32 {
        frame.render_widget(Paragraph::new("Chess board needs more room."), area);
        return;
    }

    let show_sidebar = area.width >= 72;
    let info_lines = info_lines(state, usernames);
    let content = draw_game_frame_with_info_sidebar(frame, area, "Chess", info_lines, show_sidebar);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(content);

    frame.render_widget(
        Paragraph::new(status_line(state)).alignment(Alignment::Center),
        rows[0],
    );
    draw_board(frame, rows[1], state);
    frame.render_widget(
        Paragraph::new(key_line(state)).alignment(Alignment::Center),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(clock_line(state)).alignment(Alignment::Center),
        rows[3],
    );
}

fn draw_board(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let orientation = state.orienting_color();
    let legal_targets = state.legal_targets();
    let selected = state.selected();
    let cell_width: usize = if area.width >= 42 { 5 } else { 3 };
    let board_width = ((cell_width as u16) * 8 + 4).min(area.width);
    let board_height = 10.min(area.height);
    let board_area = Rect {
        x: area.x + area.width.saturating_sub(board_width) / 2,
        y: area.y + area.height.saturating_sub(board_height) / 2,
        width: board_width,
        height: board_height,
    };

    let mut lines = Vec::with_capacity(10);
    let file_labels = file_label_line(orientation, cell_width);
    lines.push(file_labels.clone());
    for display_row in 0..8 {
        let rank = match orientation {
            ChessColor::White => 7 - display_row,
            ChessColor::Black => display_row,
        };
        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!("{} ", rank + 1),
            Style::default().fg(theme::TEXT_DIM()),
        ));
        for display_col in 0..8 {
            let file = match orientation {
                ChessColor::White => display_col,
                ChessColor::Black => 7 - display_col,
            };
            let index = rank * 8 + file;
            spans.push(square_span(
                snapshot.pieces[index],
                index,
                state.cursor(),
                selected,
                legal_targets.contains(&index),
                cell_width,
            ));
        }
        spans.push(Span::styled(
            format!(" {}", rank + 1),
            Style::default().fg(theme::TEXT_DIM()),
        ));
        lines.push(Line::from(spans));
    }
    lines.push(file_labels);
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        board_area,
    );
}

fn square_span(
    piece: Option<ChessPiece>,
    index: usize,
    cursor: usize,
    selected: Option<usize>,
    legal_target: bool,
    cell_width: usize,
) -> Span<'static> {
    let dark = ((index / 8) + (index % 8)) % 2 == 0;
    let mut style = if dark {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(Color::Rgb(42, 47, 45))
    } else {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(Color::Rgb(87, 95, 82))
    };
    if selected == Some(index) {
        style = style.bg(theme::AMBER()).fg(theme::BG_CANVAS());
    } else if cursor == index {
        style = style
            .bg(theme::BG_SELECTION())
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD);
    } else if legal_target {
        style = style.bg(Color::Rgb(80, 72, 42)).fg(theme::AMBER());
    }

    let text = match piece {
        Some(piece) => piece_label(piece).to_string(),
        None if legal_target => "*".to_string(),
        None => ".".to_string(),
    };
    let padded = if cell_width >= 5 {
        format!("  {text}  ")
    } else {
        format!(" {text} ")
    };
    Span::styled(padded, style)
}

fn file_label_line(orientation: ChessColor, cell_width: usize) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for display_col in 0..8 {
        let file = match orientation {
            ChessColor::White => display_col,
            ChessColor::Black => 7 - display_col,
        };
        let label = (b'a' + file as u8) as char;
        let text = if cell_width >= 5 {
            format!("  {label}  ")
        } else {
            format!(" {label} ")
        };
        spans.push(Span::styled(text, Style::default().fg(theme::TEXT_DIM())));
    }
    spans.push(Span::raw("  "));
    Line::from(spans)
}

fn status_line(state: &State) -> Line<'static> {
    let snapshot = state.snapshot();
    let color = match snapshot.phase {
        ChessPhase::Active => theme::AMBER(),
        ChessPhase::Finished => theme::SUCCESS(),
        _ => theme::TEXT_DIM(),
    };
    Line::from(vec![
        Span::styled(
            snapshot.status_message.clone(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            snapshot
                .last_move
                .as_ref()
                .map(|mv| format!(" · last {}", mv.label))
                .unwrap_or_default(),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

fn key_line(state: &State) -> Line<'static> {
    let seated = state.seat_index().is_some();
    let active = state.snapshot().phase == ChessPhase::Active;
    let mut spans = Vec::new();
    if seated {
        spans.push(Span::styled(
            "arrows/wasd",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " cursor  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled(
            "Space/Enter",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " select/move  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        if active {
            spans.push(Span::styled("r", Style::default().fg(theme::AMBER_DIM())));
            spans.push(Span::styled(
                " resign  ",
                Style::default().fg(theme::TEXT_DIM()),
            ));
        } else {
            spans.push(Span::styled("n", Style::default().fg(theme::AMBER_DIM())));
            spans.push(Span::styled(
                " start  ",
                Style::default().fg(theme::TEXT_DIM()),
            ));
            spans.push(Span::styled("l", Style::default().fg(theme::AMBER_DIM())));
            spans.push(Span::styled(
                " leave  ",
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    } else {
        spans.push(Span::styled(
            "s/Space/Enter",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " sit  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::styled("q", Style::default().fg(theme::AMBER_DIM())));
    spans.push(Span::styled(
        " room",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    Line::from(spans)
}

fn clock_line(state: &State) -> Line<'static> {
    let snapshot = state.snapshot();
    Line::from(vec![
        Span::styled("White ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format_clock_for(state, 0),
            Style::default().fg(clock_color(snapshot.turn == ChessColor::White)),
        ),
        Span::styled("   Black ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format_clock_for(state, 1),
            Style::default().fg(clock_color(snapshot.turn == ChessColor::Black)),
        ),
        Span::styled(
            format!("   {}", snapshot.time_control_label),
            Style::default().fg(theme::TEXT_MUTED()),
        ),
    ])
}

fn clock_color(active: bool) -> Color {
    if active {
        theme::AMBER()
    } else {
        theme::TEXT_BRIGHT()
    }
}

fn format_clock_for(state: &State, index: usize) -> String {
    let snapshot = state.snapshot();
    if snapshot.phase == ChessPhase::Active
        && snapshot.turn.seat_index() == index
        && let Some(deadline) = snapshot.active_deadline
    {
        return format_duration(deadline.saturating_duration_since(Instant::now()).as_secs());
    }
    format_clock(snapshot.clocks[index])
}

fn format_clock(clock: ChessClockSnapshot) -> String {
    if let Some(deadline) = clock.move_deadline {
        let remaining = deadline.saturating_duration_since(Instant::now()).as_secs();
        return format_duration(remaining);
    }
    clock
        .remaining_secs
        .map(format_duration)
        .unwrap_or_else(|| "--".to_string())
}

fn format_duration(secs: u64) -> String {
    if secs >= 24 * 60 * 60 {
        let days = secs.div_ceil(24 * 60 * 60);
        return format!("{days}d");
    }
    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes}:{seconds:02}")
}

fn info_lines<'a>(state: &State, usernames: &'a HashMap<Uuid, String>) -> Vec<Line<'a>> {
    let snapshot = state.snapshot();
    let white = seat_name(snapshot.seats[0], usernames);
    let black = seat_name(snapshot.seats[1], usernames);
    let result = match snapshot.result {
        Some(ChessGameResult::Checkmate { winner }) => format!("{} checkmate", winner.label()),
        Some(ChessGameResult::Timeout { winner }) => format!("{} timeout", winner.label()),
        Some(ChessGameResult::Resignation { winner }) => format!("{} resignation", winner.label()),
        Some(ChessGameResult::Draw) => "draw".to_string(),
        None => snapshot.phase_label(),
    };
    vec![
        info_tagline("Timed chess room."),
        Line::raw(""),
        info_label_value("White", white, theme::TEXT_BRIGHT()),
        info_label_value("Black", black, theme::TEXT_BRIGHT()),
        info_label_value("Clock", snapshot.time_control_label.clone(), theme::AMBER()),
        info_label_value("State", result, theme::SUCCESS()),
        Line::raw(""),
        key_hint("Space", "select / move"),
        key_hint("n", "start next game"),
        key_hint("r", "resign active game"),
        key_hint("q", "leave room"),
    ]
}

fn seat_name(user_id: Option<Uuid>, usernames: &HashMap<Uuid, String>) -> String {
    user_id
        .and_then(|id| usernames.get(&id).cloned())
        .unwrap_or_else(|| "open".to_string())
}

trait SnapshotPhaseLabel {
    fn phase_label(&self) -> String;
}

impl SnapshotPhaseLabel for crate::app::rooms::chess::svc::ChessSnapshot {
    fn phase_label(&self) -> String {
        match self.phase {
            ChessPhase::Waiting => "waiting".to_string(),
            ChessPhase::Ready => "ready".to_string(),
            ChessPhase::Active => format!("{} turn", self.turn.label()),
            ChessPhase::Finished => "finished".to_string(),
        }
    }
}
