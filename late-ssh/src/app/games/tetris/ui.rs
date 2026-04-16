use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{BOARD_HEIGHT, BOARD_WIDTH, PieceKind, State};
use crate::app::common::theme;
use crate::app::games::ui::{
    centered_rect, draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint,
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let info_lines = vec![
        info_tagline("Endless falling blocks. Speed rises as you survive."),
        Line::from(""),
        info_label_value("Score", state.score.to_string(), theme::AMBER_GLOW()),
        info_label_value("Best", state.best_score.to_string(), theme::SUCCESS()),
        info_label_value("Lines", state.lines.to_string(), theme::TEXT_BRIGHT()),
        info_label_value("Level", state.level.to_string(), theme::TEXT_BRIGHT()),
        info_label_value("Next", state.next.name().to_string(), theme::AMBER_DIM()),
        Line::from(""),
        key_hint("h/l or ←/→", "move"),
        key_hint("j or ↓", "soft drop"),
        key_hint("k or ↑", "rotate"),
        key_hint("Space", "hard drop"),
        key_hint("p", "pause"),
        key_hint("r", "restart"),
        key_hint("Esc", "exit"),
    ];

    let board_area = draw_game_frame(frame, area, "Tetris", info_lines);
    let board_rect = centered_rect(
        board_area,
        24.min(board_area.width),
        22.min(board_area.height),
    );
    let board = Paragraph::new(board_lines(state)).alignment(Alignment::Center);
    frame.render_widget(board, board_rect);

    if state.is_paused {
        draw_game_overlay(
            frame,
            board_area,
            "PAUSED",
            "Press p to resume",
            theme::AMBER(),
        );
    } else if state.is_game_over {
        draw_game_overlay(
            frame,
            board_area,
            "GAME OVER",
            "Press r for a fresh run",
            theme::ERROR(),
        );
    }
}

fn board_lines(state: &State) -> Vec<Line<'static>> {
    let board = state.board_with_active_piece();
    let mut lines = Vec::with_capacity(BOARD_HEIGHT + 2);
    lines.push(Line::from(Span::styled(
        format!("┌{}┐", "─".repeat(BOARD_WIDTH * 2)),
        Style::default().fg(theme::BORDER_ACTIVE()),
    )));

    for row in board {
        let mut spans = vec![Span::styled(
            "│",
            Style::default().fg(theme::BORDER_ACTIVE()),
        )];
        for cell in row {
            spans.push(cell_span(cell));
        }
        spans.push(Span::styled(
            "│",
            Style::default().fg(theme::BORDER_ACTIVE()),
        ));
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(Span::styled(
        format!("└{}┘", "─".repeat(BOARD_WIDTH * 2)),
        Style::default().fg(theme::BORDER_ACTIVE()),
    )));

    lines
}

fn cell_span(cell: Option<PieceKind>) -> Span<'static> {
    match cell {
        Some(kind) => Span::styled(
            "██",
            Style::default()
                .fg(piece_color(kind))
                .add_modifier(Modifier::BOLD),
        ),
        None => Span::styled("  ", Style::default().bg(theme::BG_SELECTION())),
    }
}

fn piece_color(kind: PieceKind) -> Color {
    match kind {
        PieceKind::I => Color::Cyan,
        PieceKind::O => Color::Yellow,
        PieceKind::T => Color::Magenta,
        PieceKind::S => Color::Green,
        PieceKind::Z => Color::Red,
        PieceKind::J => Color::Blue,
        PieceKind::L => Color::Rgb(255, 165, 0),
    }
}
