use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;
use crate::app::games::ui::{
    centered_rect, draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint,
};

use super::state::{self, Mode, State, adjacent_mine_count};

const CELL_HIDDEN: u8 = 0;
const CELL_REVEALED: u8 = 1;
const CELL_FLAGGED: u8 = 2;
const CELL_MINE_HIT: u8 = 3;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let diff = state.difficulty();
    let mode_str = match state.mode {
        Mode::Daily => "daily",
        Mode::Personal => "personal",
    };

    let lives_str = (0..state::MAX_LIVES)
        .map(|i| if i < state.lives { '#' } else { '.' })
        .collect::<String>();

    let info_lines = vec![
        info_tagline("Clear the field. Three strikes and you're out."),
        Line::from(""),
        info_label_value("Mode", mode_str.to_string(), theme::AMBER_GLOW()),
        info_label_value(
            "Difficulty",
            state.difficulty_key().to_string(),
            theme::SUCCESS(),
        ),
        info_label_value("Lives", lives_str, lives_color(state.lives)),
        info_label_value(
            "Revealed",
            format!("{}/{}", state.revealed_count(), state.safe_cell_count()),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "Flags",
            format!("{}/{}", state.accounted_mine_count(), state.mine_count()),
            theme::AMBER(),
        ),
        Line::from(""),
        key_hint("h/j/k/l", "move"),
        key_hint("Space", "reveal"),
        key_hint("f", "flag mine"),
        key_hint("d/p/n", "daily/pers/new"),
        key_hint("[ ]", "difficulty"),
        key_hint("Esc", "exit"),
        Line::from(""),
        Line::from(Span::styled(
            "Reveal tips",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        info_tagline("Press a matching number on"),
        info_tagline("a revealed cell to open"),
        info_tagline("all adjacent unflagged cells."),
    ];

    let board_area = draw_game_frame(frame, area, "Minesweeper", info_lines);

    let board_w = (diff.cols as u16) * 4 + 4; // row labels + borders
    let board_h = diff.rows as u16 + 3; // col headers + top/bottom borders
    let board_rect = centered_rect(
        board_area,
        board_w.min(board_area.width),
        board_h.min(board_area.height),
    );

    frame.render_widget(
        Paragraph::new(board_lines(state)).alignment(Alignment::Center),
        board_rect,
    );

    if state.is_game_over {
        let won = state.revealed_count() == state.safe_cell_count();
        if won {
            let subtext = match state.mode {
                Mode::Daily => "Change diff via [ ]",
                Mode::Personal => "n for new",
            };
            draw_game_overlay(
                frame,
                board_area,
                "FIELD CLEARED!",
                subtext,
                theme::SUCCESS(),
            );
        } else {
            let subtext = match state.mode {
                Mode::Daily => "Try another diff via [ ]",
                Mode::Personal => "n for new board",
            };
            draw_game_overlay(frame, board_area, "GAME OVER", subtext, Color::Red);
        }
    }
}

fn board_lines(state: &State) -> Vec<Line<'static>> {
    let diff = state.difficulty();
    let dim = Style::default().fg(theme::BORDER_DIM());

    let mut lines = Vec::new();

    // Column headers
    lines.push(column_header(diff.cols));

    // Top border
    let mut top = "   \u{250c}".to_string();
    for ci in 0..diff.cols {
        top.push_str("\u{2500}\u{2500}\u{2500}");
        top.push(if ci < diff.cols - 1 {
            '\u{252c}'
        } else {
            '\u{2510}'
        });
    }
    lines.push(Line::from(Span::styled(top, dim)));

    // Cell rows
    for row in 0..diff.rows {
        lines.push(board_row(state, row));
    }

    // Bottom border
    let mut bot = "   \u{2514}".to_string();
    for ci in 0..diff.cols {
        bot.push_str("\u{2500}\u{2500}\u{2500}");
        bot.push(if ci < diff.cols - 1 {
            '\u{2534}'
        } else {
            '\u{2518}'
        });
    }
    lines.push(Line::from(Span::styled(bot, dim)));

    lines
}

fn column_header(cols: usize) -> Line<'static> {
    let mut spans = vec![Span::raw("    ")];
    for col in 0..cols {
        let label = format!("{:>2} ", col + 1);
        spans.push(Span::styled(label, Style::default().fg(theme::TEXT_DIM())));
        if col < cols - 1 {
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}

fn board_row(state: &State, row: usize) -> Line<'static> {
    let diff = state.difficulty();
    let dim = Style::default().fg(theme::BORDER_DIM());

    let mut spans = vec![
        Span::styled(
            format!(" {} ", row_label(row)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled("\u{2502}", dim),
    ];

    for col in 0..diff.cols {
        spans.push(cell_span(state, row, col));
        spans.push(Span::styled("\u{2502}", dim));
    }

    Line::from(spans)
}

fn cell_span(state: &State, row: usize, col: usize) -> Span<'static> {
    let cell = state
        .player_grid()
        .get(row)
        .and_then(|r| r.get(col))
        .copied()
        .unwrap_or(CELL_HIDDEN);
    let is_selected = state.cursor == (row, col);
    let mine_map = state.mine_map();

    let (glyph, mut style) = match cell {
        CELL_REVEALED => {
            let count = adjacent_mine_count(mine_map, row, col);
            if count == 0 {
                ("   ".to_string(), Style::default().fg(theme::TEXT_FAINT()))
            } else {
                (
                    format!(" {count} "),
                    Style::default()
                        .fg(number_color(count))
                        .add_modifier(Modifier::BOLD),
                )
            }
        }
        CELL_FLAGGED => (
            " F ".to_string(),
            Style::default()
                .fg(Color::Rgb(20, 16, 10))
                .bg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        CELL_MINE_HIT => (
            " * ".to_string(),
            Style::default()
                .fg(Color::Rgb(30, 10, 10))
                .bg(Color::Rgb(180, 56, 48))
                .add_modifier(Modifier::BOLD),
        ),
        _ => (
            " \u{00b7} ".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    };

    if is_selected {
        style = style
            .bg(theme::BG_HIGHLIGHT())
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD);
    }

    Span::styled(glyph, style)
}

fn number_color(n: u8) -> Color {
    match n {
        1 => Color::Blue,
        2 => Color::Green,
        3 => Color::Red,
        4 => Color::Magenta,
        5 => Color::Yellow,
        6 => Color::Cyan,
        7 => Color::Gray,
        _ => Color::DarkGray,
    }
}

fn lives_color(lives: u8) -> Color {
    match lives {
        3 => Color::Green,
        2 => Color::Yellow,
        1 => Color::Red,
        _ => Color::DarkGray,
    }
}

fn row_label(row: usize) -> char {
    (b'A' + row as u8) as char
}
