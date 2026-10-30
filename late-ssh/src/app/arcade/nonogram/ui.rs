use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::arcade::ui::{
    GameBottomBar, OverlayAnchor, centered_rect, draw_game_frame, draw_game_overlay_anchored,
    keys_line, status_line,
};
use crate::app::common::theme;

use super::state::{LineStatus, Mode, State};

fn empty_bottom_bar() -> GameBottomBar {
    GameBottomBar {
        status: status_line(vec![]),
        keys: keys_line(vec![("`", "dashboard"), ("Esc", "exit")]),
        tip: None,
    }
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    if !state.has_puzzles() {
        let board_area = draw_game_frame(
            frame,
            area,
            "Nonograms",
            empty_bottom_bar(),
            show_bottom_bar,
        );
        frame.render_widget(
            Paragraph::new("No nonogram packs loaded. Run `gen_nonograms` first.")
                .alignment(Alignment::Center),
            board_area,
        );
        return;
    }

    let Some(puzzle) = state.puzzle() else {
        let board_area = draw_game_frame(
            frame,
            area,
            "Nonograms",
            empty_bottom_bar(),
            show_bottom_bar,
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

    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", mode_str.to_string(), theme::AMBER_GLOW()),
            ("diff", state.difficulty_key().to_string(), theme::SUCCESS()),
            (
                "filled",
                format!("{}/{}", state.filled_count(), state.target_count()),
                theme::TEXT_BRIGHT(),
            ),
        ]),
        keys: keys_line(
            vec![
                ("h/j/k/l", "move"),
                ("Space", "fill"),
                ("x", "mark"),
                ("0", "clear"),
                ("d/p/n", "daily/pers/new"),
                ("[ ]", "diff"),
                ("r", "reset"),
                ("`", "dashboard"),
                ("Esc", "exit"),
            ]
            .into_iter()
            .chain(crate::app::arcade::ui::share_hints(super::share::is_ready(
                state,
            )))
            .collect(),
        ),
        tip: state
            .reset_pending
            .map(|kind| crate::app::arcade::ui::tip_line(kind.confirm_tip())),
    };

    let board_area = draw_game_frame(frame, area, "Nonograms", bottom, show_bottom_bar);

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
        draw_game_overlay_anchored(
            frame,
            board_area,
            "PUZZLE SOLVED!",
            subtext,
            theme::SUCCESS(),
            OverlayAnchor::Top,
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
    let num_rows = puzzle.height as usize;
    let clue_pad = max_row_clues * 3;
    let dim = Style::default().fg(theme::BORDER_DIM());
    let guide = Style::default().fg(theme::BORDER());

    let mut lines = Vec::new();

    let cursor_col = state.cursor.1;
    let cursor_row = state.cursor.0;
    let clue_active = Style::default().fg(theme::TEXT_BRIGHT());
    let clue_normal = Style::default().fg(theme::AMBER_DIM());
    let clue_satisfied = Style::default()
        .fg(theme::SUCCESS())
        .add_modifier(Modifier::DIM);
    let clue_impossible = Style::default()
        .fg(theme::ERROR())
        .add_modifier(Modifier::BOLD);

    let (row_statuses, col_statuses) = state.row_col_statuses().unwrap_or_else(|| (vec![], vec![]));

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
            let style = clue_style(
                ci == cursor_col,
                col_statuses.get(ci).copied().unwrap_or(LineStatus::Pending),
                clue_active,
                clue_normal,
                clue_satisfied,
                clue_impossible,
            );
            spans.push(Span::styled(format!("{clue:>2} "), style));
            if ci < num_cols - 1 {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    // Top border
    lines.push(Line::from(Span::styled(
        border_line(clue_pad, num_cols, '┌', '┬', '┰', '┐'),
        dim,
    )));

    // Cell rows
    for row in 0..puzzle.height as usize {
        let mut spans = Vec::new();

        // Row clues
        let row_clues = &puzzle.row_clues[row];
        let pad = max_row_clues.saturating_sub(row_clues.len());
        let row_style = clue_style(
            row == cursor_row,
            row_statuses
                .get(row)
                .copied()
                .unwrap_or(LineStatus::Pending),
            clue_active,
            clue_normal,
            clue_satisfied,
            clue_impossible,
        );
        for _ in 0..pad {
            spans.push(Span::raw("   "));
        }
        for clue in row_clues {
            spans.push(Span::styled(format!("{clue:>2} "), row_style));
        }

        // Leading border, then cells with │ separators: every fifth column
        // boundary is a heavy guide, and every fifth row is underlined, the
        // way paper nonograms are ruled so runs can be counted by eye.
        let underline = |style: Style| match is_guide(row, num_rows) {
            true => style
                .add_modifier(Modifier::UNDERLINED)
                .underline_color(theme::BORDER()),
            false => style,
        };
        spans.push(Span::styled("│", dim));
        for col in 0..num_cols {
            let cell = cell_span(state, row, col);
            spans.push(Span::styled(cell.content, underline(cell.style)));
            let separator = match (col + 1 < num_cols, is_guide(col, num_cols)) {
                (true, true) => Span::styled("┃", underline(guide)),
                (true, false) => Span::styled("│", underline(dim)),
                (false, _) => Span::styled("│", dim),
            };
            spans.push(separator);
        }

        lines.push(Line::from(spans));
    }

    // Bottom border
    lines.push(Line::from(Span::styled(
        border_line(clue_pad, num_cols, '└', '┴', '┸', '┘'),
        dim,
    )));

    lines
}

/// A horizontal border under the clue gutter: `tee` at each column boundary,
/// `guide_tee` at every fifth, `left` and `right` at the outer corners.
fn border_line(
    clue_pad: usize,
    num_cols: usize,
    left: char,
    tee: char,
    guide_tee: char,
    right: char,
) -> String {
    let mut line = " ".repeat(clue_pad);
    line.push(left);
    for ci in 0..num_cols {
        line.push_str("───");
        line.push(match (ci + 1 == num_cols, is_guide(ci, num_cols)) {
            (true, _) => right,
            (false, true) => guide_tee,
            (false, false) => tee,
        });
    }
    line
}

/// Whether the boundary after line `index` (of `len`) is a guide: every fifth
/// one, never the outer edge.
fn is_guide(index: usize, len: usize) -> bool {
    (index + 1) % 5 == 0 && index + 1 < len
}

fn clue_style(
    is_cursor_line: bool,
    status: LineStatus,
    active: Style,
    normal: Style,
    satisfied: Style,
    impossible: Style,
) -> Style {
    let mut style = match status {
        LineStatus::Satisfied => satisfied,
        LineStatus::Impossible => impossible,
        LineStatus::Pending => normal,
    };

    if is_cursor_line {
        style = style.add_modifier(Modifier::BOLD);
        if status == LineStatus::Pending {
            style = active.add_modifier(Modifier::BOLD);
        }
    }

    style
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

    // The cursor is Solitaire's solid amber fill with the mark punched
    // through, so it reads on every palette; a background tint did not.
    if is_selected {
        style = theme::punch_through(theme::AMBER()).add_modifier(Modifier::BOLD);
    }

    let glyph = match filled {
        1 => " █ ",
        2 => " ✕ ",
        _ => " · ",
    };

    Span::styled(glyph, style)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
