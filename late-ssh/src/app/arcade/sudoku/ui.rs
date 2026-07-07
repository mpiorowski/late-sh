use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Mode, State};
use crate::app::arcade::ui::{
    GameBottomBar, OverlayAnchor, centered_rect, draw_game_frame, draw_game_overlay,
    draw_game_overlay_anchored, keys_line, status_line,
};
use crate::app::common::theme;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
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

    let (pencil_label, pencil_color) = if state.pencil_mode {
        ("on", theme::AMBER_GLOW())
    } else {
        ("off", theme::TEXT_DIM())
    };
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", mode_str.to_string(), theme::AMBER_GLOW()),
            ("diff", state.difficulty_key().to_string(), theme::SUCCESS()),
            ("filled", format!("{filled}/81"), theme::TEXT_BRIGHT()),
            (
                "at",
                format!("{}{}", row_label(state.cursor.0), state.cursor.1 + 1),
                theme::TEXT_BRIGHT(),
            ),
            ("pencil", pencil_label.to_string(), pencil_color),
        ]),
        keys: keys_line(vec![
            ("h/j/k/l", "move"),
            ("1-9", "place"),
            ("m", "pencil"),
            ("0", "clear"),
            ("d/p/n", "daily/pers/new"),
            ("[ ]", "diff"),
            ("r", "reset"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: state
            .reset_pending
            .map(|kind| crate::app::arcade::ui::tip_line(kind.confirm_tip())),
    };

    let board_area = draw_game_frame(frame, area, "Sudoku", bottom, show_bottom_bar);

    let board_rect = centered_rect(
        board_area,
        42.min(board_area.width),
        15.min(board_area.height),
    );
    let board = Paragraph::new(board_lines(state)).alignment(Alignment::Center);
    frame.render_widget(board, board_rect);

    if !state.is_loading() && !state.is_game_over {
        draw_notes_pad(frame, board_area, board_rect, state);
    }

    if state.is_loading() {
        draw_game_overlay(
            frame,
            board_area,
            "GENERATING...",
            "Daily board will appear shortly",
            theme::AMBER_GLOW(),
        );
    } else if state.is_game_over {
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

/// The B1 candidate pad: a numpad-laid-out view of the cursor cell's pencil
/// marks, drawn to the right of the board when the frame is wide enough. Only
/// the active cell's marks are shown here; on the board itself a noted empty
/// cell is tinted (see `cell_span`) so you can tell which cells carry marks.
fn draw_notes_pad(frame: &mut Frame, board_area: Rect, board_rect: Rect, state: &State) {
    const PAD_W: u16 = 11;
    const PAD_H: u16 = 5;
    let x = board_rect.x + board_rect.width + 2;
    if x + PAD_W > board_area.x + board_area.width || board_area.height < PAD_H {
        return;
    }

    let (r, c) = state.cursor;
    let notes = state.notes[r][c];

    let heading = if state.pencil_mode {
        Span::styled(
            "pencil ON",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("notes", Style::default().fg(theme::TEXT_DIM()))
    };

    let mut lines = vec![
        Line::from(heading),
        Line::from(Span::styled(
            format!("cell {}{}", row_label(r), c + 1),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];

    // Three bands of three, laid out like a numpad: 1-3, 4-6, 7-9.
    for band in 0..3u8 {
        let spans = (0..3u8)
            .map(|col| {
                let d = band * 3 + col + 1;
                if notes & (1 << (d - 1)) != 0 {
                    Span::styled(
                        format!(" {} ", (b'0' + d) as char),
                        Style::default()
                            .fg(theme::TEXT_BRIGHT())
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT()))
                }
            })
            .collect::<Vec<_>>();
        lines.push(Line::from(spans));
    }

    let rect = Rect::new(x, board_rect.y, PAD_W, PAD_H);
    frame.render_widget(Paragraph::new(lines), rect);
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
    let is_conflict = !is_fixed && cell_has_duplicate(&state.grid, row, col);
    let has_notes = value == 0 && state.notes[row][col] != 0;
    let mut style = if value == 0 {
        // Tint empties that carry pencil marks so you can spot them at a glance;
        // the cursor pad shows the actual candidates.
        if has_notes {
            Style::default().fg(theme::AMBER_DIM())
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        }
    } else if is_fixed {
        Style::default().fg(theme::TEXT_MUTED())
    } else if is_conflict {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    };

    if is_selected {
        style = style.bg(theme::BG_HIGHLIGHT()).add_modifier(Modifier::BOLD);
        if !is_conflict {
            style = style.fg(theme::TEXT_BRIGHT());
        }
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

fn cell_has_duplicate(grid: &[[u8; 9]; 9], row: usize, col: usize) -> bool {
    let value = grid[row][col];
    if value == 0 {
        return false;
    }

    for (peer_col, peer_value) in grid[row].iter().enumerate() {
        if peer_col != col && *peer_value == value {
            return true;
        }
    }
    for (peer_row, peer) in grid.iter().enumerate() {
        if peer_row != row && peer[col] == value {
            return true;
        }
    }

    let box_row = (row / 3) * 3;
    let box_col = (col / 3) * 3;
    for (peer_row, peer) in grid.iter().enumerate().skip(box_row).take(3) {
        for (peer_col, peer_value) in peer.iter().enumerate().skip(box_col).take(3) {
            if (peer_row != row || peer_col != col) && *peer_value == value {
                return true;
            }
        }
    }

    false
}

fn row_label(row: usize) -> char {
    (b'A' + row as u8) as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_detection_uses_only_visible_grid_rules() {
        let mut grid = [[0u8; 9]; 9];
        grid[0][0] = 5;
        grid[0][8] = 5;
        assert!(cell_has_duplicate(&grid, 0, 0));
        assert!(cell_has_duplicate(&grid, 0, 8));

        grid[0][8] = 0;
        grid[8][0] = 5;
        assert!(cell_has_duplicate(&grid, 0, 0));

        grid[8][0] = 0;
        grid[2][2] = 5;
        assert!(cell_has_duplicate(&grid, 0, 0));
    }

    #[test]
    fn duplicate_detection_does_not_mark_non_conflicting_guess() {
        let mut grid = [[0u8; 9]; 9];
        grid[0][0] = 5;
        grid[1][2] = 6;
        grid[4][4] = 5;

        assert!(!cell_has_duplicate(&grid, 0, 0));
    }
}
