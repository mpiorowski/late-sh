//! Full-screen daily checkers board: one 8x8 grid on the dark squares, a cell
//! cursor, the in-progress move path lit up, and legal next squares hinted so
//! multi-jumps can be built click by click. Shares the daily board chrome —
//! status line, player bars, pinned key hints — with the other renderers.
//! Pieces are the shared square stone, red warm / white bright, kings with a
//! gold top half; the cramped tier falls back to `●` men and `◉` kings.

use std::collections::HashSet;

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color as TermColor, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    lobby::daily::{
        board_ui::{
            CellTier, PUCK_SOLID, cell_text, draw_center_message, hint_cell, name_for,
            pick_cell_tier, piece_cell, result_banner,
        },
        checkers::{self, Color, DailyCheckersState, Piece},
        state::{CheckersDetail, DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    },
};

/// header row + board rows + summary row.
fn grid_rows(tier: CellTier) -> u16 {
    1 + checkers::SIZE as u16 * tier.ch + 1
}

/// row labels (3) + 8 cells.
fn grid_width(tier: CellTier) -> u16 {
    3 + checkers::SIZE as u16 * tier.cw
}

/// status + two player bars + key hints around the grid.
const CHROME_ROWS: u16 = 4;

const INFO_RAIL_WIDTH: u16 = 24;
const INFO_RAIL_MIN_EXTRA: u16 = 8;

fn color_fg(color: Color) -> TermColor {
    match color {
        Color::Red => theme::ERROR(),
        Color::White => theme::TEXT_BRIGHT(),
    }
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    checkers: &CheckersDetail,
) {
    let tier = pick_cell_tier(|tier| {
        grid_width(tier) <= area.width && grid_rows(tier) + CHROME_ROWS <= area.height
    });
    if area.width < grid_width(tier) || area.height < grid_rows(tier) + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }
    let state = &checkers.state;
    let my_color = state.color_of(daily.user_id());

    let show_rail = area.width >= grid_width(tier) + INFO_RAIL_WIDTH + INFO_RAIL_MIN_EXTRA;
    let content = if show_rail {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_RAIL_WIDTH)])
            .split(area);
        draw_info_rail(frame, cols[1], state);
        cols[0]
    } else {
        area
    };
    let area = content;

    let stack_h = grid_rows(tier) + CHROME_ROWS;
    let top_pad = area.height.saturating_sub(stack_h) / 2;
    let rows = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(1),               // status
        Constraint::Length(1),               // opponent bar
        Constraint::Length(grid_rows(tier)), // the grid
        Constraint::Length(1),               // own bar
        Constraint::Min(0),                  // slack, pushing the hints to the floor
        Constraint::Length(1),               // key hints
    ])
    .split(area);
    let (status_row, top_bar, grid_row, bottom_bar, hint_row) =
        (rows[1], rows[2], rows[3], rows[4], rows[6]);

    let my_turn = detail.is_active()
        && detail.row.turn_user_id == Some(daily.user_id())
        && !checkers.move_in_flight;

    // Legal moves for the side to move, as cell-index paths, to light up the
    // in-progress path and hint the next squares.
    let legal: Vec<Vec<usize>> = match (my_turn, my_color) {
        (true, Some(color)) => state
            .legal_moves(color)
            .into_iter()
            .map(|path| path.into_iter().map(cell_index).collect())
            .collect(),
        _ => Vec::new(),
    };
    let pending = &checkers.pending;
    let pending_set: HashSet<usize> = pending.iter().copied().collect();
    let mut next_steps: HashSet<usize> = HashSet::new();
    for path in &legal {
        if path.len() > pending.len() && path[..pending.len()] == pending[..] {
            next_steps.insert(path[pending.len()]);
        }
    }

    let grid_x = grid_row.x + grid_row.width.saturating_sub(grid_width(tier)) / 2;
    let over_grid = |row: Rect| Rect {
        x: grid_x,
        y: row.y,
        width: grid_width(tier).min(row.width),
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, checkers)).alignment(Alignment::Center),
        status_row,
    );
    // The top bar belongs to whichever colour the local player is NOT.
    let bottom_color = my_color.unwrap_or(Color::Red);
    draw_player_bar(
        frame,
        over_grid(top_bar),
        daily,
        board,
        detail,
        state,
        bottom_color.other(),
    );

    let grid_rect = Rect {
        x: grid_x,
        y: grid_row.y,
        width: grid_width(tier),
        height: grid_rows(tier),
    };
    frame.render_widget(
        Paragraph::new(board_lines(
            state,
            my_turn.then_some(board.cursor),
            &pending_set,
            &next_steps,
            tier,
        )),
        grid_rect,
    );
    // Cells begin after the header row and the row labels; row 0 is at the top.
    board.target_geometry.set(Some(Rect {
        x: grid_rect.x + 3,
        y: grid_rect.y + 1,
        width: checkers::SIZE as u16 * tier.cw,
        height: checkers::SIZE as u16 * tier.ch,
    }));

    draw_player_bar(
        frame,
        over_grid(bottom_bar),
        daily,
        board,
        detail,
        state,
        bottom_color,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail)).alignment(Alignment::Center),
        hint_row,
    );
}

fn cell_index((row, col): (usize, usize)) -> usize {
    row * checkers::SIZE + col
}

fn piece_glyph(piece: Piece) -> char {
    if piece.king { '◉' } else { '●' }
}

/// The board: header letters then eight rows top-down (row 0 at the top). Only
/// the dark squares are in play; the move path is lit and legal next squares
/// are hinted. Each board row is `tier.ch` text lines; the glyph and row label
/// sit on the middle one, the rest just carry the cell background.
fn board_lines(
    state: &DailyCheckersState,
    cursor: Option<usize>,
    pending: &HashSet<usize>,
    next_steps: &HashSet<usize>,
    tier: CellTier,
) -> Vec<Line<'static>> {
    let grid = state.grid();
    let last: HashSet<usize> = state
        .last_move()
        .unwrap_or_default()
        .into_iter()
        .map(cell_index)
        .collect();

    let mut lines = vec![header_line(cursor.map(|i| i % checkers::SIZE), tier)];
    for row in 0..checkers::SIZE {
        for sub in 0..tier.ch {
            let glyph_row = sub == tier.glyph_sub();
            let mut spans = vec![if glyph_row {
                row_label(row)
            } else {
                Span::raw("   ")
            }];
            for col in 0..checkers::SIZE {
                let index = row * checkers::SIZE + col;
                let playable = !(row + col).is_multiple_of(2);
                let is_cursor = cursor == Some(index);
                // Background precedence: cursor, then the picked path, then the
                // last move, then a legal next square, then the plain dark square.
                let mut style = Style::default();
                if playable {
                    style = style.bg(theme::BG_HIGHLIGHT());
                }
                if last.contains(&index) {
                    style = style.bg(theme::BG_SELECTION());
                }
                if next_steps.contains(&index) {
                    style = style.bg(theme::AMBER_DIM());
                }
                if pending.contains(&index) {
                    style = style.bg(theme::BG_SELECTION());
                }
                if is_cursor {
                    style = style.bg(theme::AMBER_DIM());
                }
                let span = match grid[row][col] {
                    Some(piece) => {
                        // A king is the same square with a gold top half —
                        // the crown is a colour, not a shape.
                        let fg = if piece.king && tier.ch == 2 && sub == 0 {
                            theme::AMBER()
                        } else {
                            color_fg(piece.color)
                        };
                        Span::styled(
                            piece_cell(PUCK_SOLID, piece_glyph(piece), tier, sub),
                            style.fg(fg).add_modifier(Modifier::BOLD),
                        )
                    }
                    // The next squares of a move wear the corner frame.
                    None if next_steps.contains(&index) => Span::styled(
                        hint_cell('·', tier, sub),
                        style.fg(theme::AMBER()).add_modifier(Modifier::BOLD),
                    ),
                    None => Span::styled(" ".repeat(tier.cw as usize), style),
                };
                spans.push(span);
            }
            lines.push(Line::from(spans));
        }
    }
    let (red, white) = state.piece_counts();
    lines.push(summary_line(
        format!("● {red}   ● {white}   {} moves", state.move_count()),
        tier,
    ));
    lines
}

fn header_line(hot_col: Option<usize>, tier: CellTier) -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];
    for col in 0..checkers::SIZE {
        let style = if hot_col == Some(col) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        spans.push(Span::styled(
            cell_text((b'a' + col as u8) as char, tier.cw),
            style,
        ));
    }
    Line::from(spans)
}

fn row_label(row: usize) -> Span<'static> {
    Span::styled(
        format!("{:>2} ", row + 1),
        Style::default().fg(theme::TEXT_FAINT()),
    )
}

fn summary_line(text: String, tier: CellTier) -> Line<'static> {
    let pad = (grid_width(tier) as usize).saturating_sub(text.chars().count()) / 2;
    Line::from(Span::styled(
        format!("{}{text}", " ".repeat(pad)),
        Style::default().fg(theme::TEXT_FAINT()),
    ))
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    checkers: &CheckersDetail,
) -> Line<'static> {
    if board.resign_confirm {
        return Line::from(Span::styled(
            "Resign this match? Press r again to confirm.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let mut spans = Vec::new();
    if detail.is_active() {
        if checkers.move_in_flight {
            spans.push(Span::styled(
                "Moving…",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else if detail.row.turn_user_id == Some(daily.user_id()) {
            let text = if checkers.pending.is_empty() {
                "Your move"
            } else {
                "Pick a square · Esc cancels"
            };
            spans.push(Span::styled(
                text,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(
                    "Waiting for {}",
                    name_for(board, detail.row.turn_user_id.unwrap_or(Uuid::nil()))
                ),
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(deadline) = detail.row.turn_deadline_at {
            spans.push(Span::styled(
                format!("   {} on the clock", format_deadline(deadline, Utc::now())),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    } else {
        let (heading, subtitle, color) = result_banner(daily, board, detail);
        spans.push(Span::styled(
            format!("{heading} · {subtitle}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

/// `● red mira · 12`, with the running deadline on the mover's bar.
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyCheckersState,
    color: Color,
) {
    if rect.height == 0 {
        return;
    }
    let user_id = state.user_of(color);
    let on_turn = detail.is_active() && detail.row.turn_user_id == Some(user_id);
    let dot_color = if on_turn {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_FAINT()
    };
    let name = if user_id == daily.user_id() {
        "you".to_string()
    } else {
        name_for(board, user_id)
    };
    let (red, white) = state.piece_counts();
    let count = match color {
        Color::Red => red,
        Color::White => white,
    };
    let left = vec![
        Span::raw("  "),
        Span::styled("\u{25CF} ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{} ", color.label()),
            Style::default()
                .fg(color_fg(color))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(name, Style::default().fg(theme::TEXT())),
        Span::styled(
            format!("   {count}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    let deadline = on_turn
        .then_some(detail.row.turn_deadline_at)
        .flatten()
        .map(|at| format_deadline(at, Utc::now()));
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(9)]).split(rect);
    frame.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    if let Some(deadline) = deadline {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{deadline} "),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            cols[1],
        );
    }
}

fn key_line(board: &DailyBoardState, detail: &DailyMatchDetail) -> Line<'static> {
    let mut spans = Vec::new();
    let hint = |spans: &mut Vec<Span<'static>>, key: &str, desc: &str| {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
        spans.push(Span::styled(
            format!(" {desc}   "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    };
    if board.spectating {
        spans.push(Span::styled(
            "watching   ".to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    } else if detail.is_active() {
        hint(&mut spans, "arrows/wasd", "move cursor");
        hint(&mut spans, "Space/Enter", "pick / play");
        hint(&mut spans, "r", "resign");
    }
    if !board.spectating && detail.row.chat_room_id.is_some() {
        hint(&mut spans, "i", "chat");
    }
    hint(&mut spans, "Esc", "back to lobby");
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

fn draw_info_rail(frame: &mut Frame, area: Rect, state: &DailyCheckersState) {
    let (red, white) = state.piece_counts();
    let lines = vec![
        Line::from(Span::styled(
            "Checkers".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "capture or block to win".to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Pieces".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("● red    ", Style::default().fg(color_fg(Color::Red))),
            Span::styled(format!("{red}"), Style::default().fg(theme::TEXT())),
        ]),
        Line::from(vec![
            Span::styled("● white  ", Style::default().fg(color_fg(Color::White))),
            Span::styled(format!("{white}"), Style::default().fg(theme::TEXT())),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "◉ = king".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
