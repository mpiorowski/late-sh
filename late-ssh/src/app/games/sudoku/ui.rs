use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::games::ui::{
    centered_rect, draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint,
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let filled = state
        .grid
        .iter()
        .flat_map(|row| row.iter())
        .filter(|&&cell| cell != 0)
        .count();

    let mode_str = match state.mode {
        Mode::Daily => "daily",
        Mode::Personal => "personal",
    };

    let info_lines = vec![
        info_tagline("Classic newspaper logic."),
        Line::from(""),
        info_label_value("Mode", mode_str.to_string(), theme::AMBER_GLOW()),
        info_label_value(
            "Difficulty",
            state.difficulty_key().to_string(),
            theme::SUCCESS(),
        ),
        info_label_value("Progress", format!("{filled}/81"), theme::TEXT_BRIGHT()),
        info_label_value(
            "Cursor",
            format!("{}{}", row_label(state.cursor.0), state.cursor.1 + 1),
            theme::TEXT_BRIGHT(),
        ),
        Line::from(""),
        key_hint("h/j/k/l", "move"),
        key_hint("1-9", "place digit"),
        key_hint("0/Bksp", "clear cell"),
        key_hint("d/p/n", "daily/pers/new"),
        key_hint("[ ]", "difficulty"),
        key_hint("r", "reset board"),
        key_hint("Esc", "exit"),
    ];

    let board_area = draw_game_frame(frame, area, "Sudoku", info_lines);

    let board_rect = centered_rect(
        board_area,
        42.min(board_area.width),
        15.min(board_area.height),
    );
    let board = Paragraph::new(board_lines(state)).alignment(Alignment::Center);
    frame.render_widget(board, board_rect);

    if state.is_game_over {
        let subtext = match state.mode {
            Mode::Daily => "Change diff via [ ]",
            Mode::Personal => "n for new",
        };
        draw_game_overlay(
            frame,
            board_area,
            "PUZZLE SOLVED!",
            subtext,
            theme::SUCCESS(),
        );
    }
}

fn board_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = vec![
        column_header(),
        Line::from(Span::styled(
            "   ┌───────────┬───────────┬───────────┐",
            Style::default().fg(theme::BORDER_ACTIVE()),
        )),
    ];

    for row in 0..9 {
        lines.push(board_row(state, row));
        if row == 2 || row == 5 {
            lines.push(Line::from(Span::styled(
                "   ├───────────┼───────────┼───────────┤",
                Style::default().fg(theme::BORDER()),
            )));
        }
    }

    lines.push(Line::from(Span::styled(
        "   └───────────┴───────────┴───────────┘",
        Style::default().fg(theme::BORDER_ACTIVE()),
    )));
    lines
}

fn column_header() -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];

    for block in 0..3 {
        for inner in 0..3 {
            let col = block * 3 + inner + 1;
            spans.push(Span::styled(
                format!(" {col} "),
                Style::default().fg(theme::TEXT_DIM()),
            ));
            if inner < 2 {
                spans.push(Span::raw(" "));
            }
        }
        if block < 2 {
            spans.push(Span::raw(" "));
        }
    }

    Line::from(spans)
}

fn board_row(state: &State, row: usize) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!(" {} ", row_label(row)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled("│", Style::default().fg(theme::BORDER_ACTIVE())),
    ];

    for block in 0..3 {
        for inner in 0..3 {
            let col = block * 3 + inner;
            spans.push(cell_span(state, row, col));
            if inner < 2 {
                spans.push(Span::raw(" "));
            }
        }
        spans.push(Span::styled(
            "│",
            Style::default().fg(theme::BORDER_ACTIVE()),
        ));
    }

    Line::from(spans)
}

fn cell_span(state: &State, row: usize, col: usize) -> Span<'static> {
    let value = state.grid[row][col];
    let is_fixed = state.fixed_mask[row][col];
    let is_selected = state.cursor == (row, col);
    let mut style = if value == 0 {
        Style::default().fg(theme::TEXT_FAINT())
    } else if is_fixed {
        Style::default().fg(theme::TEXT_MUTED())
    } else {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    };

    if is_selected {
        style = style
            .bg(theme::BG_HIGHLIGHT())
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD);
    }

    Span::styled(
        if value == 0 {
            " · ".to_string()
        } else {
            format!(" {value} ")
        },
        style,
    )
}

fn row_label(row: usize) -> char {
    (b'A' + row as u8) as char
}
