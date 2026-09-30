//! The live board: one daily match painted small, for the #lounge strip
//! (`live_strip.rs`). Grid boards as two-column cells; pool on its own tiny
//! half-block table (`fit_cloth`) with the shooter's guide and cue stick
//! while an aim is fresh.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;
use crate::app::games::{
    cards::CardSuit,
    chess_core::types::{ChessColor, ChessPiece, piece_glyph},
    pool_core::{
        ball::CUE,
        canvas::{Canvas, Rgb},
        table::TableSpec,
        table_ui::{CLOTH, GUIDE, POCKET, RAIL, SURROUND, ball_colour},
    },
};

use super::{
    backgammon, battleship, briscola, checkers, connect4,
    live::{LiveBall, LiveBoard, LiveShot, LiveTrick, LiveView},
    pool::PoolAimShare,
    reversi, std_deck,
    svc::DailyMatchItem,
};

/// Tall enough for a chess or checkers board at one row per rank; every
/// other game centres in it.
pub(crate) const BOARD_ROWS: u16 = crate::app::live::ui::PICTURE_ROWS;

/// `8ball eggy v weslin`, the player on the move in amber; `eggy is aiming`
/// while they line up.
pub(crate) fn live_compact_line(width: u16, view: &LiveView<'_>) -> Line<'static> {
    let item = view.item;
    if view.aim.is_some() {
        let shooter = if item.turn_user_id == Some(item.opponent_id) {
            &item.opponent_username
        } else {
            &item.challenger_username
        };
        return Line::from(Span::styled(
            truncate_chars(&format!("{} is aiming", name(shooter)), width as usize),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let label = item.game.label();
    let budget = (width as usize).saturating_sub(label.chars().count() + 1 + 3) / 2;
    let styled = |user_id: uuid::Uuid, username: &Option<String>| {
        let style = if item.turn_user_id == Some(user_id) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        Span::styled(truncate_chars(&name(username), budget), style)
    };
    Line::from(vec![
        Span::styled(format!("{label} "), Style::default().fg(theme::AMBER_DIM())),
        styled(item.challenger_id, &item.challenger_username),
        Span::styled(" v ", Style::default().fg(theme::TEXT_FAINT())),
        styled(item.opponent_id, &item.opponent_username),
    ])
}

/// The pool canvas paints every cell's background, so it borrows the
/// theme's canvas colour to sit flush on the card. A theme without an RGB
/// canvas gets the table's own surround.
pub(crate) fn canvas_background() -> Rgb {
    match theme::BG_CANVAS() {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => SURROUND,
    }
}

fn name(username: &Option<String>) -> String {
    username.clone().unwrap_or_else(|| "player".to_string())
}

pub(crate) fn board_lines(width: u16, view: &LiveView<'_>, background: Rgb) -> Vec<Line<'static>> {
    let item = view.item;
    match view.board {
        LiveBoard::Chess { pieces, last } => chess_lines(width, pieces, *last),
        LiveBoard::ConnectFour { grid, last } => connect4_lines(width, grid, *last),
        LiveBoard::Reversi { grid, last } => reversi_lines(width, grid, *last),
        LiveBoard::Checkers { grid, last } => checkers_lines(width, grid, *last),
        LiveBoard::Pool {
            spec, balls, cue, ..
        } => pool_lines(width, spec, balls, *cue, view.aim, background),
        LiveBoard::Battleship {
            side0_id,
            shots,
            sunk,
            last,
        } => battleship_lines(width, item, *side0_id, shots, *sunk, *last),
        LiveBoard::Backgammon {
            board,
            turn,
            roll,
            landed,
            ..
        } => backgammon_lines(width, board, *turn, *roll, landed),
        LiveBoard::Briscola {
            seat0_id,
            held,
            stock_remaining,
            trump,
            trick,
            ..
        } => {
            let top = top_seat(item, *seat0_id);
            briscola_lines(
                width,
                [held[top], held[1 - top]],
                *stock_remaining,
                *trump,
                *trick,
            )
        }
        LiveBoard::Cribbage {
            seat0_id,
            scores,
            back,
            ..
        } => {
            let top = top_seat(item, *seat0_id);
            cribbage_lines(
                width,
                [scores[top], scores[1 - top]],
                [back[top], back[1 - top]],
            )
        }
        LiveBoard::GinRummy {
            seat0_id,
            held,
            discard,
            stock,
            ..
        } => {
            let top = top_seat(item, *seat0_id);
            gin_lines(width, [held[top], held[1 - top]], *discard, *stock)
        }
    }
}

/// The seat the challenger sits in: the card games and the pegboard draw
/// the challenger on top, as the words beside them name the challenger
/// first.
pub(crate) fn top_seat(item: &DailyMatchItem, seat0_id: uuid::Uuid) -> usize {
    if seat0_id == item.challenger_id { 0 } else { 1 }
}

/// A coloured mark before each player's name, challenger first, for the
/// games whose picture tells the players apart by colour.
pub(crate) fn player_marks(view: &LiveView<'_>) -> Option<[Span<'static>; 2]> {
    let item = view.item;
    let mark = |fg: Color| Span::styled("● ", Style::default().fg(fg));
    match view.board {
        LiveBoard::Backgammon { white_id, .. } => {
            if *white_id == item.challenger_id {
                Some([mark(BG_WHITE), mark(BG_RED)])
            } else {
                Some([mark(BG_RED), mark(BG_WHITE)])
            }
        }
        LiveBoard::Cribbage { .. } => Some([mark(PEGS[0].0), mark(PEGS[1].0)]),
        LiveBoard::Chess { .. }
        | LiveBoard::Battleship { .. }
        | LiveBoard::ConnectFour { .. }
        | LiveBoard::Reversi { .. }
        | LiveBoard::Checkers { .. }
        | LiveBoard::Briscola { .. }
        | LiveBoard::GinRummy { .. }
        | LiveBoard::Pool { .. } => None,
    }
}

/// Pad a row so it sits in the middle of `width`.
fn centred(width: u16, mut spans: Vec<Span<'static>>) -> Line<'static> {
    let drawn: usize = spans.iter().map(|span| span.width()).sum();
    let pad = (width as usize).saturating_sub(drawn) / 2;
    spans.insert(0, Span::raw(" ".repeat(pad)));
    Line::from(spans)
}

// ── Grid boards ───────────────────────────────────────────────

/// One board cell: a glyph on a square, two columns wide so a board of
/// eight reads as square.
struct Cell {
    glyph: char,
    fg: Color,
    bg: Color,
}

fn grid_line(width: u16, cells: impl Iterator<Item = Cell>) -> Line<'static> {
    let cells: Vec<Cell> = cells.collect();
    let pad = (width as usize).saturating_sub(cells.len() * 2) / 2;
    let mut spans = vec![Span::raw(" ".repeat(pad))];
    for cell in cells {
        spans.push(Span::styled(
            format!("{} ", cell.glyph),
            Style::default()
                .fg(cell.fg)
                .bg(cell.bg)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

const LIGHT_SQUARE: Color = Color::Rgb(240, 217, 181);
const DARK_SQUARE: Color = Color::Rgb(181, 136, 99);
const LIGHT_MOVED: Color = Color::Rgb(205, 210, 106);
const DARK_MOVED: Color = Color::Rgb(170, 162, 58);
const WHITE_PIECE: Color = Color::Rgb(252, 252, 248);
const BLACK_PIECE: Color = Color::Rgb(18, 18, 20);

/// White at the bottom, as the spectate board opens.
fn chess_lines(
    width: u16,
    pieces: &[Option<ChessPiece>; 64],
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    (0..8)
        .rev()
        .map(|rank| {
            grid_line(
                width,
                (0..8).map(move |file| {
                    let index = rank * 8 + file;
                    let light = (rank + file) % 2 == 1;
                    let moved = last.is_some_and(|(from, to)| index == from || index == to);
                    let bg = match (light, moved) {
                        (true, false) => LIGHT_SQUARE,
                        (false, false) => DARK_SQUARE,
                        (true, true) => LIGHT_MOVED,
                        (false, true) => DARK_MOVED,
                    };
                    match pieces[index] {
                        Some(piece) => Cell {
                            glyph: piece_glyph(piece.kind),
                            fg: match piece.color {
                                ChessColor::White => WHITE_PIECE,
                                ChessColor::Black => BLACK_PIECE,
                            },
                            bg,
                        },
                        None => Cell {
                            glyph: ' ',
                            fg: bg,
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

const DISC: char = '●';
const KING: char = '◉';
const LAST_CELL: Color = Color::Rgb(150, 132, 60);

/// Row 0 is the bottom of the frame (`connect4::grid` fills upward), so it
/// is drawn last.
fn connect4_lines(
    width: u16,
    grid: &connect4::Grid,
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    const FRAME: Color = Color::Rgb(34, 72, 158);
    (0..connect4::ROWS)
        .rev()
        .map(|row| {
            grid_line(
                width,
                (0..connect4::COLS).map(move |col| {
                    let bg = if last == Some((row, col)) {
                        LAST_CELL
                    } else {
                        FRAME
                    };
                    match grid[row][col] {
                        Some(connect4::Disc::Red) => Cell {
                            glyph: DISC,
                            fg: Color::Rgb(222, 52, 42),
                            bg,
                        },
                        Some(connect4::Disc::Yellow) => Cell {
                            glyph: DISC,
                            fg: Color::Rgb(242, 200, 44),
                            bg,
                        },
                        None => Cell {
                            glyph: '·',
                            fg: Color::Rgb(70, 106, 190),
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

/// Row 0 at the top, as the full board draws it.
fn reversi_lines(
    width: u16,
    grid: &reversi::Grid,
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    const FELT: Color = Color::Rgb(32, 112, 64);
    (0..reversi::SIZE)
        .map(|row| {
            grid_line(
                width,
                (0..reversi::SIZE).map(move |col| {
                    let bg = if last == Some((row, col)) {
                        LAST_CELL
                    } else {
                        FELT
                    };
                    match grid[row][col] {
                        Some(reversi::Disc::Black) => Cell {
                            glyph: DISC,
                            fg: BLACK_PIECE,
                            bg,
                        },
                        Some(reversi::Disc::White) => Cell {
                            glyph: DISC,
                            fg: WHITE_PIECE,
                            bg,
                        },
                        None => Cell {
                            glyph: ' ',
                            fg: bg,
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

/// Row 0 at the top, as the full board draws it.
fn checkers_lines(
    width: u16,
    grid: &checkers::Grid,
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    (0..checkers::SIZE)
        .map(|row| {
            grid_line(
                width,
                (0..checkers::SIZE).map(move |col| {
                    let bg = if last == Some((row, col)) {
                        LAST_CELL
                    } else if (row + col) % 2 == 1 {
                        DARK_SQUARE
                    } else {
                        LIGHT_SQUARE
                    };
                    match grid[row][col] {
                        Some(piece) => Cell {
                            glyph: if piece.king { KING } else { DISC },
                            fg: match piece.color {
                                checkers::Color::Red => Color::Rgb(200, 36, 36),
                                checkers::Color::White => WHITE_PIECE,
                            },
                            bg,
                        },
                        None => Cell {
                            glyph: ' ',
                            fg: bg,
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

// ── Pool ──────────────────────────────────────────────────────

/// The cloth's place on the canvas, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cloth {
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
}

/// Fit the cloth to a canvas `cols` wide and `px_h` pixels tall: a
/// one-pixel rail and one pixel of floor either side, the height following
/// the table's aspect. The full renderer's `View` cannot go this small: it
/// holds every ball at three pixels across, which on a table this size
/// would be a third of the cloth.
pub(crate) fn fit_cloth(spec: &TableSpec, cols: u16, px_h: u16) -> Cloth {
    let w = (cols as i32 - 4).max(1);
    let h = ((w as f64 * spec.width / spec.length).round() as i32).clamp(1, px_h as i32 - 2);
    Cloth {
        x0: 2,
        y0: (px_h as i32 - h) / 2,
        w,
        h,
    }
}

impl Cloth {
    /// Table metres to canvas pixels, fractional.
    fn to_px(self, spec: &TableSpec, at: [f64; 2]) -> (f64, f64) {
        (
            self.x0 as f64 + at[0] / spec.length * self.w as f64,
            self.y0 as f64 + at[1] / spec.width * self.h as f64,
        )
    }

    /// The pixel a ball lands on: never off the cloth, even at the cushion.
    fn cell(self, spec: &TableSpec, at: [f64; 2]) -> (i32, i32) {
        let (x, y) = self.to_px(spec, at);
        (
            (x.floor() as i32).clamp(self.x0, self.x0 + self.w - 1),
            (y.floor() as i32).clamp(self.y0, self.y0 + self.h - 1),
        )
    }
}

const CUE_STICK: Rgb = [206, 172, 118];

fn pool_lines(
    width: u16,
    spec: &TableSpec,
    balls: &[LiveBall],
    cue: Option<[f64; 2]>,
    aim: Option<&PoolAimShare>,
    background: Rgb,
) -> Vec<Line<'static>> {
    let mut canvas = Canvas::new(width, BOARD_ROWS, background);
    let cloth = fit_cloth(spec, width, canvas.height());
    let (x0, y0, x1, y1) = (
        cloth.x0,
        cloth.y0,
        cloth.x0 + cloth.w - 1,
        cloth.y0 + cloth.h - 1,
    );
    canvas.fill_rect(x0 - 1, y0 - 1, x1 + 1, y1 + 1, RAIL);
    canvas.fill_rect(x0, y0, x1, y1, CLOTH);
    let mid = x0 + cloth.w / 2;
    for (x, y) in [
        (x0 - 1, y0 - 1),
        (mid, y0 - 1),
        (x1 + 1, y0 - 1),
        (x0 - 1, y1 + 1),
        (mid, y1 + 1),
        (x1 + 1, y1 + 1),
    ] {
        canvas.set(x, y, POCKET);
    }

    // Ball in hand puts the cue ball where the shooter is carrying it.
    let cue = aim.and_then(|aim| aim.place).or(cue);
    let cue_px = cue.map(|at| cloth.to_px(spec, at));
    let dir = aim.and_then(|aim| {
        let (sin, cos) = aim.azimuth.sin_cos();
        let (dx, dy) = (
            cos * cloth.w as f64 / spec.length,
            sin * cloth.h as f64 / spec.width,
        );
        let len = (dx * dx + dy * dy).sqrt();
        (len > 0.0).then_some((dx / len, dy / len, aim.pull.clamp(0.0, 1.0)))
    });

    // The guide goes under the balls, as on the full table: a ball in the
    // way covers it.
    if let (Some((cx, cy)), Some((dx, dy, _))) = (cue_px, dir) {
        canvas.line(
            (cx + dx * 1.5, cy + dy * 1.5),
            (cx + dx * 7.0, cy + dy * 7.0),
            GUIDE,
            1,
            1,
        );
    }
    for ball in balls {
        let (x, y) = cloth.cell(spec, ball.pos);
        canvas.set(x, y, ball_colour(ball.id));
    }
    if let Some(at) = cue {
        let (x, y) = cloth.cell(spec, at);
        canvas.set(x, y, ball_colour(CUE));
    }
    // The stick over everything, drawn back as the shooter pulls it.
    if let (Some((cx, cy)), Some((dx, dy, pull))) = (cue_px, dir) {
        let back = 1.5 + pull * 3.0;
        canvas.line(
            (cx - dx * back, cy - dy * back),
            (cx - dx * (back + 7.0), cy - dy * (back + 7.0)),
            CUE_STICK,
            1,
            0,
        );
    }
    canvas.to_lines()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('…');
    out
}

// ── Backgammon ────────────────────────────────────────────────

const BG_FRAME: Color = Color::Rgb(66, 42, 26);
const BG_FELT: Color = Color::Rgb(30, 48, 40);
const BG_POINT_A: Color = Color::Rgb(70, 110, 90);
const BG_POINT_B: Color = Color::Rgb(150, 120, 80);
/// The points the last turn landed on.
const BG_LANDED: Color = Color::Rgb(176, 140, 36);
pub(crate) const BG_WHITE: Color = Color::Rgb(250, 250, 245);
pub(crate) const BG_RED: Color = Color::Rgb(225, 45, 45);
const DIE_FACE: Color = Color::Rgb(240, 232, 212);
const DIE_PIP: Color = Color::Rgb(24, 24, 28);
/// Checker rows per half; a taller stack shows its count in the innermost.
const BG_DEPTH: usize = 3;
/// Frame, six points, bar, six points, frame, then the two-column off tray.
const BG_WIDTH: u16 = 1 + 6 + 1 + 6 + 1 + 2;

fn checker_colour(color: backgammon::Color) -> Color {
    match color {
        backgammon::Color::White => BG_WHITE,
        backgammon::Color::Red => BG_RED,
    }
}

fn side_index(color: backgammon::Color) -> usize {
    match color {
        backgammon::Color::White => 0,
        backgammon::Color::Red => 1,
    }
}

/// White's seat, as the spectate board opens: white's home bottom right,
/// red's top right, each colour's borne-off count in the tray on its side.
/// Three checker rows a half, the roll on the mover's side of the middle.
fn backgammon_lines(
    width: u16,
    board: &backgammon::Board,
    turn: backgammon::Color,
    roll: Option<[u8; 2]>,
    landed: &[u8],
) -> Vec<Line<'static>> {
    let frame = |text: String| Span::styled(text, Style::default().bg(BG_FRAME));
    let pad = Span::raw(" ".repeat((width.saturating_sub(BG_WIDTH) / 2) as usize));
    let mut lines = Vec::with_capacity(BOARD_ROWS as usize);
    for row in 0..BOARD_ROWS as usize {
        let mut spans = vec![pad.clone(), frame(" ".to_string())];
        match row {
            0..=2 | 5..=7 => {
                let top = row < 3;
                let depth = if top { row } else { 7 - row };
                for col in 0..backgammon::SLOT_COLS - 1 {
                    let slot = if top {
                        col
                    } else {
                        backgammon::SLOT_COLS + col
                    };
                    let span = match backgammon::slot_target(slot, backgammon::Color::White)
                        .expect("column within the slot grid")
                    {
                        backgammon::BgTarget::Point(point) => {
                            let point_col = if col < backgammon::BAR_COL {
                                col
                            } else {
                                col - 1
                            };
                            let light = (point_col % 2 == 0) == top;
                            point_cell(board, point, depth, top, light, landed)
                        }
                        backgammon::BgTarget::Bar => frame(" ".to_string()),
                        backgammon::BgTarget::Off => unreachable!("the off tray is drawn below"),
                    };
                    spans.push(span);
                }
                spans.push(frame(" ".to_string()));
                // The tray: red's off at the top edge, white's at the bottom.
                let off = match row {
                    0 => Some(backgammon::Color::Red),
                    7 => Some(backgammon::Color::White),
                    _ => None,
                };
                spans.push(match off {
                    Some(color) if board.off[side_index(color)] > 0 => Span::styled(
                        format!("{:>2}", board.off[side_index(color)]),
                        Style::default()
                            .fg(checker_colour(color))
                            .bg(BG_FRAME)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Some(_) | None => frame("  ".to_string()),
                });
            }
            _ => {
                // The middle: red's side then white's, each holding its
                // bar checkers and, for the mover, the roll.
                let color = if row == 3 {
                    backgammon::Color::Red
                } else {
                    backgammon::Color::White
                };
                let felt = || Span::styled(" ", Style::default().bg(BG_FELT));
                match roll {
                    Some([a, b]) if turn == color => {
                        let die = |n: u8| {
                            Span::styled(
                                n.to_string(),
                                Style::default()
                                    .fg(DIE_PIP)
                                    .bg(DIE_FACE)
                                    .add_modifier(Modifier::BOLD),
                            )
                        };
                        spans.extend([felt(), die(a), felt(), felt(), die(b), felt()]);
                    }
                    Some(_) | None => spans.extend((0..6).map(|_| felt())),
                }
                let on_bar = board.bar[side_index(color)];
                spans.push(match on_bar {
                    0 => frame(" ".to_string()),
                    1 => Span::styled(
                        "●",
                        Style::default()
                            .fg(checker_colour(color))
                            .bg(BG_FRAME)
                            .add_modifier(Modifier::BOLD),
                    ),
                    n => Span::styled(
                        count_glyph(n).to_string(),
                        Style::default()
                            .fg(checker_colour(color))
                            .bg(BG_FRAME)
                            .add_modifier(Modifier::BOLD),
                    ),
                });
                spans.extend((0..6).map(|_| felt()));
                spans.push(frame("   ".to_string()));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// One cell of one point: a checker, the count of an over-tall stack in
/// the innermost row, or the point itself, whose innermost row is a half
/// block so the column tapers like a triangle.
fn point_cell(
    board: &backgammon::Board,
    point: usize,
    depth: usize,
    top: bool,
    light: bool,
    landed: &[u8],
) -> Span<'static> {
    let n = board.points[point];
    let (color, count) = if n >= 0 {
        (backgammon::Color::White, n as usize)
    } else {
        (backgammon::Color::Red, n.unsigned_abs() as usize)
    };
    let bg = if landed.contains(&(point as u8)) {
        BG_LANDED
    } else if light {
        BG_POINT_B
    } else {
        BG_POINT_A
    };
    let checker = Style::default()
        .fg(checker_colour(color))
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let innermost = depth == BG_DEPTH - 1;
    if innermost && count > BG_DEPTH {
        return Span::styled(count_glyph(count as u8).to_string(), checker);
    }
    if depth < count {
        return Span::styled("●", checker);
    }
    if innermost {
        let tip = if top { "▀" } else { "▄" };
        return Span::styled(tip, Style::default().fg(bg).bg(BG_FELT));
    }
    Span::styled(" ", Style::default().bg(bg))
}

/// A stack count in one cell: the digit, or `+` past nine.
fn count_glyph(n: u8) -> char {
    match n {
        0..=9 => char::from(b'0' + n),
        _ => '+',
    }
}

// ── Battleship ────────────────────────────────────────────────

const SEA_A: Color = Color::Rgb(20, 48, 86);
const SEA_B: Color = Color::Rgb(26, 58, 100);
const MISS: Color = Color::Rgb(125, 145, 170);
const HIT: Color = Color::Rgb(228, 58, 40);
/// The newest shot, hit or miss.
const LAST_SHOT: Color = Color::Rgb(255, 196, 64);
const SHIP_AFLOAT: Color = Color::Rgb(150, 160, 175);

/// Both players' waters as their opponents charted them, the challenger's
/// on the left: hits and misses only, two cells a row in half blocks so a
/// ten by ten sea is square in five rows. Under each, the fleet, sunk
/// ships in red. The same secrets the spectate board keeps: never a ship.
fn battleship_lines(
    width: u16,
    item: &DailyMatchItem,
    side0_id: uuid::Uuid,
    shots: &[Vec<LiveShot>; 2],
    sunk: [usize; 2],
    last: Option<(usize, LiveShot)>,
) -> Vec<Line<'static>> {
    let grid = battleship::GRID;
    let challenger = top_seat(item, side0_id);
    // Left the challenger's waters, fired on by the opponent; right the
    // opponent's, fired on by the challenger.
    let attackers = [1 - challenger, challenger];
    let charts = attackers.map(|attacker| {
        let mut chart = [None; battleship::CELLS];
        for shot in &shots[attacker] {
            chart[shot.cell as usize] = Some(shot.hit);
        }
        chart
    });
    let colour = |side: usize, cell: usize| -> Color {
        if let Some((shooter, shot)) = last
            && shooter == attackers[side]
            && shot.cell as usize == cell
        {
            return LAST_SHOT;
        }
        match charts[side][cell] {
            Some(true) => HIT,
            Some(false) => MISS,
            None if (cell / grid + cell % grid).is_multiple_of(2) => SEA_A,
            None => SEA_B,
        }
    };
    let names = [&item.challenger_username, &item.opponent_username];
    let mut lines = Vec::with_capacity(BOARD_ROWS as usize);
    let mut label = Vec::new();
    for (side, username) in names.iter().enumerate() {
        if side == 1 {
            label.push(Span::raw(" "));
        }
        label.push(Span::styled(
            format!("{:^1$}", truncate_chars(&name(username), grid), grid),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    lines.push(centred(width, label));
    for row in 0..grid / 2 {
        let mut spans = Vec::new();
        for side in 0..2 {
            if side == 1 {
                spans.push(Span::raw(" "));
            }
            for col in 0..grid {
                let top = colour(side, 2 * row * grid + col);
                let bottom = colour(side, (2 * row + 1) * grid + col);
                spans.push(Span::styled("▀", Style::default().fg(top).bg(bottom)));
            }
        }
        lines.push(centred(width, spans));
    }
    let mut fleets = Vec::new();
    for (side, &attacker) in attackers.iter().enumerate() {
        if side == 1 {
            fleets.push(Span::raw("  "));
        }
        let ships = battleship::FLEET_LENGTHS.len();
        for ship in 0..ships {
            let fg = if ship < ships - sunk[attacker] {
                SHIP_AFLOAT
            } else {
                HIT
            };
            let gap = if ship + 1 < ships { " " } else { "" };
            fleets.push(Span::styled(format!("▰{gap}"), Style::default().fg(fg)));
        }
    }
    lines.push(centred(width, fleets));
    lines
}

// ── Cards (briscola, gin rummy) ───────────────────────────────

const CARD_FACE: Color = Color::Rgb(240, 232, 212);
const CARD_EDGE: Color = Color::Rgb(150, 140, 120);
const CARD_RED: Color = Color::Rgb(190, 30, 30);
const CARD_BLACK: Color = Color::Rgb(25, 25, 30);
const CARD_BACK: Color = Color::Rgb(44, 70, 130);
const CARD_WEAVE: Color = Color::Rgb(100, 130, 190);
/// The edge of the card that took the trick.
const CARD_WON: Color = Color::Rgb(230, 170, 40);

/// Three rows of spans, one card (or a gap) wide.
type Block = [Vec<Span<'static>>; 3];

fn suit_colour(suit: CardSuit) -> Color {
    match suit {
        CardSuit::Hearts | CardSuit::Diamonds => CARD_RED,
        CardSuit::Clubs | CardSuit::Spades => CARD_BLACK,
    }
}

/// `╭───╮ │A ♥│ ╰───╯` on an ivory face.
fn face_block(rank: &str, suit: CardSuit, edge: Color) -> Block {
    let edge = Style::default().fg(edge).bg(CARD_FACE);
    let pip = Style::default()
        .fg(suit_colour(suit))
        .bg(CARD_FACE)
        .add_modifier(Modifier::BOLD);
    [
        vec![Span::styled("╭───╮", edge)],
        vec![
            Span::styled("│", edge),
            Span::styled(format!("{rank:<2}{}", suit.symbol()), pip),
            Span::styled("│", edge),
        ],
        vec![Span::styled("╰───╯", edge)],
    ]
}

/// A face-down card, with a count in the middle for a stock.
fn back_block(count: Option<usize>) -> Block {
    let style = Style::default().fg(CARD_WEAVE).bg(CARD_BACK);
    let middle = match count {
        Some(count) => Span::styled(
            format!("│{count:^3}│"),
            style.fg(CARD_FACE).add_modifier(Modifier::BOLD),
        ),
        None => Span::styled("│░░░│", style),
    };
    [
        vec![Span::styled("╭───╮", style)],
        vec![middle],
        vec![Span::styled("╰───╯", style)],
    ]
}

/// Where a card would lie, dashed, with an optional mark inside.
fn slot_block(mark: Option<Span<'static>>) -> Block {
    let style = Style::default().fg(theme::BORDER_DIM());
    let inner = mark.unwrap_or_else(|| Span::raw("   "));
    [
        vec![Span::styled("╭┄┄┄╮", style)],
        vec![Span::styled("┆", style), inner, Span::styled("┆", style)],
        vec![Span::styled("╰┄┄┄╯", style)],
    ]
}

/// The left edge of a card lying under the one beside it.
fn under_block(face_up: bool) -> Block {
    let style = if face_up {
        Style::default().fg(CARD_EDGE).bg(CARD_FACE)
    } else {
        Style::default().fg(CARD_WEAVE).bg(CARD_BACK)
    };
    let middle = if face_up { "│ " } else { "│░" };
    [
        vec![Span::styled("╭─", style)],
        vec![Span::styled(middle, style)],
        vec![Span::styled("╰─", style)],
    ]
}

fn gap_block(width: usize) -> Block {
    let gap = || vec![Span::raw(" ".repeat(width))];
    [gap(), gap(), gap()]
}

fn block_lines(width: u16, blocks: Vec<Block>) -> Vec<Line<'static>> {
    let mut rows: [Vec<Span<'static>>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for block in blocks {
        for (row, spans) in rows.iter_mut().zip(block) {
            row.extend(spans);
        }
    }
    rows.into_iter().map(|row| centred(width, row)).collect()
}

/// A hand held face down: one back per card, how many and never which.
/// A short hand gets roomy backs; a long one fans them into slats.
fn hand_line(width: u16, held: usize) -> Line<'static> {
    let style = Style::default().fg(CARD_WEAVE).bg(CARD_BACK);
    let spans = if held <= 3 {
        (0..held)
            .flat_map(|card| {
                let gap = if card + 1 < held { " " } else { "" };
                [Span::styled("░░", style), Span::raw(gap)]
            })
            .collect()
    } else {
        (0..held).map(|_| Span::styled("▏", style)).collect()
    };
    centred(width, spans)
}

fn faint_line(width: u16, text: String) -> Line<'static> {
    centred(
        width,
        vec![Span::styled(text, Style::default().fg(theme::TEXT_FAINT()))],
    )
}

/// The challenger's hand on top, the opponent's at the bottom, and between
/// them the stock under the turned-up trump (just its suit once it is
/// drawn) beside the trick: the card led, or the last trick taken with the
/// winning card edged in gold.
fn briscola_lines(
    width: u16,
    held: [usize; 2],
    stock_remaining: usize,
    trump: briscola::Card,
    trick: LiveTrick,
) -> Vec<Line<'static>> {
    let face = |card: briscola::Card, edge: Color| face_block(card.rank.label(), card.suit, edge);
    let stock = if stock_remaining > 1 {
        under_block(false)
    } else {
        gap_block(2)
    };
    let trump = if stock_remaining > 0 {
        face(trump, CARD_EDGE)
    } else {
        slot_block(Some(Span::styled(
            format!(" {} ", trump.suit.symbol()),
            Style::default().fg(suit_colour(trump.suit)),
        )))
    };
    let (first, second) = match trick {
        LiveTrick::Empty => (slot_block(None), slot_block(None)),
        LiveTrick::Led { card } => (face(card, CARD_EDGE), slot_block(None)),
        LiveTrick::Taken {
            lead,
            answer,
            lead_won,
        } => {
            let (lead_edge, answer_edge) = if lead_won {
                (CARD_WON, CARD_EDGE)
            } else {
                (CARD_EDGE, CARD_WON)
            };
            (face(lead, lead_edge), face(answer, answer_edge))
        }
    };
    let stock_label = if stock_remaining > 0 {
        format!("{stock_remaining:>2} left")
    } else {
        String::new()
    };
    let mut lines = vec![hand_line(width, held[0]), Line::from("")];
    lines.extend(block_lines(
        width,
        vec![stock, trump, gap_block(2), first, gap_block(1), second],
    ));
    let mut label = vec![Span::styled(
        format!("{stock_label:<7}"),
        Style::default().fg(theme::TEXT_FAINT()),
    )];
    label.push(Span::raw(" ".repeat(13)));
    lines.push(centred(width, label));
    lines.push(Line::from(""));
    lines.push(hand_line(width, held[1]));
    lines
}

/// The challenger's hand on top, the opponent's at the bottom, the stock
/// and the face-up top of the pile between them.
fn gin_lines(
    width: u16,
    held: [usize; 2],
    discard: Option<(std_deck::Card, bool)>,
    stock: usize,
) -> Vec<Line<'static>> {
    let stock_block = if stock > 0 {
        back_block(Some(stock))
    } else {
        slot_block(None)
    };
    let (under, top) = match discard {
        Some((card, more)) => (
            if more {
                under_block(true)
            } else {
                gap_block(2)
            },
            face_block(card.rank.label(), card.suit, CARD_EDGE),
        ),
        None => (gap_block(2), slot_block(None)),
    };
    let mut lines = vec![hand_line(width, held[0]), Line::from("")];
    lines.extend(block_lines(
        width,
        vec![stock_block, gap_block(3), under, top],
    ));
    lines.push(faint_line(width, "stock      pile".to_string()));
    lines.push(Line::from(""));
    lines.push(hand_line(width, held[1]));
    lines
}

// ── Cribbage ──────────────────────────────────────────────────

const WOOD_A: Color = Color::Rgb(122, 84, 50);
const WOOD_B: Color = Color::Rgb(134, 94, 58);
const HOLE: Color = Color::Rgb(70, 46, 28);
/// Front and back peg colours: the challenger's lane, then the opponent's.
const PEGS: [(Color, Color); 2] = [
    (Color::Rgb(240, 70, 56), Color::Rgb(150, 52, 44)),
    (Color::Rgb(90, 170, 245), Color::Rgb(56, 100, 145)),
];
/// Holes per street: three streets of twenty make the sixty, the finish
/// hole at the end of the last.
const STREET: u32 = 20;

/// A sixty-one hole board: three streets, a lane each, the challenger's on
/// top. Every peg is public: the front peg on the score, the back peg
/// where the latest peg jumped from.
fn cribbage_lines(width: u16, scores: [u32; 2], back: [u32; 2]) -> Vec<Line<'static>> {
    let finish = super::cribbage::WINNING_SCORE;
    let board_width = STREET as usize + 1;
    let blank = || {
        centred(
            width,
            vec![Span::styled(
                " ".repeat(board_width),
                Style::default().bg(WOOD_A),
            )],
        )
    };
    let mut lines = Vec::with_capacity(BOARD_ROWS as usize);
    for street in 0..3 {
        if street > 0 {
            lines.push(blank());
        }
        for lane in 0..2 {
            let (front_fg, back_fg) = PEGS[lane];
            let mut spans = Vec::with_capacity(board_width);
            for col in 0..STREET {
                let hole = street * STREET + col + 1;
                let bg = if (col / 5).is_multiple_of(2) {
                    WOOD_A
                } else {
                    WOOD_B
                };
                let (glyph, fg) = if scores[lane] == hole {
                    ("●", front_fg)
                } else if back[lane] == hole {
                    ("●", back_fg)
                } else {
                    ("·", HOLE)
                };
                spans.push(Span::styled(
                    glyph,
                    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(if street == 2 {
                let (glyph, fg) = if scores[lane] >= finish {
                    ("●", front_fg)
                } else {
                    ("○", HOLE)
                };
                Span::styled(
                    glyph,
                    Style::default()
                        .fg(fg)
                        .bg(WOOD_A)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(" ", Style::default().bg(WOOD_A))
            });
            lines.push(centred(width, spans));
        }
    }
    lines
}

#[cfg(test)]
#[path = "live_board_test.rs"]
mod live_board_test;
