use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
    tip_line,
};

use crate::app::common::theme;

use super::state::{self, Mode, State, adjacent_mine_count};

const CELL_HIDDEN: u8 = 0;
const CELL_REVEALED: u8 = 1;
const CELL_FLAGGED: u8 = 2;
const CELL_MINE_HIT: u8 = 3;
const CHORD_PREVIEW_GLYPH: &str = "\u{2591}\u{2591}\u{2591}";

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let diff = state.difficulty();
    let mode_str = match state.mode {
        Mode::Daily => "daily",
        Mode::Personal => "personal",
    };

    let lives_str = (0..state::MAX_LIVES)
        .map(|i| if i < state.lives { '#' } else { '.' })
        .collect::<String>();

    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", mode_str.to_string(), theme::AMBER_GLOW()),
            ("diff", state.difficulty_key().to_string(), theme::SUCCESS()),
            ("lives", lives_str, lives_color(state.lives)),
            (
                "revealed",
                format!("{}/{}", state.revealed_count(), state.safe_cell_count()),
                theme::TEXT_BRIGHT(),
            ),
            (
                "mines",
                state
                    .mine_count()
                    .saturating_sub(state.flag_count() + state.hit_mine_count())
                    .to_string(),
                theme::AMBER(),
            ),
        ]),
        keys: keys_line(vec![
            ("h/j/k/l", "move"),
            ("Space", "reveal"),
            ("f", "flag"),
            ("1-8", "chord"),
            ("d/p/n", "daily/pers/new"),
            ("[ ]", "diff"),
            ("o", "cell style"),
            ("{ }", "scroll"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: Some(tip_line(
            "On a revealed cell, press its number to open all adjacent unflagged cells.",
        )),
    };

    let board_area = draw_game_frame(frame, area, "Minesweeper", bottom, show_bottom_bar);

    let board_w = (diff.cols as u16) * 4 + 4; // row labels + borders
    let board_h = diff.rows as u16 * 2 + 2; // col headers + top/bottom borders + row separators
    let board_rect = centered_rect(
        board_area,
        board_w.min(board_area.width),
        board_h.min(board_area.height),
    );

    frame.render_widget(
        Paragraph::new(
            board_lines(state)
                .into_iter()
                .skip(state.scroll_offset as usize)
                .collect::<Vec<_>>(),
        )
        .alignment(Alignment::Center),
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

    // Cell rows with separators
    for row in 0..diff.rows {
        lines.push(board_row(state, row));
        if row < diff.rows - 1 {
            lines.push(row_separator(diff.cols));
        }
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

fn row_separator(cols: usize) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER_DIM());
    let mut s = "   \u{251c}".to_string();
    for ci in 0..cols {
        s.push_str("\u{2500}\u{2500}\u{2500}");
        s.push(if ci < cols - 1 {
            '\u{253c}'
        } else {
            '\u{2524}'
        });
    }
    Line::from(Span::styled(s, dim))
}

fn cell_span(state: &State, row: usize, col: usize) -> Span<'static> {
    let cell = state
        .player_grid()
        .get(row)
        .and_then(|r| r.get(col))
        .copied()
        .unwrap_or(CELL_HIDDEN);
    let is_selected = state.cursor == (row, col);
    let is_chord_target = is_chord_preview_target(state, row, col);
    let mine_map = state.mine_map();

    let (mut glyph, mut style) = match cell {
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
        CELL_FLAGGED => flag_span_parts(state, mine_map, row, col),
        CELL_MINE_HIT => (
            " * ".to_string(),
            Style::default()
                .fg(Color::Rgb(30, 10, 10))
                .bg(Color::Rgb(180, 56, 48))
                .add_modifier(Modifier::BOLD),
        ),
        _ => {
            let glyph = if state.use_dot_style {
                " \u{00b7} ".to_string()
            } else {
                "\u{2588}\u{2588}\u{2588}".to_string()
            };
            (glyph, Style::default().fg(theme::TEXT_FAINT()))
        }
    };

    if is_chord_target {
        apply_chord_preview_style(&mut glyph, &mut style);
    }

    if is_selected {
        style = style
            .bg(theme::BG_HIGHLIGHT())
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD);
    }

    Span::styled(glyph, style)
}

fn apply_chord_preview_style(glyph: &mut String, style: &mut Style) {
    *glyph = CHORD_PREVIEW_GLYPH.to_string();
    *style = Style::default().fg(theme::BORDER_DIM());
}

fn flag_span_parts(
    state: &State,
    mine_map: &[Vec<bool>],
    row: usize,
    col: usize,
) -> (String, Style) {
    if state.is_game_over {
        let is_correct = mine_map
            .get(row)
            .and_then(|line| line.get(col))
            .copied()
            .unwrap_or(false);
        let bg = if is_correct {
            theme::SUCCESS()
        } else {
            theme::ERROR()
        };
        return (
            " F ".to_string(),
            Style::default()
                .fg(Color::Rgb(20, 16, 10))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        );
    }

    (
        " F ".to_string(),
        Style::default()
            .fg(Color::Rgb(20, 16, 10))
            .bg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    )
}

fn is_chord_preview_target(state: &State, row: usize, col: usize) -> bool {
    if state.is_game_over {
        return false;
    }

    let (cursor_row, cursor_col) = state.cursor;
    if row == cursor_row && col == cursor_col {
        return false;
    }

    let Some(cursor_cell) = state
        .player_grid()
        .get(cursor_row)
        .and_then(|line| line.get(cursor_col))
        .copied()
    else {
        return false;
    };
    if cursor_cell != CELL_REVEALED {
        return false;
    }

    let Some(cell) = state
        .player_grid()
        .get(row)
        .and_then(|line| line.get(col))
        .copied()
    else {
        return false;
    };
    if cell != CELL_HIDDEN {
        return false;
    }

    let row_delta = row.abs_diff(cursor_row);
    let col_delta = col.abs_diff(cursor_col);
    if row_delta > 1 || col_delta > 1 {
        return false;
    }

    let number = adjacent_mine_count(state.mine_map(), cursor_row, cursor_col);
    number > 0
        && adjacent_accounted_mine_count(state.player_grid(), cursor_row, cursor_col) == number
}

fn adjacent_accounted_mine_count(player_grid: &[Vec<u8>], row: usize, col: usize) -> u8 {
    let mut count = 0u8;
    for dr in -1..=1i32 {
        for dc in -1..=1i32 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let r = row as i32 + dr;
            let c = col as i32 + dc;
            if r < 0 || c < 0 {
                continue;
            }
            if player_grid
                .get(r as usize)
                .and_then(|line| line.get(c as usize))
                .copied()
                .is_some_and(|cell| cell == CELL_FLAGGED || cell == CELL_MINE_HIT)
            {
                count = count.saturating_add(1);
            }
        }
    }
    count
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

pub fn hit_area(area: Rect, diff: &state::DifficultyConfig) -> Rect {
    let board_area = crate::app::arcade::ui::game_content_area(area, true, true);
    let content_width = 4 + diff.cols * 4;
    let board_w = (content_width as u16).min(board_area.width);
    let board_h = ((diff.rows as u16) * 2 + 2).min(board_area.height);
    centered_rect(board_area, board_w, board_h)
}

pub fn hit_test(
    area: Rect,
    diff: &state::DifficultyConfig,
    scroll_offset: u16,
    x: u16,
    y: u16,
) -> Option<(usize, usize)> {
    let board_rect = hit_area(area, diff);

    if x < board_rect.x || x >= board_rect.x + board_rect.width {
        return None;
    }
    if y < board_rect.y || y >= board_rect.y.saturating_add(board_rect.height) {
        return None;
    }

    let content_width = 4 + diff.cols * 4;
    if board_rect.width < content_width as u16 {
        return None;
    }
    let text_start_x = board_rect.x + (board_rect.width - (content_width as u16)) / 2;

    if x < text_start_x {
        return None;
    }

    let local_x = x - text_start_x;
    if local_x < 4 {
        return None;
    }
    let cell_offset = local_x - 4;
    let col = cell_offset / 4;
    let in_cell = cell_offset % 4 < 3;

    if !in_cell || col as usize >= diff.cols {
        return None;
    }

    let line = y.saturating_sub(board_rect.y).saturating_add(scroll_offset);
    if line < 2 || line >= 2 + (diff.rows as u16) * 2 {
        return None;
    }

    let board_row = (line - 2) as usize;
    if !board_row.is_multiple_of(2) || board_row / 2 >= diff.rows {
        return None;
    }

    Some((board_row / 2, col as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_preview_uses_subtle_glyph_without_background() {
        let mut glyph = " \u{00b7} ".to_string();
        let mut style = Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION());

        apply_chord_preview_style(&mut glyph, &mut style);

        assert_eq!(glyph, CHORD_PREVIEW_GLYPH);
        assert_eq!(style.fg, Some(theme::BORDER_DIM()));
        assert_eq!(style.bg, None);
    }

    fn board_origin(area: Rect, diff: &state::DifficultyConfig) -> (u16, u16) {
        let br = hit_area(area, diff);
        let content_width = 4 + diff.cols * 4;
        let text_start_x = br.x + (br.width - (content_width as u16)) / 2;
        (text_start_x + 4, br.y + 2)
    }

    #[test]
    fn hit_test_hits_cells() {
        for diff in &state::DIFFICULTIES {
            let area = Rect::new(0, 0, 120, 60);
            let (ox, oy) = board_origin(area, diff);
            assert_eq!(hit_test(area, diff, 0, ox, oy), Some((0, 0)));
            assert_eq!(
                hit_test(area, diff, 0, ox + (diff.cols as u16 - 1) * 4, oy),
                Some((0, diff.cols - 1))
            );
            assert_eq!(
                hit_test(area, diff, 0, ox, oy + (diff.rows as u16 - 1) * 2),
                Some((diff.rows - 1, 0))
            );
            if diff.cols > 1 {
                assert_eq!(hit_test(area, diff, 0, ox + 4, oy), Some((0, 1)));
            }
        }
    }

    #[test]
    fn hit_test_rejects_non_cell_area() {
        let diff = state::DIFFICULTIES[0];
        let area = Rect::new(0, 0, 80, 40);
        let (ox, oy) = board_origin(area, &diff);
        let br = hit_area(area, &diff);

        assert_eq!(
            hit_test(area, &diff, 0, ox + 3, oy),
            None,
            "vertical separator"
        );
        assert_eq!(
            hit_test(area, &diff, 0, ox, oy + 1),
            None,
            "horizontal separator"
        );
        assert_eq!(
            hit_test(area, &diff, 0, br.x + 1, br.y + 2),
            None,
            "row label"
        );
        assert_eq!(hit_test(area, &diff, 0, ox, oy - 1), None, "column header");
        assert_eq!(
            hit_test(area, &diff, 0, ox, oy + (diff.rows as u16 - 1) * 2 + 1),
            None,
            "bottom border"
        );
        assert_eq!(hit_test(area, &diff, 0, 0, 0), None, "top-left corner");
        assert_eq!(
            hit_test(area, &diff, 0, 79, 39),
            None,
            "bottom-right corner"
        );
    }
}
