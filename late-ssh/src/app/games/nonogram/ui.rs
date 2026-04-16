use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;
use crate::app::games::ui::{
    centered_rect, draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint,
};

use super::state::{Mode, State};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    if !state.has_puzzles() {
        let board_area = draw_game_frame(
            frame,
            area,
            "Nonograms",
            vec![info_tagline("Paint by logic.")],
        );
        frame.render_widget(
            Paragraph::new("No nonogram packs loaded. Run `gen_nonograms` first.")
                .alignment(Alignment::Center),
            board_area,
        );
        return;
    }

    let Some(pack) = state.selected_pack() else {
        return;
    };
    let Some(puzzle) = state.puzzle() else {
        let board_area = draw_game_frame(
            frame,
            area,
            "Nonograms",
            vec![info_tagline("Paint by logic.")],
        );
        frame.render_widget(
            Paragraph::new("Selected nonogram puzzle is missing from the loaded pack.")
                .alignment(Alignment::Center),
            board_area,
        );
        return;
    };

    let mode_str = match state.mode {
        Mode::Daily => "daily",
        Mode::Personal => "personal",
    };

    let info_lines = vec![
        info_tagline("Paint by logic."),
        Line::from(""),
        info_label_value("Mode", mode_str.to_string(), theme::AMBER_GLOW()),
        info_label_value("Size", pack.size_key.clone(), theme::TEXT_BRIGHT()),
        info_label_value("Difficulty", puzzle.difficulty.clone(), theme::SUCCESS()),
        info_label_value(
            "Progress",
            format!("{}/{}", state.filled_count(), state.target_count()),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "Puzzle",
            state.current_puzzle_id().to_string(),
            theme::TEXT_DIM(),
        ),
        Line::from(""),
        key_hint("h/j/k/l", "move"),
        key_hint("Space", "fill cell"),
        key_hint("x", "mark empty"),
        key_hint("0/Bksp", "clear"),
        key_hint("d/p/n", "daily/pers/new"),
        key_hint("[ ]", "size"),
        key_hint("r", "reset"),
        key_hint("Esc", "exit"),
    ];

    let board_area = draw_game_frame(frame, area, "Nonograms", info_lines);

    let max_col_clues = puzzle.col_clues.iter().map(|c| c.len()).max().unwrap_or(0) as u16;
    let max_row_clues = puzzle.row_clues.iter().map(|c| c.len()).max().unwrap_or(0) as u16;
    let board_h = max_col_clues + puzzle.height + 2; // +2 for top/bottom borders
    let board_w = max_row_clues * 3 + puzzle.width * 4 + 1;
    let board_rect = centered_rect(
        board_area,
        board_w.min(board_area.width),
        board_h.min(board_area.height),
    );

    frame.render_widget(Paragraph::new(board_lines(state, puzzle)), board_rect);

    if state.is_game_over() {
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

fn board_lines(state: &State, puzzle: &late_core::nonogram::NonogramPuzzle) -> Vec<Line<'static>> {
    let max_row_clues = puzzle
        .row_clues
        .iter()
        .map(|clues| clues.len())
        .max()
        .unwrap_or(0);
    let max_col_clues = puzzle
        .col_clues
        .iter()
        .map(|clues| clues.len())
        .max()
        .unwrap_or(0);

    let num_cols = puzzle.width as usize;
    let clue_pad = max_row_clues * 3;
    let dim = Style::default().fg(theme::BORDER_DIM());

    let mut lines = Vec::new();

    let cursor_col = state.cursor.1;
    let cursor_row = state.cursor.0;
    let clue_active = Style::default().fg(theme::TEXT_BRIGHT());
    let clue_normal = Style::default().fg(theme::AMBER_DIM());

    // Column clue rows (offset by 1 to align with cells inside │)
    for clue_row in 0..max_col_clues {
        let mut spans = vec![Span::raw(" ".repeat(clue_pad + 1))];
        for (ci, clues) in puzzle.col_clues.iter().enumerate() {
            let offset = max_col_clues.saturating_sub(clues.len());
            let clue = if clue_row >= offset {
                clues[clue_row - offset].to_string()
            } else {
                String::new()
            };
            let style = if ci == cursor_col {
                clue_active
            } else {
                clue_normal
            };
            spans.push(Span::styled(format!("{clue:>2} "), style));
            if ci < num_cols - 1 {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    // Top border
    let mut top = " ".repeat(clue_pad);
    top.push('┌');
    for ci in 0..num_cols {
        top.push_str("───");
        top.push(if ci < num_cols - 1 { '┬' } else { '┐' });
    }
    lines.push(Line::from(Span::styled(top, dim)));

    // Cell rows
    for row in 0..puzzle.height as usize {
        let mut spans = Vec::new();

        // Row clues
        let row_clues = &puzzle.row_clues[row];
        let pad = max_row_clues.saturating_sub(row_clues.len());
        let row_style = if row == cursor_row {
            clue_active
        } else {
            clue_normal
        };
        for _ in 0..pad {
            spans.push(Span::raw("   "));
        }
        for clue in row_clues {
            spans.push(Span::styled(format!("{clue:>2} "), row_style));
        }

        // Leading border
        spans.push(Span::styled("│", dim));

        // Cells with │ separators
        for col in 0..num_cols {
            spans.push(cell_span(state, row, col));
            spans.push(Span::styled("│", dim));
        }

        lines.push(Line::from(spans));
    }

    // Bottom border
    let mut bot = " ".repeat(clue_pad);
    bot.push('└');
    for ci in 0..num_cols {
        bot.push_str("───");
        bot.push(if ci < num_cols - 1 { '┴' } else { '┘' });
    }
    lines.push(Line::from(Span::styled(bot, dim)));

    lines
}

fn cell_span(state: &State, row: usize, col: usize) -> Span<'static> {
    let is_selected = state.cursor == (row, col);
    let filled = state
        .player_grid()
        .get(row)
        .and_then(|line| line.get(col))
        .copied()
        .unwrap_or(0);

    let mut style = if filled == 1 {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else if filled == 2 {
        Style::default()
            .fg(theme::TEXT_MUTED())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };

    if is_selected {
        style = style
            .bg(theme::BG_HIGHLIGHT())
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD);
    }

    let glyph = match filled {
        1 => " # ",
        2 => " x ",
        _ => " · ",
    };

    Span::styled(glyph, style)
}
