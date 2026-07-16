//! Full-screen daily reversi board: one 8x8 grid with a cell cursor, legal
//! squares hinted for the side to move, and a ghost disc under the cursor.
//! Shares the daily board chrome — status line, player bars, pinned key hints
//! — with the chess, battleship, and connect four renderers. Black discs are
//! the solid square stone, white the square-with-a-hole, both bright — the
//! same solid-vs-ring pair as the cramped tier's `●` vs `○`.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    lobby::daily::{
        board_ui::{
            CellTier, PUCK_HOLLOW, PUCK_SOLID, cell_text, draw_center_message, hint_cell, name_for,
            pick_cell_tier, piece_cell, result_banner,
        },
        reversi::{self, DailyReversiState, Disc},
        state::{DailyBoardState, DailyMatchDetail, DailyState, ReversiDetail, format_deadline},
    },
};

/// header row + board rows + summary row.
fn grid_rows(tier: CellTier) -> u16 {
    1 + reversi::SIZE as u16 * tier.ch + 1
}

/// row labels (3) + 8 cells.
fn grid_width(tier: CellTier) -> u16 {
    3 + reversi::SIZE as u16 * tier.cw
}

/// status + two player bars + key hints around the grid.
const CHROME_ROWS: u16 = 4;

const INFO_RAIL_WIDTH: u16 = 24;
const INFO_RAIL_MIN_EXTRA: u16 = 8;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    reversi: &ReversiDetail,
) {
    let tier = pick_cell_tier(|tier| {
        grid_width(tier) <= area.width && grid_rows(tier) + CHROME_ROWS <= area.height
    });
    if area.width < grid_width(tier) || area.height < grid_rows(tier) + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }
    let state = &reversi.state;
    // Spectators aren't a player; default them to black's perspective. Reversi
    // hides nothing, so the view is complete and the cursor never shows.
    let my_disc = state.disc_of(daily.user_id()).unwrap_or(Disc::Black);

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

    let finished = !detail.is_active();
    let my_turn = detail.is_active()
        && detail.row.turn_user_id == Some(daily.user_id())
        && !reversi.move_in_flight;

    let grid_x = grid_row.x + grid_row.width.saturating_sub(grid_width(tier)) / 2;
    let over_grid = |row: Rect| Rect {
        x: grid_x,
        y: row.y,
        width: grid_width(tier).min(row.width),
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, reversi)).alignment(Alignment::Center),
        status_row,
    );
    draw_player_bar(
        frame,
        over_grid(top_bar),
        daily,
        board,
        detail,
        state,
        my_disc.other(),
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
            my_disc,
            finished,
            tier,
        )),
        grid_rect,
    );
    // Cells begin after the header row and the row labels; row 0 is at the top.
    board.target_geometry.set(Some(Rect {
        x: grid_rect.x + 3,
        y: grid_rect.y + 1,
        width: reversi::SIZE as u16 * tier.cw,
        height: reversi::SIZE as u16 * tier.ch,
    }));

    draw_player_bar(
        frame,
        over_grid(bottom_bar),
        daily,
        board,
        detail,
        state,
        my_disc,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail)).alignment(Alignment::Center),
        hint_row,
    );
}

/// Alternating cell background so the grid reads at a glance.
fn checker(row: usize, col: usize) -> Style {
    if (row + col).is_multiple_of(2) {
        Style::default().bg(theme::BG_HIGHLIGHT())
    } else {
        Style::default()
    }
}

fn disc_glyph(disc: Disc) -> char {
    match disc {
        Disc::Black => '●',
        Disc::White => '○',
    }
}

/// Solid vs the square-with-a-hole: the art-tier `●`/`○`, full contrast on
/// any palette.
fn disc_art(disc: Disc) -> [&'static str; 2] {
    match disc {
        Disc::Black => PUCK_SOLID,
        Disc::White => PUCK_HOLLOW,
    }
}

/// The board: header letters then eight rows top-down (row 0 at the top),
/// with legal squares hinted and a ghost disc under the cursor. Each board
/// row is `tier.ch` text lines; the glyph and row label sit on the middle
/// one, the rest just carry the cell background.
fn board_lines(
    state: &DailyReversiState,
    cursor: Option<usize>,
    my_disc: Disc,
    _finished: bool,
    tier: CellTier,
) -> Vec<Line<'static>> {
    let grid = state.grid();
    let last = state.last_move();
    let cursor_rc = cursor.map(|i| (i / reversi::SIZE, i % reversi::SIZE));
    // Legal squares to hint, only while it's a real move to make.
    let legal = if cursor.is_some() {
        state.legal_moves(my_disc)
    } else {
        Vec::new()
    };

    let mut lines = vec![header_line(cursor_rc.map(|(_, col)| col), tier)];
    for row in 0..reversi::SIZE {
        for sub in 0..tier.ch {
            let glyph_row = sub == tier.glyph_sub();
            let mut spans = vec![if glyph_row {
                row_label(row)
            } else {
                Span::raw("   ")
            }];
            for col in 0..reversi::SIZE {
                let is_cursor = cursor_rc == Some((row, col));
                let is_legal = legal.contains(&(row, col));
                let mut style = checker(row, col);
                if last == Some((row, col)) {
                    style = style.bg(theme::BG_SELECTION());
                }
                if is_cursor {
                    style = style.bg(theme::AMBER_DIM());
                }
                let span = match grid[row][col] {
                    Some(disc) => Span::styled(
                        piece_cell(disc_art(disc), disc_glyph(disc), tier, sub),
                        style.fg(theme::TEXT_BRIGHT()).add_modifier(Modifier::BOLD),
                    ),
                    // Playable squares wear a corner frame; under the cursor
                    // it brightens (the amber cell background marks the spot).
                    None if is_cursor && is_legal => Span::styled(
                        hint_cell('◌', tier, sub),
                        style.fg(theme::TEXT_BRIGHT()).add_modifier(Modifier::BOLD),
                    ),
                    None if is_legal => {
                        Span::styled(hint_cell('·', tier, sub), style.fg(theme::AMBER()))
                    }
                    None => Span::styled(" ".repeat(tier.cw as usize), style),
                };
                spans.push(span);
            }
            lines.push(Line::from(spans));
        }
    }
    let (black, white) = state.disc_counts();
    lines.push(summary_line(
        format!("● {black}   ○ {white}   {} moves", state.move_count()),
        tier,
    ));
    lines
}

fn header_line(hot_col: Option<usize>, tier: CellTier) -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];
    for col in 0..reversi::SIZE {
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
    reversi: &ReversiDetail,
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
        if reversi.move_in_flight {
            spans.push(Span::styled(
                "Placing…",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else if detail.row.turn_user_id == Some(daily.user_id()) {
            spans.push(Span::styled(
                "Your move",
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

/// `● black mira · 12`, with the running deadline on the mover's bar.
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyReversiState,
    disc: Disc,
) {
    if rect.height == 0 {
        return;
    }
    let user_id = state.user_of(disc);
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
    let (black, white) = state.disc_counts();
    let count = match disc {
        Disc::Black => black,
        Disc::White => white,
    };
    let swatch = match disc {
        Disc::Black => "\u{25CF} ",
        Disc::White => "\u{25CB} ",
    };
    let left = vec![
        Span::raw("  "),
        Span::styled(swatch.to_string(), Style::default().fg(dot_color)),
        Span::styled(
            format!("{} ", disc.label()),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
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
        hint(&mut spans, "Space/Enter", "place");
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

fn draw_info_rail(frame: &mut Frame, area: Rect, state: &DailyReversiState) {
    let (black, white) = state.disc_counts();
    let lines = vec![
        Line::from(Span::styled(
            "Correspondence reversi".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "most discs wins".to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Discs".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("● black  ", Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled(format!("{black}"), Style::default().fg(theme::TEXT())),
        ]),
        Line::from(vec![
            Span::styled("○ white  ", Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled(format!("{white}"), Style::default().fg(theme::TEXT())),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}
