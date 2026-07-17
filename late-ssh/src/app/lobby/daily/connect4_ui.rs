//! Full-screen daily connect four board: one gravity grid with the cursor
//! sliding along the columns and a landing preview in the hovered one.
//! Shares the daily board chrome — status line, player bars, pinned key
//! hints — with the chess and battleship renderers.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    lobby::daily::{
        board_ui::{
            CellTier, PUCK_SOLID, cell_text, draw_center_message, hint_cell, name_for,
            pick_cell_tier, piece_cell, result_banner,
        },
        connect4::{self, DailyConnect4State, Disc},
        state::{Connect4Detail, DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    },
};

/// column header + drop indicator + board rows + drop count.
fn grid_rows(tier: CellTier) -> u16 {
    2 + connect4::ROWS as u16 * tier.ch + 1
}

/// row labels (3) + 7 cells.
fn grid_width(tier: CellTier) -> u16 {
    3 + connect4::COLS as u16 * tier.cw
}

/// status + two player bars + key hints around the grid.
const CHROME_ROWS: u16 = 4;

const INFO_RAIL_WIDTH: u16 = 24;
/// Breathing room required around the grid before the rail appears.
const INFO_RAIL_MIN_EXTRA: u16 = 8;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    connect4: &Connect4Detail,
) {
    let tier = pick_cell_tier(|tier| {
        grid_width(tier) <= area.width && grid_rows(tier) + CHROME_ROWS <= area.height
    });
    if area.width < grid_width(tier) || area.height < grid_rows(tier) + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }
    let state = &connect4.state;
    // Spectators aren't a player; default them to red's perspective (red on
    // the bottom bar). Connect four hides nothing, so the view is complete
    // and the ghost preview never draws (a spectator's cursor stays off).
    let my_disc = state.disc_of(daily.user_id()).unwrap_or(Disc::Red);

    // Same shape as the other boards: the drop rail splits off the right
    // edge when there is room, everything else centres in what remains.
    let show_rail = area.width >= grid_width(tier) + INFO_RAIL_WIDTH + INFO_RAIL_MIN_EXTRA;
    let content = if show_rail {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_RAIL_WIDTH)])
            .split(area);
        draw_info_rail(frame, cols[1], daily, board, state);
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
        && !connect4.drop_in_flight;

    let grid_x = grid_row.x + grid_row.width.saturating_sub(grid_width(tier)) / 2;
    // Player bars hug the grid, not the screen edges — the same
    // centred-stack rule as the other boards.
    let over_grid = |row: Rect| Rect {
        x: grid_x,
        y: row.y,
        width: grid_width(tier).min(row.width),
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, connect4)).alignment(Alignment::Center),
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
    // Cells begin after the header + indicator rows and the row labels.
    board.target_geometry.set(Some(Rect {
        x: grid_rect.x + 3,
        y: grid_rect.y + 2,
        width: connect4::COLS as u16 * tier.cw,
        height: connect4::ROWS as u16 * tier.ch,
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

fn disc_color(disc: Disc) -> Color {
    match disc {
        Disc::Red => theme::ERROR(),
        Disc::Yellow => theme::AMBER(),
    }
}

/// The board: header letters, the drop indicator, six rows bottom-up, and
/// the running drop count. The winning line lights up as solid tiles. Each
/// board row is `tier.ch` text lines; the glyph and row label sit on the
/// middle one, the rest just carry the cell background.
fn board_lines(
    state: &DailyConnect4State,
    cursor: Option<usize>,
    my_disc: Disc,
    finished: bool,
    tier: CellTier,
) -> Vec<Line<'static>> {
    let grid = state.grid();
    let last = state.last_drop();
    let winning = finished
        .then(|| state.winning_line())
        .flatten()
        .unwrap_or_default();
    // Where the hovered column would take a disc, for the ghost preview.
    let landing = cursor.and_then(|col| (0..connect4::ROWS).find(|&row| grid[row][col].is_none()));

    let mut lines = vec![header_line(cursor, tier), indicator_line(cursor, tier)];
    for row in (0..connect4::ROWS).rev() {
        for sub in 0..tier.ch {
            let glyph_row = sub == tier.glyph_sub();
            let mut spans = vec![if glyph_row {
                row_label(row, landing == Some(row))
            } else {
                Span::raw("   ")
            }];
            for (col, cell) in grid[row].iter().enumerate() {
                let span = match *cell {
                    Some(disc) if winning.contains(&(row, col)) => Span::styled(
                        // The line that ended it: dark discs on solid tiles.
                        piece_cell(PUCK_SOLID, '●', tier, sub),
                        Style::default()
                            .fg(theme::BG_CANVAS())
                            .bg(disc_color(disc))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Some(disc) if last == Some((row, col)) => Span::styled(
                        piece_cell(PUCK_SOLID, '●', tier, sub),
                        Style::default()
                            .fg(disc_color(disc))
                            .bg(theme::BG_SELECTION())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Some(disc) => Span::styled(
                        piece_cell(PUCK_SOLID, '●', tier, sub),
                        checker(row, col).fg(disc_color(disc)),
                    ),
                    None if cursor == Some(col) && landing == Some(row) => Span::styled(
                        // Where the drop would land: the corner frame in
                        // your colour.
                        hint_cell('◌', tier, sub),
                        checker(row, col)
                            .fg(disc_color(my_disc))
                            .add_modifier(Modifier::BOLD),
                    ),
                    None => Span::styled(" ".repeat(tier.cw as usize), checker(row, col)),
                };
                spans.push(span);
            }
            lines.push(Line::from(spans));
        }
    }
    lines.push(summary_line(format!("{} drops", state.move_count()), tier));
    lines
}

/// Alternating cell background — the checkerboard is what makes the grid
/// readable at a glance without drawing actual rules.
fn checker(row: usize, col: usize) -> Style {
    if (row + col).is_multiple_of(2) {
        Style::default().bg(theme::BG_HIGHLIGHT())
    } else {
        Style::default()
    }
}

/// `hot_col` lights up the cursor's column letter as a crosshair.
fn header_line(hot_col: Option<usize>, tier: CellTier) -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];
    for col in 0..connect4::COLS {
        let style = if hot_col == Some(col) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        spans.push(Span::styled(
            cell_text(connect4::column_label(col), tier.cw),
            style,
        ));
    }
    Line::from(spans)
}

/// The `▼` hovering over the cursor column. Blank off-turn: the row keeps
/// its slot so the grid never shifts.
fn indicator_line(hot_col: Option<usize>, tier: CellTier) -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];
    for col in 0..connect4::COLS {
        if hot_col == Some(col) {
            spans.push(Span::styled(
                cell_text('▼', tier.cw),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw(" ".repeat(tier.cw as usize)));
        }
    }
    Line::from(spans)
}

fn row_label(row: usize, hot: bool) -> Span<'static> {
    let style = if hot {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    Span::styled(format!("{:>2} ", row + 1), style)
}

fn summary_line(text: String, tier: CellTier) -> Line<'static> {
    let pad = (grid_width(tier) as usize).saturating_sub(text.chars().count()) / 2;
    Line::from(Span::styled(
        format!("{}{text}", " ".repeat(pad)),
        Style::default().fg(theme::TEXT_FAINT()),
    ))
}

/// `(user, "d3")` for the most recent drop.
fn last_drop_feed(state: &DailyConnect4State) -> Option<(Uuid, String)> {
    let (row, col) = state.last_drop()?;
    let disc = if (state.move_count() - 1).is_multiple_of(2) {
        Disc::Red
    } else {
        Disc::Yellow
    };
    Some((
        state.user_of(disc),
        format!("{}{}", connect4::column_label(col), row + 1),
    ))
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    connect4: &Connect4Detail,
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
        if connect4.drop_in_flight {
            spans.push(Span::styled(
                "Drop away…",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else if detail.row.turn_user_id == Some(daily.user_id()) {
            spans.push(Span::styled(
                "Your drop",
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
    if let Some((by, spot)) = last_drop_feed(&connect4.state) {
        let who = if by == daily.user_id() {
            "you".to_string()
        } else {
            name_for(board, by)
        };
        spans.push(Span::styled(
            format!("   last {who} {spot}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

/// `● red mira`, with the running deadline on the mover's bar.
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyConnect4State,
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
    let left = vec![
        Span::raw("  "),
        Span::styled("\u{25CF} ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{} ", disc.label()),
            Style::default()
                .fg(disc_color(disc))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(name, Style::default().fg(theme::TEXT())),
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
        hint(&mut spans, "arrows/wasd", "choose column");
        hint(&mut spans, "Space/Enter", "drop");
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

/// Drop history rail: every disc in play order, newest at the bottom, same
/// slot the chess move list occupies.
fn draw_info_rail(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    state: &DailyConnect4State,
) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Correspondence connect four".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "four in a row wins".to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Drops".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
    ];

    // Replay the history to recover each drop's landing row and disc.
    let mut heights = [0usize; connect4::COLS];
    let mut drops: Vec<(Disc, String)> = Vec::with_capacity(state.drops.len());
    for (index, &col) in state.drops.iter().enumerate() {
        let col = col as usize;
        let disc = if index.is_multiple_of(2) {
            Disc::Red
        } else {
            Disc::Yellow
        };
        drops.push((
            disc,
            format!("{}{}", connect4::column_label(col), heights[col] + 1),
        ));
        heights[col] += 1;
    }

    let budget = (area.height as usize).saturating_sub(lines.len());
    if drops.is_empty() {
        lines.push(Line::from(Span::styled(
            "no drops yet",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        if drops.len() > budget && budget > 0 {
            lines.push(Line::from(Span::styled(
                "  \u{22EE}",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
            let skip = drops.len() - (budget - 1);
            drops.drain(..skip);
        }
        for (disc, spot) in drops {
            let who = if state.user_of(disc) == daily.user_id() {
                "you".to_string()
            } else {
                name_for(board, state.user_of(disc))
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{who:<9}"), Style::default().fg(theme::TEXT())),
                Span::styled(format!("{spot:<4}"), Style::default().fg(theme::TEXT_DIM())),
                Span::styled("●".to_string(), Style::default().fg(disc_color(disc))),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}
