use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
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
                            .fg(digit_color(d))
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
            // Tint each column number in its own digit colour so the header
            // doubles as a legend for the board's palette.
            spans.push(Span::styled(
                format!(" {col} "),
                Style::default()
                    .fg(digit_color(col as u8))
                    .add_modifier(Modifier::BOLD),
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

/// The nine digit colours — a distinct hue per number, colored-pencil style, so
/// the board reads at a glance. Fixed clues use a calmer shade of the same hue
/// (see `cell_span`) so you can still tell givens from your own entries.
fn digit_color(value: u8) -> Color {
    match value {
        1 => Color::Rgb(239, 83, 80),   // red
        2 => Color::Rgb(255, 152, 0),   // orange
        3 => Color::Rgb(255, 213, 79),  // amber
        4 => Color::Rgb(102, 187, 106), // green
        5 => Color::Rgb(38, 198, 218),  // cyan
        6 => Color::Rgb(66, 165, 245),  // blue
        7 => Color::Rgb(121, 134, 203), // indigo
        8 => Color::Rgb(186, 104, 200), // purple
        9 => Color::Rgb(240, 98, 146),  // pink
        _ => theme::TEXT_FAINT(),
    }
}

/// Scale an RGB colour toward black by `pct` percent (a calmer shade). Non-RGB
/// colours pass through unchanged.
fn dim(color: Color, pct: u16) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as u16 * pct / 100) as u8,
            (g as u16 * pct / 100) as u8,
            (b as u16 * pct / 100) as u8,
        ),
        other => other,
    }
}

fn cell_span(state: &State, row: usize, col: usize) -> Span<'static> {
    let value = state.grid[row][col];
    let is_fixed = state.fixed_mask[row][col];
    let is_selected = state.cursor == (row, col);
    let is_conflict = !is_fixed && cell_has_duplicate(&state.grid, row, col);
    let has_notes = value == 0 && state.notes[row][col] != 0;

    // Cells sharing the selected cell's number light up so you can scan for
    // placements; row/column/box peers are intentionally left alone to keep the
    // board calm.
    let (cr, cc) = state.cursor;
    let sel_val = state.grid[cr][cc];
    let is_same_num = !is_selected && value != 0 && value == sel_val;

    // Foreground: colour each digit by its value; givens are a calmer shade,
    // your entries are bright and bold, conflicts red.
    let mut style = if value == 0 {
        if has_notes {
            Style::default().fg(theme::AMBER_DIM())
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        }
    } else if is_conflict {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else if is_fixed {
        Style::default().fg(dim(digit_color(value), 66))
    } else {
        Style::default()
            .fg(digit_color(value))
            .add_modifier(Modifier::BOLD)
    };

    // Background: selected cell strongest, then all cells sharing its number -
    // the modern-sudoku "light up the board" feel.
    if is_selected {
        style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
        if value == 0 {
            style = style.fg(theme::TEXT_BRIGHT());
        }
    } else if is_same_num {
        style = style
            .bg(theme::SUDOKU_SAME_NUM_BG())
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
#[path = "ui_test.rs"]
mod ui_test;
