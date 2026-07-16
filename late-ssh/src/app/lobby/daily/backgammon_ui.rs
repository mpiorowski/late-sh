//! Full-screen daily backgammon board: two rows of six point columns around
//! the bar, the off tray at the right edge, checker stacks growing toward
//! the middle, and the pending roll drawn as dice between the halves. The
//! cursor walks the same 2 x 14 visual slot grid the mouse hit-test and
//! `backgammon::slot_target` use; a lifted checker's legal landings glow,
//! and pending hops are previewed on the board before the turn is sent.
//! Shares the daily board chrome — status line, player bars, pinned key
//! hints — with the other renderers. The board is drawn from the viewer's
//! seat (home board bottom-right); spectators get white's.

use std::collections::HashSet;

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color as TermColor, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    lobby::daily::{
        backgammon::{
            self, BAR_COL, BgTarget, Board, Color, DailyBackgammonState, SLOT_COLS, Turn,
            point_name, slot_target,
        },
        board_ui::{draw_center_message, name_for, result_banner},
        state::{BackgammonDetail, DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    },
};

/// Terminal columns per visual board column.
const COL_W: u16 = 3;
/// Checker stack rows per half; taller stacks collapse into a count.
const HALF_ROWS: u16 = 6;
const GRID_W: u16 = SLOT_COLS as u16 * COL_W;
/// header numbers + top stacks + two gap rows + bottom stacks + footer.
const GRID_H: u16 = 1 + HALF_ROWS + 2 + HALF_ROWS + 1;
/// status + two player bars + key hints around the grid.
const CHROME_ROWS: u16 = 4;

const INFO_RAIL_WIDTH: u16 = 24;
const INFO_RAIL_MIN_EXTRA: u16 = 8;

const CHECKER: char = '●';

fn color_fg(color: Color) -> TermColor {
    match color {
        Color::Red => theme::ERROR(),
        Color::White => theme::TEXT_BRIGHT(),
    }
}

/// One die as a numbered tile. The die-face glyphs (⚀..⚅) are unreadably
/// small at terminal font sizes, so the roll reads as a bold numeral on a
/// lit tile instead.
fn die_tile(n: u8) -> Span<'static> {
    Span::styled(
        format!(" {n} "),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    bg: &BackgammonDetail,
) {
    if area.width < GRID_W || area.height < GRID_H + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }
    let state = &bg.state;
    let my_color = state.color_of(daily.user_id());
    let seat = my_color.unwrap_or(Color::White);

    let show_rail = area.width >= GRID_W + INFO_RAIL_WIDTH + INFO_RAIL_MIN_EXTRA;
    let content = if show_rail {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_RAIL_WIDTH)])
            .split(area);
        draw_info_rail(frame, cols[1], state);
        cols[0]
    } else {
        area
    };
    let area = content;

    let stack_h = GRID_H + CHROME_ROWS;
    let top_pad = area.height.saturating_sub(stack_h) / 2;
    let rows = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(1),      // status
        Constraint::Length(1),      // opponent bar
        Constraint::Length(GRID_H), // the board
        Constraint::Length(1),      // own bar
        Constraint::Min(0),         // slack, pushing the hints to the floor
        Constraint::Length(1),      // key hints
    ])
    .split(area);
    let (status_row, top_bar, grid_row, bottom_bar, hint_row) =
        (rows[1], rows[2], rows[3], rows[4], rows[6]);

    let my_turn = detail.is_active()
        && detail.row.turn_user_id == Some(daily.user_id())
        && !bg.move_in_flight;

    // Legal turns for the mover, filtered down to what can still follow the
    // pending hops: lift-able checkers and, once one is lifted, its landings.
    let legal = if my_turn {
        state.legal_turns()
    } else {
        Vec::new()
    };
    let next_hops: Vec<backgammon::Hop> = legal
        .iter()
        .filter(|turn| turn.len() > bg.pending.len() && turn[..bg.pending.len()] == bg.pending[..])
        .map(|turn| turn[bg.pending.len()])
        .collect();
    let sources: HashSet<u8> = match bg.selected {
        Some(_) => HashSet::new(),
        None => next_hops.iter().map(|&(from, _)| from).collect(),
    };
    let dests: HashSet<u8> = match bg.selected {
        Some(lifted) => next_hops
            .iter()
            .filter(|&&(from, _)| from == lifted)
            .map(|&(_, to)| to)
            .collect(),
        None => HashSet::new(),
    };
    let hops_left = legal
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .saturating_sub(bg.pending.len());

    let grid_x = grid_row.x + grid_row.width.saturating_sub(GRID_W) / 2;
    let over_grid = |row: Rect| Rect {
        x: grid_x,
        y: row.y,
        width: GRID_W.min(row.width),
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, bg, hops_left))
            .alignment(Alignment::Center),
        status_row,
    );
    let bottom_color = seat;
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
        width: GRID_W,
        height: GRID_H,
    };
    let ctx = GridCtx {
        seat,
        view: state.preview(&bg.pending),
        cursor: my_turn.then_some(board.cursor),
        selected: bg.selected,
        sources,
        dests,
        roll: state.next_roll,
        hops_left: my_turn.then_some(hops_left),
    };
    frame.render_widget(Paragraph::new(grid_lines(&ctx)), grid_rect);
    // The hit-test area: the stack rows and both gap rows, header and footer
    // numbers excluded, so height / 2 splits exactly into the two halves.
    board.target_geometry.set(Some(Rect {
        x: grid_rect.x,
        y: grid_rect.y + 1,
        width: GRID_W,
        height: GRID_H - 2,
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

/// Everything one board render needs, so the row builders stay small.
struct GridCtx {
    seat: Color,
    view: Board,
    cursor: Option<usize>,
    selected: Option<u8>,
    sources: HashSet<u8>,
    dests: HashSet<u8>,
    roll: Option<[u8; 2]>,
    hops_left: Option<usize>,
}

impl GridCtx {
    /// The semantic target under a visual column of one half.
    fn code_at(&self, half: usize, col: usize) -> u8 {
        slot_target(half * SLOT_COLS + col, self.seat)
            .expect("column within the slot grid")
            .code()
    }

    /// Background for a column in one half: landing hints, the lifted
    /// checker, then the cursor on top — the same precedence the other
    /// boards use.
    fn column_bg(&self, half: usize, col: usize) -> Style {
        let code = self.code_at(half, col);
        let mut style = Style::default();
        if self.dests.contains(&code) {
            style = style.bg(theme::AMBER_DIM());
        }
        if self.selected == Some(code) {
            style = style.bg(theme::BG_SELECTION());
        }
        if self.cursor == Some(half * SLOT_COLS + col) {
            style = style.bg(theme::AMBER_DIM());
        }
        style
    }
}

fn grid_lines(ctx: &GridCtx) -> Vec<Line<'static>> {
    let mut lines = vec![number_line(ctx, 0)];
    for sub in 0..HALF_ROWS {
        lines.push(stack_line(ctx, 0, sub));
    }
    lines.push(gap_line(ctx, 0));
    lines.push(gap_line(ctx, 1));
    for sub in 0..HALF_ROWS {
        lines.push(stack_line(ctx, 1, sub));
    }
    lines.push(number_line(ctx, 1));
    lines
}

/// The point numbers over/under one half, in the seat's own 1..24 counting;
/// lift-able points glow amber, the cursor column reads bold.
fn number_line(ctx: &GridCtx, half: usize) -> Line<'static> {
    let mut spans = Vec::new();
    for col in 0..SLOT_COLS {
        let slot = half * SLOT_COLS + col;
        // The bar column wears the divider glyph, not a word: "bar" jammed
        // between two-digit point numbers reads as one blob.
        let (label, code) = match slot_target(slot, ctx.seat).expect("column within the slot grid")
        {
            BgTarget::Bar => ("│".to_string(), backgammon::BAR),
            BgTarget::Off => ("off".to_string(), backgammon::OFF),
            BgTarget::Point(p) => (point_name(ctx.seat, p as u8), p as u8),
        };
        let mut style = if ctx.sources.contains(&code) || ctx.dests.contains(&code) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        if ctx.cursor == Some(slot) {
            style = style
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(format!("{label:^0$}", COL_W as usize), style));
    }
    Line::from(spans)
}

/// One row of checker stacks in one half. Stacks grow toward the middle:
/// downward from the top numbers, upward from the bottom ones. A stack
/// taller than the half shows a count in its innermost cell.
fn stack_line(ctx: &GridCtx, half: usize, sub: u16) -> Line<'static> {
    // How deep into the stack this row reaches (0 = outer edge of the half).
    let depth = if half == 0 { sub } else { HALF_ROWS - 1 - sub };
    let mut spans = Vec::new();
    for col in 0..SLOT_COLS {
        let bg_style = ctx.column_bg(half, col);
        let span = match slot_target(half * SLOT_COLS + col, ctx.seat)
            .expect("column within the slot grid")
        {
            BgTarget::Bar => Span::styled(
                format!("{:^1$}", '│', COL_W as usize),
                bg_style.fg(theme::BORDER_DIM()),
            ),
            BgTarget::Off => {
                // The off tray: the top half holds the top player's borne-off
                // checkers, the bottom half the seat's.
                let color = if half == 0 {
                    ctx.seat.other()
                } else {
                    ctx.seat
                };
                stack_cell(ctx.view.off[off_idx(color)], color, depth, bg_style)
            }
            BgTarget::Point(p) => {
                let (color, count) = point_stack(&ctx.view, p);
                stack_cell(count, color, depth, bg_style)
            }
        };
        spans.push(span);
    }
    Line::from(spans)
}

fn off_idx(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Red => 1,
    }
}

/// Who owns a point and how many checkers sit there.
fn point_stack(view: &Board, point: usize) -> (Color, u8) {
    let n = view.points[point];
    if n >= 0 {
        (Color::White, n as u8)
    } else {
        (Color::Red, (-n) as u8)
    }
}

/// One cell of a checker stack: a checker at this depth, the overflow count
/// in the innermost cell of an over-tall stack, or empty.
fn stack_cell(count: u8, color: Color, depth: u16, bg_style: Style) -> Span<'static> {
    let text = if count as u16 > HALF_ROWS && depth == HALF_ROWS - 1 {
        format!("{count:^0$}", COL_W as usize)
    } else if (depth as u32) < count as u32 {
        format!("{CHECKER:^0$}", COL_W as usize)
    } else {
        " ".repeat(COL_W as usize)
    };
    Span::styled(text, bg_style.fg(color_fg(color)).add_modifier(Modifier::BOLD))
}

/// The two rows between the halves: the pending roll as dice in the left
/// table half, how much of the turn is still to play in the right, and each
/// player's bar checkers in the bar column (the top row belongs to the top
/// player, like the off tray).
fn gap_line(ctx: &GridCtx, row: usize) -> Line<'static> {
    let side_w = (BAR_COL as u16 * COL_W) as usize;
    let bar_color = if row == 0 {
        ctx.seat.other()
    } else {
        ctx.seat
    };
    let bar_count = ctx.view.bar[off_idx(bar_color)];

    // Left half: the roll as dice tiles, on the top gap row only.
    let mut spans = Vec::new();
    match (row, ctx.roll) {
        (0, Some(roll)) => {
            let pad = side_w.saturating_sub(7); // two 3-wide tiles + the gap
            spans.push(Span::raw(" ".repeat(pad - pad / 2)));
            spans.push(die_tile(roll[0]));
            spans.push(Span::raw(" "));
            spans.push(die_tile(roll[1]));
            spans.push(Span::raw(" ".repeat(pad / 2)));
        }
        (0, None) => spans.push(Span::styled(
            format!("{:^side_w$}", "rolling…"),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        _ => spans.push(Span::raw(" ".repeat(side_w))),
    }
    // Right half: the hops still owed this turn, mover only.
    let right = match (row, ctx.hops_left) {
        (0, Some(left)) if left > 0 => format!("{left} to play"),
        _ => String::new(),
    };
    let bar_bg = ctx.column_bg(row, BAR_COL);
    let bar_text = if bar_count > 0 {
        format!("{CHECKER}{bar_count}")
    } else {
        "│".to_string()
    };
    spans.push(Span::styled(
        format!("{bar_text:^0$}", COL_W as usize),
        if bar_count > 0 {
            bar_bg.fg(color_fg(bar_color)).add_modifier(Modifier::BOLD)
        } else {
            bar_bg.fg(theme::BORDER_DIM())
        },
    ));
    spans.push(Span::styled(
        format!("{right:^side_w$}"),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    // The off column stays quiet in the gap rows.
    spans.push(Span::raw(" ".repeat(COL_W as usize)));
    Line::from(spans)
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    bg: &BackgammonDetail,
    hops_left: usize,
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
        if bg.move_in_flight {
            spans.push(Span::styled(
                "Moving…",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else if detail.row.turn_user_id == Some(daily.user_id()) {
            let text = if bg.selected.is_some() {
                "Pick a landing · Esc cancels".to_string()
            } else if !bg.pending.is_empty() {
                format!("{hops_left} to play · Esc cancels")
            } else {
                "Your move".to_string()
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
        if let Some(turn) = bg.state.last_turn() {
            spans.push(Span::styled(
                format!("   last {}", turn_text(&bg.state, turn)),
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

/// `52: 13/8 13/11` for the most recent recorded turn (hit stars are not
/// reconstructed; the salvo-by-salvo record lives in the state, not here).
fn turn_text(state: &DailyBackgammonState, turn: &Turn) -> String {
    // The last turn's mover: the opposite parity of the side now to move.
    let color = if state.move_count() % 2 == 1 {
        Color::White
    } else {
        Color::Red
    };
    let dice = format!("{}{}", turn.roll[0], turn.roll[1]);
    if turn.hops.is_empty() {
        format!("{dice}: no play")
    } else {
        let hops: Vec<String> = turn
            .hops
            .iter()
            .map(|&(from, to)| format!("{}/{}", point_name(color, from), point_name(color, to)))
            .collect();
        format!("{dice}: {}", hops.join(" "))
    }
}

/// `● white mira · 3 off · 121 pips`, with the running deadline on the
/// mover's bar.
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyBackgammonState,
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
    let view = state.board();
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
            format!(
                "   {} off · {} pips",
                view.off[off_idx(color)],
                view.pip_count(color)
            ),
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
        hint(&mut spans, "Space/Enter", "lift / land");
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

fn draw_info_rail(frame: &mut Frame, area: Rect, state: &DailyBackgammonState) {
    let view = state.board();
    let side = |color: Color| {
        Line::from(vec![
            Span::styled(
                format!("{CHECKER} {:<6} ", color.label()),
                Style::default().fg(color_fg(color)),
            ),
            Span::styled(
                format!(
                    "{} off · {} pips",
                    view.off[off_idx(color)],
                    view.pip_count(color)
                ),
                Style::default().fg(theme::TEXT()),
            ),
        ])
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Correspondence backgammon".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "bear off all fifteen".to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Race".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        side(Color::White),
        side(Color::Red),
        Line::raw(""),
        Line::from(Span::styled(
            "How to".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
    ];
    for text in [
        "Your checkers run 24→1.",
        "Each die moves one",
        "checker that many pips.",
        "Lift a lit point, then",
        "pick a landing.",
        "A lone enemy checker is",
        "hit to the bar; it must",
        "re-enter first.",
        "Bear off all fifteen",
        "once everyone is home.",
    ] {
        lines.push(Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    let bars = (
        view.bar[off_idx(Color::White)],
        view.bar[off_idx(Color::Red)],
    );
    if bars.0 > 0 || bars.1 > 0 {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("bar: white {} · red {}", bars.0, bars.1),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
