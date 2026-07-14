//! Full-screen daily battleship board: your shots on their waters (left,
//! where the cursor lives) and your own fleet taking fire (right). Shares
//! the daily board chrome — status line, player bars, pinned key hints —
//! with the chess renderer in `board_ui`.

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
        battleship::{self, DailyBattleshipState, Shot},
        board_ui::{draw_center_message, name_for, result_banner},
        state::{BattleshipDetail, DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    },
};

/// title + column header + 10 rows + fleet summary.
const GRID_ROWS: u16 = 13;
/// Terminal columns per board cell; the mouse hit-test divides by this.
pub(crate) const CELL_W: u16 = 3;
/// row labels (3) + 10 cells.
const GRID_WIDTH: u16 = 3 + (battleship::GRID as u16) * CELL_W;
const GRID_GAP: u16 = 6;
const GRIDS_WIDTH: u16 = GRID_WIDTH * 2 + GRID_GAP;
/// status + two player bars + key hints around the grids.
const CHROME_ROWS: u16 = 4;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    battleship: &BattleshipDetail,
) {
    if area.width < GRIDS_WIDTH || area.height < GRID_ROWS + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }
    let state = &battleship.state;
    // A spectator isn't a player: they watch a ships-hidden view of both
    // players' waters (`top_side` / `bottom_side`), never a fleet.
    let me = state.side_index_of(daily.user_id());
    let (top_side, bottom_side) = match me {
        Some(me) => (DailyBattleshipState::opponent_index(me), me),
        None => (1, 0),
    };

    // Same shape as the chess board: the salvo rail splits off the right
    // edge when there is room, everything else centres in what remains.
    let show_rail = area.width >= GRIDS_WIDTH + INFO_RAIL_WIDTH + INFO_RAIL_MIN_EXTRA;
    let content = if show_rail {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_RAIL_WIDTH)])
            .split(area);
        draw_info_rail(frame, cols[1], daily, board, state);
        cols[0]
    } else {
        area
    };
    let area = content;

    let stack_h = GRID_ROWS + CHROME_ROWS;
    let top_pad = area.height.saturating_sub(stack_h) / 2;
    let rows = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(1),         // status
        Constraint::Length(1),         // opponent bar
        Constraint::Length(GRID_ROWS), // the two grids
        Constraint::Length(1),         // own bar
        Constraint::Min(0),            // slack, pushing the hints to the floor
        Constraint::Length(1),         // key hints
    ])
    .split(area);
    let (status_row, top_bar, grids_row, bottom_bar, hint_row) =
        (rows[1], rows[2], rows[3], rows[4], rows[6]);

    let finished = !detail.is_active();
    let my_turn = detail.is_active()
        && detail.row.turn_user_id == Some(daily.user_id())
        && !battleship.shot_in_flight;

    let grids_x = grids_row.x + grids_row.width.saturating_sub(GRIDS_WIDTH) / 2;
    // Player bars hug the grids block, not the screen edges — the same
    // centred-stack rule as the chess board's `centered_x` bars.
    let over_grids = |row: Rect| Rect {
        x: grids_x,
        y: row.y,
        width: GRIDS_WIDTH.min(row.width),
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, battleship)).alignment(Alignment::Center),
        status_row,
    );
    draw_player_bar(
        frame,
        over_grids(top_bar),
        daily,
        board,
        detail,
        battleship,
        top_side,
    );

    let target_rect = Rect {
        x: grids_x,
        y: grids_row.y,
        width: GRID_WIDTH,
        height: GRID_ROWS,
    };
    let fleet_rect = Rect {
        x: grids_x + GRID_WIDTH + GRID_GAP,
        y: grids_row.y,
        width: GRID_WIDTH,
        height: GRID_ROWS,
    };
    // A player sees their shots on the enemy (left) beside their own fleet
    // taking fire (right). A spectator sees both players' waters charted by
    // hits and misses only — the fleets stay hidden.
    let (left_lines, right_lines) = match me {
        Some(me) => (
            target_grid_lines(state, me, my_turn.then_some(board.cursor), finished),
            fleet_grid_lines(state, me),
        ),
        None => (
            spectate_waters_lines(
                state,
                top_side,
                name_for(board, state.side(top_side).user_id),
            ),
            spectate_waters_lines(
                state,
                bottom_side,
                name_for(board, state.side(bottom_side).user_id),
            ),
        ),
    };
    frame.render_widget(Paragraph::new(left_lines), target_rect);
    frame.render_widget(Paragraph::new(right_lines), fleet_rect);
    // Cells begin after the title + header rows and the row labels. Only a
    // player clicks to fire; a spectator's cursor never resolves.
    if me.is_some() {
        board.target_geometry.set(Some(Rect {
            x: target_rect.x + 3,
            y: target_rect.y + 2,
            width: (battleship::GRID as u16) * CELL_W,
            height: battleship::GRID as u16,
        }));
    }

    draw_player_bar(
        frame,
        over_grids(bottom_bar),
        daily,
        board,
        detail,
        battleship,
        bottom_side,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail)).alignment(Alignment::Center),
        hint_row,
    );
}

const INFO_RAIL_WIDTH: u16 = 24;
/// Breathing room required around the grids before the rail appears.
const INFO_RAIL_MIN_EXTRA: u16 = 8;

/// Their waters: your shots, the cursor, and (once the match ends) whatever
/// survived of their fleet.
fn target_grid_lines(
    state: &DailyBattleshipState,
    me: usize,
    cursor: Option<usize>,
    finished: bool,
) -> Vec<Line<'static>> {
    let them = DailyBattleshipState::opponent_index(me);
    let hot_col = cursor.map(|cell| cell % battleship::GRID);
    let hot_row = cursor.map(|cell| cell / battleship::GRID);
    let mut lines = vec![grid_title("their waters"), header_line(hot_col)];
    for row in 0..battleship::GRID {
        let mut spans = vec![row_label(row, hot_row == Some(row))];
        for col in 0..battleship::GRID {
            let cell = row * battleship::GRID + col;
            let shot = state
                .side(me)
                .shots
                .iter()
                .find(|shot| shot.cell as usize == cell);
            let enemy_ship = state
                .side(them)
                .ships
                .iter()
                .any(|ship| ship.cells.contains(&(cell as u8)));
            let hit = matches!(shot, Some(Shot { hit: true, .. }));
            let (full, mid, style) = match shot {
                // A solid red tile with a dark cross — readable from across
                // the room, unlike a lone red mark on black.
                Some(Shot { hit: true, .. }) => (" X ", 'X', hit_style()),
                Some(Shot { hit: false, .. }) => (
                    " x ",
                    'x',
                    checker(row, col)
                        .fg(theme::TEXT_MUTED())
                        .add_modifier(Modifier::BOLD),
                ),
                None if finished && enemy_ship => {
                    // The reveal: ships you never found.
                    ("░░░", '░', checker(row, col).fg(theme::TEXT_MUTED()))
                }
                None => (" · ", '·', checker(row, col).fg(theme::BORDER_DIM())),
            };
            if cursor == Some(cell) {
                let bracket = Style::default()
                    .fg(theme::AMBER())
                    .bg(theme::BG_SELECTION())
                    .add_modifier(Modifier::BOLD);
                let mut mid_style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
                if hit {
                    // The hit tile's dark-on-red inverts to red-on-selection
                    // under the cursor so the cross stays legible.
                    mid_style = mid_style.fg(theme::ERROR());
                }
                spans.push(Span::styled("[", bracket));
                spans.push(Span::styled(mid.to_string(), mid_style));
                spans.push(Span::styled("]", bracket));
            } else {
                spans.push(Span::styled(full.to_string(), style));
            }
        }
        lines.push(Line::from(spans));
    }
    let sunk = battleship::FLEET_LENGTHS.len() - state.ships_afloat_against(me);
    lines.push(summary_line(format!(
        "sunk {sunk}/{}",
        battleship::FLEET_LENGTHS.len()
    )));
    lines
}

/// Your fleet: ships, the hits they've taken, and their misses around them.
fn fleet_grid_lines(state: &DailyBattleshipState, me: usize) -> Vec<Line<'static>> {
    let them = DailyBattleshipState::opponent_index(me);
    let mut lines = vec![grid_title("your fleet"), header_line(None)];
    for row in 0..battleship::GRID {
        let mut spans = vec![row_label(row, false)];
        for col in 0..battleship::GRID {
            let cell = row * battleship::GRID + col;
            let shot = state
                .side(them)
                .shots
                .iter()
                .find(|shot| shot.cell as usize == cell);
            let my_ship = state
                .side(me)
                .ships
                .iter()
                .any(|ship| ship.cells.contains(&(cell as u8)));
            let (glyph, style) = match (my_ship, shot) {
                (true, Some(Shot { hit: true, .. })) => (" X ", hit_style()),
                (true, _) => ("███", Style::default().fg(theme::TEXT_DIM())),
                (false, Some(_)) => (
                    " x ",
                    checker(row, col)
                        .fg(theme::TEXT_MUTED())
                        .add_modifier(Modifier::BOLD),
                ),
                (false, None) => (" · ", checker(row, col).fg(theme::BORDER_DIM())),
            };
            spans.push(Span::styled(glyph.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    lines.push(summary_line(format!(
        "afloat {}/{}",
        state.ships_afloat_against(them),
        battleship::FLEET_LENGTHS.len()
    )));
    lines
}

/// A player's waters as their opponent has charted them: hit and miss marks
/// only, never the ships. This is exactly the public salvo record, so a
/// spectator learns nothing the shooter doesn't already know — the fleets
/// stay hidden even after the match ends.
fn spectate_waters_lines(
    state: &DailyBattleshipState,
    defender: usize,
    title: String,
) -> Vec<Line<'static>> {
    let attacker = DailyBattleshipState::opponent_index(defender);
    let mut lines = vec![grid_title(&title), header_line(None)];
    for row in 0..battleship::GRID {
        let mut spans = vec![row_label(row, false)];
        for col in 0..battleship::GRID {
            let cell = row * battleship::GRID + col;
            let shot = state
                .side(attacker)
                .shots
                .iter()
                .find(|shot| shot.cell as usize == cell);
            let (glyph, style) = match shot {
                Some(Shot { hit: true, .. }) => (" X ", hit_style()),
                Some(Shot { hit: false, .. }) => (
                    " x ",
                    checker(row, col)
                        .fg(theme::TEXT_MUTED())
                        .add_modifier(Modifier::BOLD),
                ),
                None => (" · ", checker(row, col).fg(theme::BORDER_DIM())),
            };
            spans.push(Span::styled(glyph.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    let sunk = battleship::FLEET_LENGTHS.len() - state.ships_afloat_against(attacker);
    lines.push(summary_line(format!(
        "sunk {sunk}/{}",
        battleship::FLEET_LENGTHS.len()
    )));
    lines
}

fn grid_title(title: &str) -> Line<'static> {
    let pad = (GRID_WIDTH as usize).saturating_sub(title.chars().count()) / 2;
    Line::from(Span::styled(
        format!("{}{title}", " ".repeat(pad)),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::ITALIC),
    ))
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

/// A hit: dark cross on a solid error-colored tile.
fn hit_style() -> Style {
    Style::default()
        .fg(theme::BG_CANVAS())
        .bg(theme::ERROR())
        .add_modifier(Modifier::BOLD)
}

/// `hot_col` lights up the cursor's column letter as a crosshair.
fn header_line(hot_col: Option<usize>) -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];
    for col in 0..battleship::GRID {
        let letter = (b'A' + col as u8) as char;
        let style = if hot_col == Some(col) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        spans.push(Span::styled(format!(" {letter} "), style));
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

fn summary_line(text: String) -> Line<'static> {
    let pad = (GRID_WIDTH as usize).saturating_sub(text.chars().count()) / 2;
    Line::from(Span::styled(
        format!("{}{text}", " ".repeat(pad)),
        Style::default().fg(theme::TEXT_FAINT()),
    ))
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    battleship: &BattleshipDetail,
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
        if battleship.shot_in_flight {
            spans.push(Span::styled(
                "Shot away…",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else if detail.row.turn_user_id == Some(daily.user_id()) {
            spans.push(Span::styled(
                "Your shot",
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
    if let Some((by, shot)) = last_salvo(&battleship.state) {
        let shooter = battleship.state.side(by).user_id;
        let who = if shooter == daily.user_id() {
            "you".to_string()
        } else {
            name_for(board, shooter)
        };
        spans.push(Span::styled(
            format!(
                "   last {who} {} {}",
                battleship::cell_label(shot.cell as usize),
                if shot.hit { "hit" } else { "miss" }
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

/// `● mira   3/5 afloat`, with the running deadline on the mover's bar.
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    battleship: &BattleshipDetail,
    side: usize,
) {
    if rect.height == 0 {
        return;
    }
    let state = &battleship.state;
    let user_id = state.side(side).user_id;
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
    let afloat = state.ships_afloat_against(DailyBattleshipState::opponent_index(side));
    let left = vec![
        Span::raw("  "),
        Span::styled("\u{25CF} ", Style::default().fg(dot_color)),
        Span::styled(
            name,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   {afloat}/{} afloat", battleship::FLEET_LENGTHS.len()),
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
            "watching · fleets hidden   ".to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    } else if detail.is_active() {
        hint(&mut spans, "arrows/wasd", "aim");
        hint(&mut spans, "Space/Enter", "fire");
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

/// Most recent shot across both sides.
fn last_salvo(state: &DailyBattleshipState) -> Option<(usize, &Shot)> {
    [0usize, 1]
        .into_iter()
        .filter_map(|side| state.side(side).shots.last().map(|shot| (side, shot)))
        .max_by_key(|(_, shot)| shot.at)
}

/// Salvo history rail: every shot from both sides, newest at the bottom,
/// same slot the chess move list occupies.
fn draw_info_rail(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    state: &DailyBattleshipState,
) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Correspondence battleship".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "a hit fires again".to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Salvos".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let mut salvos: Vec<(usize, &Shot)> = (0..2)
        .flat_map(|side| state.side(side).shots.iter().map(move |shot| (side, shot)))
        .collect();
    salvos.sort_by_key(|(_, shot)| shot.at);

    let budget = (area.height as usize).saturating_sub(lines.len());
    if salvos.is_empty() {
        lines.push(Line::from(Span::styled(
            "no shots yet",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        if salvos.len() > budget && budget > 0 {
            lines.push(Line::from(Span::styled(
                "  \u{22EE}",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
            let skip = salvos.len() - (budget - 1);
            salvos.drain(..skip);
        }
        for (side, shot) in salvos {
            let shooter = state.side(side).user_id;
            let who = if shooter == daily.user_id() {
                "you".to_string()
            } else {
                name_for(board, shooter)
            };
            let (mark, mark_color) = if shot.hit {
                ("X", theme::ERROR())
            } else {
                ("x", theme::TEXT_MUTED())
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{who:<9}"), Style::default().fg(theme::TEXT())),
                Span::styled(
                    format!("{:<4}", battleship::cell_label(shot.cell as usize)),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled(mark.to_string(), Style::default().fg(mark_color)),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}
